//! A top-level session driven by an external ACP agent (T52.4, DT§3.3.1):
//! one agent process for the session's life, not one per turn as the
//! subagent driver in `external_agents` runs it. Here, beside that driver,
//! because it reuses the driver's spawn (env allowlist, process group,
//! reap), its `ClientHost` and its sandbox wrap, so there is still one way
//! an agent process starts.
//!
//! The surface gets a plain `Event` stream, folded by `cox_acp::UpdateFold`
//! from what the agent reports, with the `TurnStarted` and user item the
//! agent does not echo put in front of each prompt. ACP allows one prompt
//! in flight, so a prompt sent while one runs waits here in order.

use std::collections::VecDeque;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use agent_client_protocol::schema::v1::{
    CancelNotification, ContentBlock, NewSessionRequest, PromptRequest, PromptResponse,
    SessionId as AcpId, SessionUpdate,
};
use agent_client_protocol::{Agent, ByteStreams, Client, ConnectTo, ConnectionTo};
use cox_acp::{Approver, ClientHost, UpdateFold};
use cox_plugin::external_agent::ExternalAgentCommand;
use cox_plugin_api::AgentMode;
use cox_protocol::Config;
use cox_protocol::errors::{CoreError, ProviderError};
use cox_protocol::ids::{ItemId, TurnId};
use cox_protocol::types::{Event, ItemKind, Job, ModelId, Tier};
use tokio::process::Child;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_util::compat::{TokioAsyncReadCompatExt as _, TokioAsyncWriteCompatExt as _};

use crate::external_agents::{Reap, RefuseAsk, Spawn, acp_host, stderr_tail};

/// The core's own event bound (DT§4.5).
const EVENTS: usize = 256;
/// How long a failed prompt waits for the agent's stderr to close, so the
/// error can name what the agent said before it died.
const TAIL_WAIT: Duration = Duration::from_millis(500);

/// Why an agent session did not open.
#[derive(Debug, thiserror::Error)]
pub enum AcpOpenError {
    /// EA§7: its program is on no `PATH` directory, its key is not set, or
    /// its host could not be built. The one warning the surface shows.
    #[error("{0}")]
    Unavailable(String),
    /// The process started but `initialize` or `session/new` failed.
    #[error(transparent)]
    Agent(#[from] CoreError),
}

enum Input {
    Prompt(String),
    Cancel,
}

/// The running agent. Ending it, or dropping it, kills its process group.
pub struct AcpSession {
    agent: String,
    input: mpsc::UnboundedSender<Input>,
    process: Mutex<Option<(Child, Reap)>>,
}

/// A session just opened, and the events it will emit.
pub struct OpenedAcp {
    pub session: AcpSession,
    pub events: mpsc::Receiver<Event>,
}

impl AcpSession {
    /// The agent's name, as its entry declares it.
    pub fn agent(&self) -> &str {
        &self.agent
    }

    /// Sends `text` as the next prompt, after the one in flight if any.
    /// False once the agent is gone.
    pub fn prompt(&self, text: String) -> bool {
        self.input.send(Input::Prompt(text)).is_ok()
    }

    /// `session/cancel` for the prompt in flight; prompts waiting behind it
    /// are dropped. The turn ends when the agent answers `cancelled`.
    pub fn cancel(&self) -> bool {
        self.input.send(Input::Cancel).is_ok()
    }

