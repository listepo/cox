//! Hook call sites (plan.md T7.4/§1.8): the one place the core asks the
//! runner and applies D14 — a hook that fails to run is a warning, never a
//! stopped turn. The runner itself is a `Hook` trait object the surface
//! installs; the core never spawns a process.

use std::time::Duration;

use cox_protocol::types::{Event, HookEvent, HookOutcome, Level};
use serde_json::{Value, json};

use crate::session::Session;

/// Runs `event` with the common payload fields plus `extra` merged in.
pub(crate) async fn fire(session: &Session, event: HookEvent, extra: Value) -> HookOutcome {
    let config = &session.config.hooks;
    let Some(runner) = session.hook().filter(|_| config.enabled) else {
        return HookOutcome::Continue;
    };
    let mut payload = json!({
        "session_id": session.id.to_string(),
        "cwd": session.cwd,
        "hook_event_name": event.name(),
    });
    if let (Some(into), Some(from)) = (payload.as_object_mut(), extra.as_object()) {
        into.extend(from.clone());
    }
    let timeout = Duration::from_secs(u64::from(config.timeout_s));
    match runner.run(event, payload, timeout).await {
        HookOutcome::Failed { error } if config.fail_open => {
            let _ = session
                .emit(Event::Notice {
                    level: Level::Warn,
                    text: format!("hook {} skipped: {error}", event.name()),
                })
                .await;
            HookOutcome::Continue
        }
        HookOutcome::Failed { error } => HookOutcome::Block { reason: error },
        outcome => outcome,
    }
}
