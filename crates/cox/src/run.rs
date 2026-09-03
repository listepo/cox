//! `cox run -p`: the headless surface (plan §1.12, T6.1). One consumer of
//! the same `Event` stream the TUI reads, printed in one of three shapes;
//! the exit code tells a script what happened without parsing anything.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Duration;

use cox_core::Session;
use cox_protocol::Event;
use cox_protocol::ids::{CallId, ItemId, SessionId};
use cox_protocol::types::{ApprovalPolicy, Decision, ItemKind, StopReason, Submission};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::cli::{Cli, RunArgs};
use crate::{resume, session};

/// Exit codes from §1.12: `0 ok · 1 error · 2 denied · 3 budget · 4 interrupted`.
pub const EXIT_OK: i32 = 0;
pub const EXIT_ERROR: i32 = 1;
pub const EXIT_DENIED: i32 = 2;
pub const EXIT_BUDGET: i32 = 3;
pub const EXIT_INTERRUPTED: i32 = 4;

#[derive(Clone, Copy, PartialEq)]
enum Format {
    Text,
    Json,
    StreamJson,
}

/// What one headless turn produced, folded from the event stream.
#[derive(Default)]
struct Outcome {
    session: Option<SessionId>,
    texts: HashMap<ItemId, String>,
    result: String,
    // input, output, cache read, cache write — summed over provider calls.
    tokens: [u32; 4],
    cost_usd: f64,
    turns: u32,
    denied: u32,
    stop: Option<StopReason>,
    failed: bool,
}

impl Outcome {
    fn exit_code(&self) -> i32 {
        match &self.stop {
            _ if self.failed => EXIT_ERROR,
            None | Some(StopReason::Error) => EXIT_ERROR,
            Some(StopReason::Refusal { .. }) => EXIT_DENIED,
            Some(StopReason::Budget) => EXIT_BUDGET,
            Some(StopReason::Interrupted) => EXIT_INTERRUPTED,
            Some(StopReason::EndTurn | StopReason::MaxTurns) if self.denied > 0 => EXIT_DENIED,
            Some(StopReason::EndTurn | StopReason::MaxTurns) => EXIT_OK,
        }
    }

    /// The `json` shape and the `stream-json` `result` alias share one body.
    fn summary(&self) -> Value {
        let session = self.session.map(|s| s.to_string());
        json!({
            "session": session,
            "result": self.result,
            "usage": {
                "input_tokens": self.tokens[0],
                "output_tokens": self.tokens[1],
                "cache_read_tokens": self.tokens[2],
                "cache_write_tokens": self.tokens[3],
            },
            "cost_usd": self.cost_usd,
            "turns": self.turns,
            "stop": self.stop,
            "denied": self.denied,
            "exit_code": self.exit_code(),
        })
    }

    /// Folds one event in; returns a Claude-compatible alias line to print
    /// under `stream-json`, when this event has one.
    fn fold(&mut self, ev: &Event) -> Option<Value> {
        match ev {
            Event::SessionStarted { session, .. } => self.session = Some(*session),
            Event::ItemStarted {
                item,
                kind: ItemKind::AssistantMessage { text },
            } => {
                self.texts.insert(*item, text.clone());
            }
            Event::TextDelta { item, text } => {
                if let Some(buf) = self.texts.get_mut(item) {
                    buf.push_str(text);
                }
            }
            Event::ItemDone { item } => {
                if let Some(text) = self.texts.remove(item) {
                    self.result = text.clone();
                    return Some(json!({
                        "type": "assistant",
                        "message": { "role": "assistant", "content": [{ "type": "text", "text": text }] },
                        "session_id": self.session.map(|s| s.to_string()),
                    }));
                }
            }
            Event::Usage { usage, .. } => {
                self.turns += 1;
                self.tokens[0] += usage.input_tokens;
                self.tokens[1] += usage.output_tokens;
                self.tokens[2] += usage.cache_read_tokens;
                self.tokens[3] += usage.cache_write_tokens;
                self.cost_usd += usage.cost_usd;
            }
            Event::ApprovalDecided {
                decision: Decision::Deny { .. },
                ..
            } => self.denied += 1,
            Event::TurnDone { stop, .. } => self.stop = Some(stop.clone()),
            Event::Error { fatal: true, .. } => self.failed = true,
            _ => {}
        }
        None
    }
}