    /// Kills the agent's process group now (closing the session, or quit).
    pub fn end(&self) {
        let process = self
            .process
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        drop(process);
    }
}

/// Every agent a top-level session may be driven by: the user config's
/// `[external_agents.<name>]` entries and the granted plugins'
/// `[[external_agents]]` entries, each already wrapped, plus one warning per
/// entry refused or plugin skipped.
pub fn agents(
    config: &Config,
    home: &Path,
    cwd: &Path,
    writable: &[PathBuf],
) -> (Vec<ExternalAgentCommand>, Vec<String>) {
    let (mut agents, mut warnings) =
        crate::external_agents::config_agents(config, writable, std::env::home_dir().as_deref());
    match cox_store::Store::open(home) {
        Ok(store) => {
            let plugins = crate::load_plugins(config, home, cwd, Arc::new(store), Some(writable));
            agents.extend(plugins.external_agents);
            warnings.extend(plugins.notices);
        }
        Err(e) => warnings.push(format!("plugin agents left out: {e}")),
    }
    (agents, warnings)
}

/// Starts `agent` in `cwd` under its wrap and opens an ACP session with it.
/// `path` is where a bare program name is looked up; `key` resolves the
/// entry's `key_env` (a test passes its own, never the OS keychain);
/// `approver` answers the engine's `Ask` verdicts, and without one they
/// are refused with the way out named, as for a subagent.
pub async fn open(
    agent: ExternalAgentCommand,
    config: &Config,
    cwd: &Path,
    writable: &[PathBuf],
    path: Option<&OsStr>,
    key: impl Fn(&str, &str) -> Result<String, ProviderError>,
    approver: Option<Arc<dyn Approver>>,
) -> Result<OpenedAcp, AcpOpenError> {
    let name = agent.name().to_string();
    let unavailable = |why: String| {
        AcpOpenError::Unavailable(format!("{name} ({}) cannot start: {why}", agent.origin()))
    };
    if agent.mode() != AgentMode::Acp {
        return Err(unavailable(String::from("it speaks stream-json, not ACP")));
    }
    if let Some(cli) = agent.missing_cli(path) {
        return Err(unavailable(format!("`{}` is not on PATH", cli.display())));
    }
    let Ok(secret) = key(agent.key_env(), &name) else {
        return Err(unavailable(format!("{} is not set", agent.key_env())));
    };
    let host = acp_host(config, cwd, writable).map_err(unavailable)?;
    let spawn = Spawn {
        name: name.clone(),
        key_env: agent.key_env().to_string(),
        mode: agent.mode(),
        agent,
        key: secret,
        cwd: cwd.to_path_buf(),
    };
    let (mut child, reap) = spawn.spawn(&[], Stdio::piped())?;
    let (Some(stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err(spawn.error("no stdio pipes".into()).into());
    };
    let tail = tokio::spawn(stderr_tail(stderr));
    let (tx, updates) = mpsc::unbounded_channel();
    let client = host.client(approver.unwrap_or_else(|| Arc::new(RefuseAsk)), tx);
    let transport = ByteStreams::new(stdin.compat_write(), stdout.compat());
    let mut opened = connect(name, transport, client, updates, Some(tail)).await?;
    opened.session.process = Mutex::new(Some((child, reap)));
    Ok(opened)
}

/// [`open`] over any transport: `initialize`, `session/new` in the host's
/// cwd, then the session loop. A test passes one end of a duplex channel.
/// `updates` is the receiving end of `host.updates`.
pub async fn connect(
    agent: String,
    transport: impl ConnectTo<Client> + Send + 'static,
    host: ClientHost,
    updates: mpsc::UnboundedReceiver<SessionUpdate>,
    tail: Option<JoinHandle<String>>,
) -> Result<OpenedAcp, CoreError> {
    let (input_tx, input) = mpsc::unbounded_channel();
    let (events_tx, events) = mpsc::channel(EVENTS);
    let (ready_tx, ready) = oneshot::channel::<Result<(), String>>();
    let (cwd, sandboxed) = (host.cwd.clone(), host.sandbox.is_some());
    let driver = Driver {
        fold: UpdateFold::new(agent.clone(), TurnId::new()),
        model: ModelId(format!("{agent} · ACP")),
        events: events_tx,
        tail,
        seq: 0,
        busy: false,
        waiting: VecDeque::new(),
        started: Instant::now(),
    };
    let run = cox_acp::connect(transport, host, async move |cx| {
        let setup = async {
            cx.send_request(cox_acp::initialize_request(sandboxed))
                .block_task()
                .await?;
            cx.send_request(NewSessionRequest::new(cwd))
                .block_task()
                .await
        };
        let session = match setup.await {
            Ok(session) => session.session_id,
            Err(e) => {
                let _ = ready_tx.send(Err(e.to_string()));
                return Err(e);
            }
        };
        let _ = ready_tx.send(Ok(()));
        driver.run(cx, session, input, updates).await;
        Ok(())
    });
    // The driver reports a failed prompt itself; a connection that fails
    // after it has no turn left to end.
    tokio::spawn(async move {
        let _ = run.await;
    });
    let failed = |message: String| CoreError::ExternalAgent {
        agent: agent.clone(),
        message: cox_sanitize::sanitize(&message),
    };
    match ready.await {
        Ok(Ok(())) => Ok(OpenedAcp {
            session: AcpSession {
                agent: agent.clone(),
                input: input_tx,
                process: Mutex::new(None),
            },
            events,
        }),
        Ok(Err(e)) => Err(failed(e)),
        Err(_) => Err(failed(String::from("closed before it answered"))),
    }
}

/// The session loop: owns the fold, so every event leaves in the order the
/// agent reported it.
struct Driver {
    fold: UpdateFold,
    model: ModelId,
    events: mpsc::Sender<Event>,
    tail: Option<JoinHandle<String>>,
    seq: u32,
    busy: bool,
    waiting: VecDeque<String>,
    started: Instant,
}

type Stopped = Result<PromptResponse, String>;

impl Driver {
    async fn run(
        mut self,
        cx: ConnectionTo<Agent>,
        session: AcpId,
        mut input: mpsc::UnboundedReceiver<Input>,
        mut updates: mpsc::UnboundedReceiver<SessionUpdate>,
    ) {
        let (stop_tx, mut stops) = mpsc::unbounded_channel::<Stopped>();
        loop {
            // Updates first: a prompt's response follows its last update on
            // the wire, so it must not overtake them here.
            let alive = tokio::select! {
                biased;
                Some(update) = updates.recv() => {
                    let events = self.fold.update(update, self.now());
                    self.emit(events).await
                }
                Some(stopped) = stops.recv() => {
                    let mut alive = self.stopped(&mut updates, stopped).await;
                    if alive && let Some(next) = self.waiting.pop_front() {
                        alive = self.start(&cx, &session, next, &stop_tx).await;
                    }
                    alive
                }
                cmd = input.recv() => match cmd {
                    None => false,
                    Some(Input::Prompt(text)) if self.busy => {
                        self.waiting.push_back(text);
                        true
                    }
                    Some(Input::Prompt(text)) => self.start(&cx, &session, text, &stop_tx).await,
                    Some(Input::Cancel) => {
                        self.waiting.clear();
                        if self.busy {
                            // An agent that is gone fails the prompt instead.
                            let _ = cx.send_notification(CancelNotification::new(session.clone()));
                        }
                        true
                    }
                },
            };
            if !alive {
                break;
            }
        }
    }

    /// `TurnStarted` and the user item, then the prompt, whose response
    /// comes back through `stops`.
    async fn start(
        &mut self,
        cx: &ConnectionTo<Agent>,
        session: &AcpId,
        text: String,
        stops: &mpsc::UnboundedSender<Stopped>,
    ) -> bool {
        let turn = TurnId::new();
        self.fold.start_turn(turn);
        self.seq += 1;
        self.busy = true;
        let item = ItemId::new();
        let kind = ItemKind::UserMessage {
            text: text.clone(),
            attachments: Vec::new(),
        };
        let started = vec![
            Event::TurnStarted {
                turn,
                seq: self.seq,
                job: Job::Main,
                tier: Tier::Code,
                model: self.model.clone(),
            },
            Event::ItemStarted { item, kind },
            Event::ItemDone { item },
        ];
        // Sent here, not in the task: a cancel after it must reach the wire
        // after it, or the agent would cancel nothing and then run it.
        let request = PromptRequest::new(session.clone(), vec![ContentBlock::from(text)]);
        let sent = cx.send_request(request);
        let stops = stops.clone();
        tokio::spawn(async move {
            let result = sent.block_task().await;
            let _ = stops.send(result.map_err(|e| e.to_string()));
        });
        self.emit(started).await
    }

    /// The prompt came back: the updates still queued, then its stop
    /// reason, or the failure with what the agent last wrote to stderr.
    async fn stopped(
        &mut self,
        updates: &mut mpsc::UnboundedReceiver<SessionUpdate>,
        stopped: Stopped,
    ) -> bool {
        let mut out = Vec::new();
        while let Ok(update) = updates.try_recv() {
            out.extend(self.fold.update(update, self.now()));
        }
        self.busy = false;
        match stopped {
            Ok(response) => out.extend(self.fold.stop(response.stop_reason, self.now())),
            Err(e) => {
                let tail = self.tail().await;
                let message = if tail.is_empty() {
                    e
                } else {
                    format!("{e}: {tail}")
                };
                out.extend(self.fold.failed(&message, self.now()));
            }
        }
        self.emit(out).await
    }

    async fn tail(&mut self) -> String {
        let Some(tail) = self.tail.take() else {
            return String::new();
        };
        match tokio::time::timeout(TAIL_WAIT, tail).await {
            Ok(Ok(text)) => text,
            _ => String::new(),
        }
    }

    /// False once the surface stopped listening.
    async fn emit(&self, events: Vec<Event>) -> bool {
        for event in events {
            if self.events.send(event).await.is_err() {
                return false;
            }
        }
        true
    }

    fn now(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}
