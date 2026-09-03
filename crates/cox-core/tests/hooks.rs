//! T7.4 in the loop: hook verdicts reach the turn through the core's call
//! sites, and a broken hook is a warning, never a stopped turn
//! (`broken_hook_is_skipped_not_fatal`, plan.md §1.10 #10).

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use cox_protocol::traits::Hook;
use cox_protocol::types::{Event, HookEvent, HookOutcome, Level, StopReason};
use serde_json::{Value, json};

/// Answers `PreToolUse` with `verdict`, everything else with `Continue`,
/// and records every event it saw.
struct Stub {
    verdict: HookOutcome,
    seen: Mutex<Vec<(HookEvent, Value)>>,
}

impl Stub {
    fn new(verdict: HookOutcome) -> Arc<Self> {
        Arc::new(Self {
            verdict,
            seen: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl Hook for Stub {
    async fn run(&self, event: HookEvent, payload: Value, _timeout: Duration) -> HookOutcome {
        self.seen.lock().unwrap().push((event, payload));
        if event == HookEvent::PreToolUse {
            self.verdict.clone()
        } else {
            HookOutcome::Continue
        }
    }
}

async fn run_one_tool(stub: Arc<Stub>) -> Vec<Event> {
    let (session, _store, mut rx) = common::open(
        &common::scenario("one_tool"),
        cox_protocol::Config::default(),
    );
    session.set_hook(stub);
    let running = common::spawn_turn(&session, "one_tool");
    let events = common::drain(&mut rx).await;
    running.await.expect("join").expect("turn");
    events
}

#[tokio::test]
async fn broken_hook_is_skipped_not_fatal() {
    let stub = Stub::new(HookOutcome::Failed {
        error: "exit 7".into(),
    });
    let events = run_one_tool(stub.clone()).await;
    assert!(common::tool_results(&events)[0].0);
    assert!(events.iter().any(|e| matches!(e, Event::Notice { level: Level::Warn, text } if text == "hook PreToolUse skipped: exit 7")));
    assert!(matches!(
        events.last(),
        Some(Event::TurnDone {
            stop: StopReason::EndTurn,
            ..
        })
    ));
    let seen: Vec<&str> = stub
        .seen
        .lock()
        .unwrap()
        .iter()
        .map(|(e, _)| e.name())
        .collect();
    assert_eq!(
        seen,
        ["UserPromptSubmit", "PreToolUse", "PostToolUse", "Stop"]
    );
}

#[tokio::test]
async fn hooks_pre_tool_use_block_fails_the_call_with_the_reason() {
    let stub = Stub::new(HookOutcome::Block {
        reason: "no echoing".into(),
    });
    let events = run_one_tool(stub.clone()).await;
    let (ok, visible) = &common::tool_results(&events)[0];
    assert!(!ok);
    assert!(visible.contains("blocked by hook: no echoing"), "{visible}");
    let seen = stub.seen.lock().unwrap();
    let (_, payload) = seen
        .iter()
        .find(|(e, _)| *e == HookEvent::PreToolUse)
        .unwrap();
    assert_eq!(payload["tool_name"], "echo");
    assert_eq!(payload["tool_input"], json!({ "text": "hi" }));
    assert_eq!(payload["hook_event_name"], "PreToolUse");
    assert!(
        payload["session_id"]
            .as_str()
            .is_some_and(|s| s.len() == 26)
    );
}

#[tokio::test]
async fn hooks_pre_tool_use_modify_rewrites_the_input() {
    let stub = Stub::new(HookOutcome::Modify {
        input: json!({ "text": "rewritten" }),
    });
    let events = run_one_tool(stub.clone()).await;
    let (ok, visible) = &common::tool_results(&events)[0];
    assert!(ok);
    assert!(visible.contains("rewritten"), "{visible}");
    let seen = stub.seen.lock().unwrap();
    let (_, post) = seen
        .iter()
        .find(|(e, _)| *e == HookEvent::PostToolUse)
        .unwrap();
    assert_eq!(post["tool_input"], json!({ "text": "rewritten" }));
    assert_eq!(post["tool_response"]["is_error"], false);
}
