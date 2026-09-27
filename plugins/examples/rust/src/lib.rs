//! `cox-plugin-example`: the reference plugin PL§13 asks every guest
//! language to implement, written against `cox-plugin-sdk`. It counts turns
//! through a `turn_started` subscription, counts failed tool calls in a
//! `PostToolUse`/`PostToolUseFailure` hook, shows both in a status segment
//! and clears them with `/example:reset`. Both counters persist in the
//! plugin's kv store, mirrored in memory for `cox_render`, which may not
//! touch kv.
//!
//! The decisions are pure functions below so `just plugin-test` checks them
//! on the host target; the exports, which need extism's memory and the
//! `cox:host/v1` imports, build only for `wasm32` (as in `plugins/jev`).
//! `crates/cox-plugin-fixtures` builds this crate into the package the
//! host's end-to-end test installs and `just bench` times.

use cox_plugin_sdk::ui::Span;
use cox_plugin_sdk::{CommandDecl, InitOut, Slot, StyleToken, Value, Widget};

/// kv key of the turn counter.
pub const TURNS: &str = "turns";
/// kv key of the failed-tool-call counter.
pub const FAILURES: &str = "failures";

/// What `cox_init` declares: the status segment, `/example:reset` and the
/// one event kind it wants.
pub fn declarations() -> InitOut {
    InitOut {
        commands: vec![CommandDecl {
            name: "reset".into(),
            description: "Reset the turn and failure counters".into(),
        }],
        status: vec![Slot::StatusRight],
        subscribe: vec!["turn_started".into()],
        ..InitOut::default()
    }
}

/// Turns started in one event batch. The host sends only subscribed kinds,
/// but a newer host may widen a batch, so the tag is checked here too.
pub fn turns_in(events: &[Value]) -> u64 {
    let started = |e: &&Value| e.get("type").and_then(Value::as_str) == Some("turn_started");
    events.iter().filter(started).count() as u64
}

/// Whether a post-tool hook payload reports a failed call.
pub fn is_failure(payload: &Value) -> bool {
    payload
        .pointer("/tool_response/is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// The status segment: dim when nothing failed, a warning colour otherwise.
pub fn status(turns: u64, failures: u64) -> Widget {
    let style = if failures == 0 {
        StyleToken::Dim
    } else {
        StyleToken::Warn
    };
    Widget::Text(vec![vec![Span {
        text: format!("turns {turns} · failed tools {failures}"),
        style,
        ..Span::default()
    }]])
}

#[cfg(target_arch = "wasm32")]
mod guest {
    use core::sync::atomic::{AtomicU64, Ordering};

    use cox_plugin_sdk::{
        CommandIn, CommandOut, Effects, EventBatch, HookCall, InitIn, InitOut, NoticeLevel,
        PluginNotice, RenderIn, SdkError, SdkResult, Value, Widget, kv_delete, kv_get, kv_put,
        notify,
    };

    use super::{FAILURES, TURNS};

    // `cox_render` may not touch the kv store (PL§4: it only reads, under a
    // strict time cap), so the one instance keeps the counters in memory and
    // writes each change through to kv, which carries them to the next
    // session.
    static TURNS_NOW: AtomicU64 = AtomicU64::new(0);
    static FAILURES_NOW: AtomicU64 = AtomicU64::new(0);

    fn load(key: &str, into: &AtomicU64) -> SdkResult<()> {
        let n = kv_get(key)?.and_then(|v| v.as_u64()).unwrap_or(0);
        into.store(n, Ordering::Relaxed);
        Ok(())
    }

    fn add(key: &str, counter: &AtomicU64, by: u64) -> SdkResult<u64> {
        let n = counter.load(Ordering::Relaxed) + by;
        kv_put(key, &Value::from(n))?;
        counter.store(n, Ordering::Relaxed);
        Ok(n)
    }

    fn init(_: InitIn) -> Result<InitOut, SdkError> {
        load(TURNS, &TURNS_NOW)?;
        load(FAILURES, &FAILURES_NOW)?;
        Ok(super::declarations())
    }

    fn on_event(batch: EventBatch) -> Result<Effects, SdkError> {
        let started = super::turns_in(&batch.events);
        if started > 0 {
            add(TURNS, &TURNS_NOW, started)?;
        }
        Ok(Effects {
            redraw: started > 0,
            ..Effects::default()
        })
    }

    fn hook(call: HookCall) -> Result<Value, SdkError> {
        if super::is_failure(&call.payload) {
            let n = add(FAILURES, &FAILURES_NOW, 1)?;
            // A refusal (the notice queue is full) only loses the notice;
            // the count is already stored, so the hook still succeeds.
            let _ = notify(NoticeLevel::Info, &format!("failed tool calls: {n}"));
        }
        // Informational: a post-tool hook never changes the call.
        Ok(serde_json::json!({ "type": "continue" }))
    }

    fn command(c: CommandIn) -> Result<CommandOut, SdkError> {
        if c.name != "reset" {
            return Ok(CommandOut::Nothing);
        }
        kv_delete(TURNS)?;
        kv_delete(FAILURES)?;
        TURNS_NOW.store(0, Ordering::Relaxed);
        FAILURES_NOW.store(0, Ordering::Relaxed);
        Ok(CommandOut::Notice(PluginNotice {
            level: NoticeLevel::Info,
            text: "counters reset".into(),
        }))
    }

    fn render(_: RenderIn) -> Result<Widget, SdkError> {
        Ok(super::status(
            TURNS_NOW.load(Ordering::Relaxed),
            FAILURES_NOW.load(Ordering::Relaxed),
        ))
    }

    cox_plugin_sdk::register!(
        init => init,
        on_event => on_event,
        hook => hook,
        command => command,
        render => render,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn declares_status_reset_and_turn_started() {
        let out = declarations();
        assert_eq!(out.status, [Slot::StatusRight]);
        assert_eq!(out.commands[0].name, "reset");
        assert_eq!(out.subscribe, ["turn_started"]);
    }

    #[test]
    fn counts_only_turn_started_events() {
        let events = [
            json!({ "type": "turn_started", "seq": 1 }),
            json!({ "type": "notice", "text": "x" }),
            json!({ "type": "turn_started", "seq": 2 }),
        ];
        assert_eq!(turns_in(&events), 2);
    }

    #[test]
    fn only_an_error_response_is_a_failure() {
        let failed = json!({ "tool_response": { "text": "no such file", "is_error": true } });
        let ok = json!({ "tool_response": { "text": "x", "is_error": false } });
        assert!(is_failure(&failed));
        assert!(!is_failure(&ok));
        assert!(!is_failure(&json!({})));
    }

    #[test]
    fn status_warns_once_a_tool_failed() {
        let Widget::Text(lines) = status(3, 1) else {
            panic!("a text widget");
        };
        assert_eq!(lines[0][0].text, "turns 3 · failed tools 1");
        assert_eq!(lines[0][0].style, StyleToken::Warn);
        let Widget::Text(calm) = status(3, 0) else {
            panic!("a text widget");
        };
        assert_eq!(calm[0][0].style, StyleToken::Dim);
    }
}
