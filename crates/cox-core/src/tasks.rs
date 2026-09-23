//! Background tasks (T9.2, T27.1): `agent` with `background: true` runs its
//! child session concurrently, and any running `bash` or `agent` call can be
//! detached into a task — at once for `bash(background: true)`, mid-run on
//! `Submission::Background` (`Ctrl+B`). The contract, in both directions:
//! the model sees a short pointer line in history and the user sees a
//! bounded notice — the full output lives only in the archive (or the
//! child's rollout) and is never smuggled into context silently.
//!
//! Cancellation is turn-scoped: a background task clones the spawning
//! turn's token, so `Interrupt` stops it only while that turn is current;
//! a later turn does not cancel work it did not start.

use cox_protocol::ArchivePut;
use cox_protocol::errors::{CoreError, ToolError};
use cox_protocol::ids::{ArchiveId, CallId, ItemId, TaskId, TurnId};
use cox_protocol::traits::ToolCx;
use cox_protocol::types::{Content, Event, Level, Message, Role, Tier, ToolOutput, ToolResult};
use serde_json::Value;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::checkpoint::{self, Pending};
use crate::session::Session;

/// What a task runs. A subagent reports its own `TaskCreated`/
/// `TaskCompleted` pair (`subagent.rs`); a detached shell call gets its pair
/// here, with an exit code and the archive row of its output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    /// A subagent child session.
    Agent,
    /// A detached `bash` call.
    Shell,
}

impl TaskKind {
    /// `bash` is the one shell tool; everything else detachable is an agent.
    pub fn of(tool: &str) -> Self {
        if tool == "bash" {
            Self::Shell
        } else {
            Self::Agent
        }
    }
}

/// A spawned call: its output plus the context back, so a sandbox-denied
/// retry can run again with the same context.
pub(crate) type Running = JoinHandle<(ToolOutput, ToolCx)>;

/// The call a detached task stands for.
pub(crate) struct Detachable {
    pub turn: TurnId,
    pub call: CallId,
    pub tool: String,
    pub subject: String,
}

/// History pointer on completion: label, id, detail (cost, or exit code and
/// archive id) and the output's first line, capped so a multi-kilobyte
/// answer cannot leak in through it.
pub fn pointer_line(label: &str, task: TaskId, detail: &str, answer: &str) -> String {
    let first: String = answer
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(120)
        .collect();
    format!("background `{label}` finished (task {task}, {detail}): {first}")
}

/// How much of the answer the completion notice shows the user.
pub const NOTICE_CAP: usize = 2000;

/// Notice text on completion: label, detail and the answer, truncated with
/// a marker rather than cut silently.
pub fn notice_text(label: &str, task: TaskId, answer: &str, detail: &str) -> String {
    let mut short: String = answer.chars().take(NOTICE_CAP).collect();
    if answer.chars().count() > NOTICE_CAP {
        short.push_str("\n[truncated]");
    }
    format!("background task finished: {label} (task {task}, {detail})\n{short}")
}

/// A subagent's detail: what it cost.
pub fn cost_detail(cost_usd: f64) -> String {
    format!("${cost_usd:.4}")
}

/// A detached call's detail: its exit code, if it has one, and how to get
/// the whole output back.
pub fn detached_detail(exit_code: Option<i32>, archive: Option<ArchiveId>) -> String {
    let exit = exit_code.map(|c| format!("exit {c}, ")).unwrap_or_default();
    match archive {
        Some(id) => format!("{exit}full output: expand {id}"),
        None => format!("{exit}output could not be archived"),
    }
}

impl Session {
    /// Registers a running task; `/tasks` and the status count read the
    /// `TaskCreated`/`TaskCompleted` events, this is the core's own view.
    pub(crate) async fn register_task(
        &self,
        task: TaskId,
        label: String,
        tier: Tier,
        kind: TaskKind,
    ) {
        self.inner
            .lock()
            .await
            .tasks
            .insert(task, (label, tier, kind));
    }

    /// Forgets a finished task.
    pub(crate) async fn complete_task(&self, task: TaskId) {
        self.inner.lock().await.tasks.remove(&task);
    }

    /// Completion report: the pointer line enters history for the model,
    /// the notice goes to the user. The full output stays in the archive or
    /// the child's rollout file.
    pub(crate) async fn publish_task_result(
        &self,
        task: TaskId,
        label: &str,
        answer: &str,
        detail: &str,
    ) -> Result<(), CoreError> {
        {
            let mut inner = self.inner.lock().await;
            inner.history.push(Message {
                role: Role::User,
                content: vec![Content::Text {
                    text: pointer_line(label, task, detail, answer),
                }],
            });
        }
        self.emit(Event::Notice {
            level: Level::Info,
            text: notice_text(label, task, answer, detail),
        })
        .await
    }

    /// Arms `Submission::Background` for a call about to run. `bash` with
    /// `background: true` is the same detach pulled at once, so the flag
    /// leaves the input the tool sees: the core owns the task, not the tool.
    pub(crate) async fn arm_detach(
        &self,
        call: CallId,
        tool: &str,
        mut input: Value,
    ) -> (Value, CancellationToken) {
        let token = CancellationToken::new();
        if TaskKind::of(tool) == TaskKind::Shell
            && let Some(flag) = input.as_object_mut().and_then(|o| o.remove("background"))
            && flag.as_bool() == Some(true)
        {
            token.cancel();
        }
        self.inner.lock().await.detach.insert(call, token.clone());
        (input, token)
    }

