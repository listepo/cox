//! Loop tests: Scripted provider + stub tools, golden Event JSONL (T2.1).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cox_core::{MemoryStore, Session};
use cox_protocol::errors::ToolError;
use cox_protocol::traits::{Tool, ToolCx};
use cox_protocol::types::{
    Concurrency, Content, Event, Risk, StopReason, Submission, ToolOutput, ToolSpec,
};
use cox_provider::scripted::Scripted;
use serde_json::Value;
use tokio::sync::mpsc;

struct Echo;
struct Touch;
struct Slow;

#[async_trait]
impl Tool for Echo {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "echo".into(),
            description: "echo input text".into(),
            input_schema: serde_json::json!({"type": "object"}),
            deferred: false,
            risk: Risk::ReadOnly,
            concurrency: Concurrency::Parallel,
        }
    }
    fn subject(&self, input: &Value) -> String {
        input
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .into()
    }
    async fn call(&self, input: Value, _cx: &ToolCx) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput {
            text: self.subject(&input),
            is_error: false,
            diff: None,
            structured: None,
        })
    }
}

#[async_trait]
impl Tool for Touch {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "touch".into(),
            description: "exclusive write stub".into(),
            input_schema: serde_json::json!({"type": "object"}),
            deferred: false,
            risk: Risk::Write,
            concurrency: Concurrency::Exclusive,
        }
    }
    fn subject(&self, _input: &Value) -> String {
        "touch".into()
    }
    async fn call(&self, _input: Value, _cx: &ToolCx) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput {
            text: "touched".into(),
            is_error: false,
            diff: None,
            structured: None,
        })
    }
}

