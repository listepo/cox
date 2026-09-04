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
    // `--no-hooks` decides what the surface installs (T16.2: presence stays
    // on, shell hooks come off); an installed hook always runs.
    let Some(runner) = session.hook() else {
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

/// What a `UserPromptSubmit` `Modify` means (T16.2): a string replaces the
/// prompt; an object may carry `prompt` and/or `additional_context`. The
/// context rides to the model as a second text block after the last cache
/// breakpoint and is never shown as the user's words.
pub(crate) fn prompt_rewrite(text: String, input: Value) -> (String, Option<String>) {
    match input {
        Value::String(s) => (s, None),
        Value::Object(o) => {
            let prompt = o
                .get("prompt")
                .and_then(Value::as_str)
                .map_or(text, str::to_owned);
            let context = o
                .get("additional_context")
                .and_then(Value::as_str)
                .filter(|c| !c.is_empty())
                .map(str::to_owned);
            (prompt, context)
        }
        _ => (text, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_rewrite_accepts_a_string_or_a_prompt_and_context_object() {
        let t = || "hi".to_owned();
        assert_eq!(prompt_rewrite(t(), json!("new")), ("new".into(), None));
        assert_eq!(
            prompt_rewrite(t(), json!({ "additional_context": "peer" })),
            ("hi".into(), Some("peer".into()))
        );
        assert_eq!(
            prompt_rewrite(t(), json!({ "prompt": "new", "additional_context": "" })),
            ("new".into(), None)
        );
        assert_eq!(prompt_rewrite(t(), json!(7)), ("hi".into(), None));
    }
}