    /// `Submission::Background`: pulls a running call's detach switch.
    pub(crate) async fn background(&self, call: CallId) -> Result<(), CoreError> {
        let token = self.inner.lock().await.detach.get(&call).cloned();
        match token {
            Some(token) => {
                token.cancel();
                Ok(())
            }
            None => {
                self.emit(Event::Notice {
                    level: Level::Warn,
                    text: format!("no running call {call} to move to the background"),
                })
                .await
            }
        }
    }

    /// Waits for `running` unless it is detached first. `Ok` is the call's
    /// own output (with its context unless the call panicked); `Err` is the
    /// pointer result the model gets for a call that now runs as a task.
    pub(crate) async fn wait_or_detach(
        &self,
        at: Detachable,
        mut running: Running,
        detach: CancellationToken,
        pending: &mut Option<Pending>,
    ) -> Result<(ToolOutput, Option<ToolCx>), ToolResult> {
        let joined = tokio::select! {
            biased;
            joined = &mut running => Some(joined),
            _ = detach.cancelled() => None,
        };
        self.inner.lock().await.detach.remove(&at.call);
        match joined {
            Some(Ok((output, cx))) => Ok((output, Some(cx))),
            Some(Err(_)) => Ok((crate::turn::error_output(ToolError::Io), None)),
            None => Err(self.detach_task(at, running, pending.take()).await),
        }
    }

    /// Turns a running call into a task: registry and `TaskCreated` for a
    /// shell call, the rest of the call on its own tokio task, and the
    /// pointer result back for the turn to continue with.
    async fn detach_task(
        &self,
        at: Detachable,
        running: Running,
        pending: Option<Pending>,
    ) -> ToolResult {
        let task = TaskId::new();
        let kind = TaskKind::of(&at.tool);
        let label = format!("{}: {}", at.tool, crate::subagent::first_line(&at.subject));
        if kind == TaskKind::Shell {
            let _ = self
                .emit(Event::TaskCreated {
                    task,
                    label: label.clone(),
                    tier: self.tier,
                })
                .await;
            self.register_task(task, label.clone(), self.tier, kind)
                .await;
        }
        let session = self.clone();
        let text = format!(
            "background task {task} started: {label}\n\
             its result will arrive as a notice, not in this turn"
        );
        tokio::spawn(async move {
            let output = match running.await {
                Ok((output, _)) => output,
                Err(_) => crate::turn::error_output(ToolError::Io),
            };
            if let Some(pending) = pending {
                checkpoint::after(&session, at.turn, at.call, pending).await;
            }
            session
                .finish_detached(task, kind, &at, &label, output)
                .await;
        });
        ToolResult {
            ok: true,
            bytes: text.len() as u64,
            visible: text,
            archive: None,
            duration_ms: 0,
            diff: None,
        }
    }

    /// The archive row first, then the completion pair and the pointer —
    /// the model never sees a shortened output before it is retrievable.
    async fn finish_detached(
        &self,
        task: TaskId,
        kind: TaskKind,
        at: &Detachable,
        label: &str,
        output: ToolOutput,
    ) {
        let archive = self
            .archive
            .put(ArchivePut {
                session: self.id,
                call: at.call,
                tool: at.tool.clone(),
                subject: Some(at.subject.clone()),
                bytes: output.text.as_bytes().to_vec(),
            })
            .await
            .ok();
        let exit_code = output
            .structured
            .as_ref()
            .and_then(|s| s.get("exit_code"))
            .and_then(Value::as_i64)
            .and_then(|c| i32::try_from(c).ok());
        if kind == TaskKind::Shell {
            self.complete_task(task).await;
            let _ = self
                .emit(Event::TaskCompleted {
                    task,
                    result_item: ItemId::new(),
                    cost_usd: 0.0,
                    exit_code,
                    archive,
                })
                .await;
        }
        let detail = detached_detail(exit_code, archive);
        let _ = self
            .publish_task_result(task, label, &output.text, &detail)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tasks_pointer_line_is_bounded() {
        let long = format!("{}\nsecond line", "x".repeat(5000));
        let line = pointer_line("explore: y", TaskId::new(), &cost_detail(0.002), &long);
        assert!(line.contains("explore: y"), "{line}");
        assert!(line.contains("$0.0020"), "{line}");
        assert!(!line.contains("second line"), "first line only");
        assert!(line.len() < 300, "bounded: {}", line.len());
    }

    #[test]
    fn tasks_notice_truncates_with_a_marker() {
        let long = "y".repeat(NOTICE_CAP + 10);
        let text = notice_text("shell: make", TaskId::new(), &long, "exit 0");
        assert!(text.contains("[truncated]"), "marked, not cut silently");
        let short = notice_text("shell: make", TaskId::new(), "ok", "exit 0");
        assert!(!short.contains("[truncated]"), "{short}");
    }

    #[test]
    fn detached_detail_names_exit_code_and_expand_id() {
        let id = ArchiveId::new();
        assert_eq!(
            detached_detail(Some(2), Some(id)),
            format!("exit 2, full output: expand {id}")
        );
        assert_eq!(detached_detail(None, None), "output could not be archived");
    }

    #[test]
    fn only_bash_is_a_shell_task() {
        assert_eq!(TaskKind::of("bash"), TaskKind::Shell);
        assert_eq!(TaskKind::of("agent"), TaskKind::Agent);
    }
}