#[async_trait]
impl Tool for Slow {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "slow".into(),
            description: "polls cancel".into(),
            input_schema: serde_json::json!({"type": "object"}),
            deferred: false,
            risk: Risk::ReadOnly,
            concurrency: Concurrency::Parallel,
        }
    }
    fn subject(&self, _input: &Value) -> String {
        "slow".into()
    }
    async fn call(&self, _input: Value, cx: &ToolCx) -> Result<ToolOutput, ToolError> {
        loop {
            if cx.cancel.is_cancelled() {
                return Ok(ToolOutput {
                    text: "cancelled".into(),
                    is_error: true,
                    diff: None,
                    structured: None,
                });
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

fn scenario(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/scenarios")
        .join(format!("{name}.toml"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn tools() -> Vec<Arc<dyn Tool>> {
    vec![Arc::new(Echo), Arc::new(Touch), Arc::new(Slow)]
}

fn open(
    toml: &str,
    mut config: cox_protocol::Config,
) -> (Session, Arc<MemoryStore>, mpsc::Receiver<Event>) {
    config.core.workspace_roots = vec![PathBuf::from("/tmp/cox-turn")];
    let provider = Arc::new(Scripted::from_toml(toml, "").expect("scenario"));
    let store = Arc::new(MemoryStore::new());
    let session = Session::new(
        config,
        provider,
        tools(),
        store.clone(),
        store.clone(),
        PathBuf::from("/tmp/cox-turn"),
    )
    .expect("session");
    let rx = session.events().expect("events once");
    (session, store, rx)
}

async fn drain(rx: &mut mpsc::Receiver<Event>) -> Vec<Event> {
    let mut out = Vec::new();
    loop {
        let ev = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("event timeout")
            .expect("event stream closed");
        let done = matches!(ev, Event::TurnDone { .. });
        out.push(ev);
        if done {
            break;
        }
    }
    out
}

async fn run(name: &str) -> (Vec<Event>, Arc<MemoryStore>, Session) {
    let (session, store, mut rx) = open(&scenario(name), cox_protocol::Config::default());
    session
        .submit(Submission::UserTurn {
            text: name.into(),
            attachments: vec![],
            confirm_think: false,
        })
        .await
        .expect("submit");
    let events = drain(&mut rx).await;
    (events, store, session)
}

fn is_ulid(s: &str) -> bool {
    s.len() == 26
        && s.bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'A'..=b'H' | b'J'..=b'K' | b'M'..=b'N' | b'P'..=b'T' | b'V'..=b'Z'))
}

fn redact(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) if is_ulid(s) => *s = "<id>".into(),
        // The truncation trailer embeds an archive id mid-string.
        serde_json::Value::String(s) if s.contains("expand #") => {
            let start = s.find("expand #").unwrap_or(0) + "expand #".len();
            if s.len() >= start + 26 && is_ulid(&s[start..start + 26]) {
                s.replace_range(start..start + 26, "<id>");
            }
        }
        serde_json::Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if matches!(
                    k.as_str(),
                    "duration_ms"
                        | "input_tokens"
                        | "output_tokens"
                        | "cache_read_tokens"
                        | "cache_write_tokens"
                        | "latency_ms"
                ) {
                    *v = serde_json::json!(0);
                } else if k == "cost_usd" {
                    *v = serde_json::json!(0.0);
                } else {
                    redact(v);
                }
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(redact),
        _ => {}
    }
}

fn snapshot(name: &str, events: &[Event]) {
    let mut value = serde_json::to_value(events).expect("events json");
    redact(&mut value);
    insta::with_settings!({
        snapshot_path => "scenarios",
        prepend_module_to_snapshot => false,
    }, {
        insta::assert_json_snapshot!(format!("{name}.events"), value);
    });
}

#[tokio::test]
async fn turn_text_only_snapshot() {
    let (events, store, _) = run("text_only").await;
    snapshot("text_only", &events);
    assert!(matches!(
        events.last(),
        Some(Event::TurnDone {
            stop: StopReason::EndTurn,
            ..
        })
    ));
    assert_eq!(store.usage_rows().len(), 1);
}

#[tokio::test]
async fn turn_one_tool_snapshot() {
    let (events, store, _) = run("one_tool").await;
    snapshot("one_tool", &events);
    assert_eq!(store.usage_rows().len(), 2);
}

#[tokio::test]
async fn turn_three_parallel_snapshot() {
    let (events, _, session) = run("three_parallel").await;
    snapshot("three_parallel", &events);
    let done: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::ToolCallDone { result, .. } => Some(result.visible.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(done, ["a", "b", "c"]);
    let history = session.history().await;
    let results = history
        .iter()
        .find(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, Content::ToolResult { .. }))
        })
        .expect("one user message of tool results");
    let n = results
        .content
        .iter()
        .filter(|c| matches!(c, Content::ToolResult { .. }))
        .count();
    assert_eq!(n, 3);
}

#[tokio::test]
async fn turn_big_tool_output_is_truncated_then_expandable() {
    let mut config = cox_protocol::Config::default();
    config.context.tool_output_visible_bytes = 120;
    config.context.tool_output_head_lines = 2;
    config.context.tool_output_tail_lines = 2;
    let (session, store, mut rx) = open(&scenario("big_tool_output"), config);
    session
        .submit(Submission::UserTurn {
            text: "big".into(),
            attachments: vec![],
            confirm_think: false,
        })
        .await
        .expect("submit");
    let events = drain(&mut rx).await;
    snapshot("big_tool_output", &events);
    let result = events
        .iter()
        .find_map(|e| match e {
            Event::ToolCallDone { result, .. } => Some(result.clone()),
            _ => None,
        })
        .expect("tool result");
    let archive = result.archive.expect("archived before truncation");
    assert!(result.visible.contains(&format!("expand #{}", archive.id)));
    assert!(result.visible.len() <= 120);
    // The follow-up `expand` call reads the whole output back from the archive.
    let (tx, _rx) = mpsc::channel(1);
    let cx = cox_tools::tool_cx(
        vec![PathBuf::from("/tmp/cox-turn")],
        PathBuf::from("/tmp/cox-turn"),
        cox_protocol::types::SandboxPolicy {
            mode: cox_protocol::types::SandboxMode::ReadOnly,
            network: false,
            writable: vec![],
            readonly_in_workspace: vec![],
        },
        store,
        tokio_util::sync::CancellationToken::new(),
        tx,
        cox_protocol::ids::SessionId::new(),
        cox_protocol::ids::CallId::new(),
    );
    let expanded = cox_tools::expand::ExpandTool
        .call(serde_json::json!({"id": archive.id.to_string()}), &cx)
        .await
        .expect("expand");
    assert_eq!(expanded.text.len() as u64, archive.bytes);
    assert!(expanded.text.starts_with("line 01\n") && expanded.text.ends_with("line 20"));
}

