//! Resume rebuilds the same `Request` a live session would assemble (T2.4).

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use cox_core::{History, MemoryStore, Session, assemble};
use cox_protocol::errors::ToolError;
use cox_protocol::ids::SessionId;
use cox_protocol::traits::{Store, Tool, ToolCx};
use cox_protocol::types::{Concurrency, Event, Risk, Submission, ToolOutput, ToolSpec};
use cox_provider::scripted::Scripted;
use serde_json::Value;

struct Echo;

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

fn scenario() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/scenarios/one_tool.toml");
    std::fs::read_to_string(&path).expect("one_tool.toml")
}

#[tokio::test]
async fn resume_builds_identical_request() {
    let toml = scenario();
    let mut config = cox_protocol::Config::default();
    config.core.workspace_roots = vec![PathBuf::from("/tmp/cox-turn")];
    let provider = Arc::new(Scripted::from_toml(&toml, "").expect("scenario"));
    let store = Arc::new(MemoryStore::new());
    let tools: Vec<Arc<dyn Tool>> = vec![Arc::new(Echo)];
    let cwd = PathBuf::from("/tmp/cox-turn");
    let session = Session::new(
        config.clone(),
        provider,
        tools.clone(),
        store.clone(),
        store.clone(),
        cwd.clone(),
    )
    .expect("session");
    session
        .submit(Submission::UserTurn {
            text: "one_tool".into(),
            attachments: vec![],
            confirm_think: false,
        })
        .await
        .expect("submit");
    let live = session.history().await;
    let events = store.rollout_read(&SessionId::new()).expect("rollout");
    assert!(
        events.len() >= 15,
        "expected a full tool turn, got {}",
        events.len()
    );
    assert!(matches!(events.last(), Some(Event::TurnDone { .. })));

    let rebuilt = History::from_events(&events);
    assert_eq!(rebuilt.messages, live);

    let live_req = assemble(&live, &config, &tools, &cwd, "");
    let resume_req = assemble(&rebuilt.messages, &config, &tools, &cwd, "");
    assert_eq!(live_req, resume_req);
}
