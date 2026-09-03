//! Background tasks (T9.2): `agent` with `background: true` runs its child
//! session concurrently and reports back visibly. The contract, in both
//! directions: the model sees a short pointer line in history and the user
//! sees a bounded notice — the full answer lives only in the child's
//! rollout and is never smuggled into context silently.
//!
//! Cancellation is turn-scoped: a background task clones the spawning
//! turn's token, so `Interrupt` stops it only while that turn is current;
//! a later turn does not cancel work it did not start.

use cox_protocol::errors::CoreError;
use cox_protocol::ids::TaskId;
use cox_protocol::types::{Content, Event, Level, Message, Role, Tier};

use crate::session::Session;

/// History pointer on completion: label, id, cost and the answer's first
/// line, capped so a multi-kilobyte answer cannot leak in through it.
pub fn pointer_line(label: &str, task: TaskId, cost_usd: f64, answer: &str) -> String {
    let first: String = answer
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(120)
        .collect();
    format!("background `{label}` finished (task {task}, ${cost_usd:.4}): {first}")
}

/// How much of the answer the completion notice shows the user.
pub const NOTICE_CAP: usize = 2000;

/// Notice text on completion: label, cost and the answer, truncated with a
/// marker rather than cut silently.
pub fn notice_text(label: &str, task: TaskId, answer: &str, cost_usd: f64) -> String {
    let mut short: String = answer.chars().take(NOTICE_CAP).collect();
    if answer.chars().count() > NOTICE_CAP {
        short.push_str("\n[truncated]");
    }
    format!("background task finished: {label} (task {task}, ${cost_usd:.4})\n{short}")
}

impl Session {
    /// Registers a running task; `/tasks` and the status count read the
    /// `TaskCreated`/`TaskCompleted` events, this is the core's own view.
    pub(crate) async fn register_task(&self, task: TaskId, label: String, tier: Tier) {
        self.inner.lock().await.tasks.insert(task, (label, tier));
    }

    /// Forgets a finished task.
    pub(crate) async fn complete_task(&self, task: TaskId) {
        self.inner.lock().await.tasks.remove(&task);
    }

    /// Completion report: the pointer line enters history for the model,
    /// the notice goes to the user. The full answer stays in the child's
    /// rollout file.
    pub(crate) async fn publish_task_result(
        &self,
        task: TaskId,
        label: &str,
        answer: &str,
        cost_usd: f64,
    ) -> Result<(), CoreError> {
        {
            let mut inner = self.inner.lock().await;
            inner.history.push(Message {
                role: Role::User,
                content: vec![Content::Text {
                    text: pointer_line(label, task, cost_usd, answer),
                }],
            });
        }
        self.emit(Event::Notice {
            level: Level::Info,
            text: notice_text(label, task, answer, cost_usd),
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tasks_pointer_line_is_bounded() {
        let long = format!("{}\nsecond line", "x".repeat(5000));
        let line = pointer_line("explore: y", TaskId::new(), 0.002, &long);
        assert!(line.contains("explore: y"), "{line}");
        assert!(line.contains("$0.0020"), "{line}");
        assert!(!line.contains("second line"), "first line only");
        assert!(line.len() < 300, "bounded: {}", line.len());
    }

    #[test]
    fn tasks_notice_truncates_with_a_marker() {
        let long = "y".repeat(NOTICE_CAP + 10);
        let text = notice_text("shell: make", TaskId::new(), &long, 0.0);
        assert!(text.contains("[truncated]"), "marked, not cut silently");
        let short = notice_text("shell: make", TaskId::new(), "ok", 0.0);
        assert!(!short.contains("[truncated]"), "{short}");
    }
}