/// Runs one prompt to completion and returns the process exit code.
/// Without `-p` this is the resume/continue listing from T2.4.
pub fn run(cli: &Cli, args: &RunArgs, cwd: &Path) -> anyhow::Result<i32> {
    let Some(prompt) = args.prompt.clone() else {
        resume::run(cli, args)?;
        return Ok(EXIT_OK);
    };
    let format = match args.output_format.as_deref().unwrap_or("text") {
        "text" => Format::Text,
        "json" => Format::Json,
        "stream-json" => Format::StreamJson,
        other => anyhow::bail!("unknown --output-format `{other}` (text | json | stream-json)"),
    };
    // Headless defaults to `never`: nobody is there to answer an ask.
    let approve_default = cli.approve.is_none();
    let (session, loaded) = session::open(cli, cwd, args.answer.clone(), |config| {
        if approve_default {
            config.permissions.approval = ApprovalPolicy::Never;
        }
    })?;
    // T6.3: with any other policy a driver answers asks on stdin, within
    // `hooks.timeout_s`; `never` never asks, so stdin is left alone.
    let approvals = (loaded.config.permissions.approval != ApprovalPolicy::Never)
        .then(|| Duration::from_secs(u64::from(loaded.config.hooks.timeout_s)));
    let outcome =
        tokio::runtime::Runtime::new()?.block_on(drive(session, prompt, format, approvals))?;
    let mut out = std::io::stdout().lock();
    match format {
        Format::Text => writeln!(out, "{}", outcome.result)?,
        Format::Json => writeln!(out, "{}", outcome.summary())?,
        Format::StreamJson => {
            let mut result = outcome.summary();
            result["type"] = json!("result");
            result["is_error"] = json!(outcome.exit_code() != EXIT_OK);
            writeln!(out, "{result}")?;
        }
    }
    Ok(outcome.exit_code())
}

async fn drive(
    session: Session,
    prompt: String,
    format: Format,
    approvals: Option<Duration>,
) -> anyhow::Result<Outcome> {
    let mut rx = session
        .events()
        .ok_or_else(|| anyhow::anyhow!("session events already taken"))?;
    let interrupter = session.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            interrupter.interrupt();
        }
    });
    // The core runs the turn inside `submit`, so it must live on its own
    // task or nothing could answer an `ApprovalRequired` mid-turn.
    let turn = tokio::spawn({
        let session = session.clone();
        async move {
            session
                .submit(Submission::UserTurn {
                    text: prompt,
                    attachments: Vec::new(),
                    confirm_think: false,
                })
                .await
        }
    });
    let mut outcome = Outcome::default();
    let mut out = std::io::stdout().lock();
    let mut driver = approvals.map(|_| spawn_stdin());
    let mut pending: Vec<(CallId, Instant)> = Vec::new();
    loop {
        let deadline = pending.iter().map(|(_, at)| *at).min();
        let ev = tokio::select! {
            ev = rx.recv() => match ev {
                Some(ev) => ev,
                None => break,
            },
            line = recv_or_pend(&mut driver) => {
                match line {
                    Some((call_id, decision)) => {
                        pending.retain(|(id, _)| *id != call_id);
                        session.submit(Submission::Approve { call_id, decision }).await?;
                    }
                    // stdin closed: whatever is still pending times out.
                    None => driver = None,
                }
                continue;
            }
            _ = tokio::time::sleep_until(deadline.unwrap_or_else(Instant::now)), if deadline.is_some() => {
                let now = Instant::now();
                let (expired, kept): (Vec<_>, Vec<_>) = pending.drain(..).partition(|(_, at)| *at <= now);
                pending = kept;
                for (call_id, _) in expired {
                    let reason = format!("no decision from the driver within {}s", approvals.unwrap_or_default().as_secs());
                    session.submit(Submission::Approve { call_id, decision: Decision::Deny { reason } }).await?;
                }
                continue;
            }
        };
        if format == Format::StreamJson {
            writeln!(out, "{}", serde_json::to_string(&ev)?)?;
        }
        if let Event::ApprovalRequired { call, .. } = &ev {
            match approvals {
                Some(timeout) => pending.push((call.id, Instant::now() + timeout)),
                None => {
                    session
                        .submit(Submission::Approve {
                            call_id: call.id,
                            decision: Decision::Deny {
                                reason: "no approver in headless mode".into(),
                            },
                        })
                        .await?;
                }
            }
        }
        let alias = outcome.fold(&ev);
        if let (Format::StreamJson, Some(alias)) = (format, alias) {
            writeln!(out, "{alias}")?;
        }
        if matches!(
            ev,
            Event::TurnDone { .. } | Event::Error { fatal: true, .. }
        ) {
            break;
        }
    }
    if let Ok(Err(e)) = turn.await {
        return Err(e.into());
    }
    Ok(outcome)
}

/// One driver line: `{"approve":"<call_id>"}` or `{"deny":"<call_id>","reason":"…"}`.
#[derive(serde::Deserialize)]
struct DriverLine {
    approve: Option<String>,
    deny: Option<String>,
    reason: Option<String>,
}

fn decision_line(line: &str) -> Option<(CallId, Decision)> {
    let d: DriverLine = serde_json::from_str(line).ok()?;
    if let Some(id) = d.approve {
        return Some((id.parse().ok()?, Decision::Allow));
    }
    let id: CallId = d.deny?.parse().ok()?;
    let reason = d.reason.unwrap_or_else(|| "denied by driver".into());
    Some((id, Decision::Deny { reason }))
}

/// stdin is blocking; a thread turns its lines into decisions. Unparseable
/// lines are ignored so a stray log line cannot approve anything.
fn spawn_stdin() -> mpsc::Receiver<(CallId, Decision)> {
    let (tx, rx) = mpsc::channel(16);
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if let Some(decision) = decision_line(&line)
                && tx.blocking_send(decision).is_err()
            {
                break;
            }
        }
    });
    rx
}

/// Pends forever without a driver so `select!` never spins on a closed side.
async fn recv_or_pend<T>(rx: &mut Option<mpsc::Receiver<T>>) -> Option<T> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}