#[tokio::test]
async fn turn_provider_error_snapshot() {
    let (events, _, _) = run("provider_error").await;
    snapshot("provider_error", &events);
    assert!(events.iter().any(|e| matches!(e, Event::Error { .. })));
    assert!(matches!(
        events.last(),
        Some(Event::TurnDone {
            stop: StopReason::Error,
            ..
        })
    ));
}

#[tokio::test]
async fn turn_max_turns_snapshot() {
    let mut config = cox_protocol::Config::default();
    config.core.max_turns = 1;
    let (session, _, mut rx) = open(&scenario("max_turns"), config);
    session
        .submit(Submission::UserTurn {
            text: "max".into(),
            attachments: vec![],
            confirm_think: false,
        })
        .await
        .expect("submit");
    let events = drain(&mut rx).await;
    snapshot("max_turns", &events);
    assert!(matches!(
        events.last(),
        Some(Event::TurnDone {
            stop: StopReason::MaxTurns,
            ..
        })
    ));
}

#[tokio::test]
async fn turn_interrupt_mid_tool_snapshot() {
    let (session, _, mut rx) = open(&scenario("interrupt"), cox_protocol::Config::default());
    let running = {
        let session = session.clone();
        tokio::spawn(async move {
            session
                .submit(Submission::UserTurn {
                    text: "interrupt".into(),
                    attachments: vec![],
                    confirm_think: false,
                })
                .await
        })
    };
    let mut events = Vec::new();
    loop {
        let ev = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("wait tool")
            .expect("closed");
        let requested = matches!(ev, Event::ToolCallRequested { .. });
        events.push(ev);
        if requested {
            break;
        }
    }
    session
        .submit(Submission::Interrupt)
        .await
        .expect("interrupt");
    running.await.expect("join").expect("turn");
    events.extend(drain(&mut rx).await);
    snapshot("interrupt", &events);
    assert!(matches!(
        events.last(),
        Some(Event::TurnDone {
            stop: StopReason::Interrupted,
            ..
        })
    ));
}

#[tokio::test]
async fn turn_all_tool_results_return_in_one_message() {
    let (_, _, session) = run("three_parallel").await;
    let with_results: Vec<_> = session
        .history()
        .await
        .into_iter()
        .filter(|m| {
            m.content
                .iter()
                .any(|c| matches!(c, Content::ToolResult { .. }))
        })
        .collect();
    assert_eq!(with_results.len(), 1);
    assert_eq!(
        with_results[0]
            .content
            .iter()
            .filter(|c| matches!(c, Content::ToolResult { .. }))
            .count(),
        3
    );
}

#[tokio::test]
async fn turn_no_event_after_turn_done() {
    let (session, _, mut rx) = open(&scenario("text_only"), cox_protocol::Config::default());
    session
        .submit(Submission::UserTurn {
            text: "hi".into(),
            attachments: vec![],
            confirm_think: false,
        })
        .await
        .expect("submit");
    let events = drain(&mut rx).await;
    assert!(matches!(events.last(), Some(Event::TurnDone { .. })));
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn turn_every_request_has_a_usage_row() {
    let (events, store, _) = run("one_tool").await;
    let usage_events = events
        .iter()
        .filter(|e| matches!(e, Event::Usage { .. }))
        .count();
    assert_eq!(usage_events, store.usage_rows().len());
    assert_eq!(usage_events, 2);
}
