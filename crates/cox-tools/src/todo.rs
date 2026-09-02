//! `todo`: a structured task list the model reports progress through
//! (plan.md §1.11, T3.6). No filesystem or network access — `ReadOnly` per
//! the tool catalogue — so it does nothing `confine` needs to guard; its
//! only job is validating the list shape and handing the TUI a
//! machine-readable panel via `ToolOutput.structured` (T5.5).

use async_trait::async_trait;
use cox_protocol::{Concurrency, Risk, Tool, ToolCx, ToolError, ToolOutput, ToolSpec};
use serde_json::{Value, json};

use crate::write::str_field;

/// The only states a todo item may be in (plan.md §1.11: "state drives the
/// TUI todo panel").
const VALID_STATES: [&str; 3] = ["pending", "in_progress", "done"];

struct TodoItem {
    id: String,
    text: String,
    state: String,
}

pub struct TodoTool;

#[async_trait]
impl Tool for TodoTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "todo".to_string(),
            description: "Report the current task list so progress is visible in the TUI. \
                Pass the full list every time (this replaces the previous list, it does not \
                merge). Each item needs a unique `id`, a short `text`, and a `state` of \
                \"pending\", \"in_progress\", or \"done\". Errors this tool may return: \
                `denied: missing or non-array \"items\" field`, `denied: duplicate todo id \
                \"<id>\"`, `denied: invalid state \"<state>\" for todo \"<id>\": must be \
                pending, in_progress, or done`."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "description": "The full todo list, replacing any previous list.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {"type": "string"},
                                "text": {"type": "string"},
                                "state": {
                                    "type": "string",
                                    "enum": ["pending", "in_progress", "done"]
                                }
                            },
                            "required": ["id", "text", "state"]
                        }
                    }
                },
                "required": ["items"]
            }),
            deferred: false,
            risk: Risk::ReadOnly,
            concurrency: Concurrency::Parallel,
        }
    }

    fn subject(&self, _input: &Value) -> String {
        "todo".to_string()
    }

    async fn call(&self, input: Value, _cx: &ToolCx) -> Result<ToolOutput, ToolError> {
        let items = parse_items(&input)?;
        validate(&items)?;

        let structured = Value::Array(
            items
                .iter()
                .map(|it| {
                    json!({
                        "id": it.id,
                        "text": it.text,
                        "state": it.state,
                    })
                })
                .collect(),
        );

        Ok(ToolOutput {
            text: render(&items),
            is_error: false,
            diff: None,
            structured: Some(structured),
        })
    }
}

fn parse_items(input: &Value) -> Result<Vec<TodoItem>, ToolError> {
    let arr = input
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::Denied {
            why: "missing or non-array \"items\" field".to_string(),
        })?;

    arr.iter()
        .map(|item| {
            Ok(TodoItem {
                id: str_field(item, "id")?,
                text: str_field(item, "text")?,
                state: str_field(item, "state")?,
            })
        })
        .collect()
}

/// Ids must be unique within the list and every state must be one of
/// `VALID_STATES` — both are `ToolError::Denied` (the closest existing
/// variant with room for a message; see `edit.rs`'s module docs for why no
/// dedicated variant exists).
fn validate(items: &[TodoItem]) -> Result<(), ToolError> {
    let mut seen = std::collections::HashSet::new();
    for item in items {
        if !seen.insert(item.id.as_str()) {
            return Err(ToolError::Denied {
                why: format!("duplicate todo id \"{}\"", item.id),
            });
        }
        if !VALID_STATES.contains(&item.state.as_str()) {
            return Err(ToolError::Denied {
                why: format!(
                    "invalid state \"{}\" for todo \"{}\": must be pending, in_progress, or done",
                    item.state, item.id
                ),
            });
        }
    }
    Ok(())
}

fn render(items: &[TodoItem]) -> String {
    items
        .iter()
        .map(|it| {
            let mark = match it.state.as_str() {
                "done" => "x",
                "in_progress" => "~",
                _ => " ",
            };
            format!("[{mark}] {}: {}", it.id, it.text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use cox_protocol::{ArchiveId, ArchivePut, SandboxMode, SandboxPolicy, SessionId, StoreError};
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use super::*;

    struct NoopArchive;

    #[async_trait]
    impl cox_protocol::Archive for NoopArchive {
        async fn put(&self, _put: ArchivePut) -> Result<ArchiveId, StoreError> {
            Ok(ArchiveId::new())
        }
        async fn get(&self, _id: &ArchiveId) -> Result<Vec<u8>, StoreError> {
            Ok(Vec::new())
        }
    }

    fn cx() -> ToolCx {
        let (tx, _rx) = mpsc::channel(8);
        crate::tool_cx(
            vec![std::path::PathBuf::from("/tmp")],
            std::path::PathBuf::from("/tmp"),
            SandboxPolicy {
                mode: SandboxMode::WorkspaceWrite,
                network: false,
                writable: vec![],
                readonly_in_workspace: vec![],
                linux_backend: Default::default(),
            },
            std::sync::Arc::new(NoopArchive),
            CancellationToken::new(),
            tx,
            SessionId::new(),
            cox_protocol::CallId::new(),
        )
    }

    #[tokio::test]
    async fn todo_renders_list_and_sets_structured() {
        let cx = cx();
        let out = TodoTool
            .call(
                json!({"items": [
                    {"id": "1", "text": "write tests", "state": "in_progress"},
                    {"id": "2", "text": "ship it", "state": "pending"},
                ]}),
                &cx,
            )
            .await
            .expect("todo call");

        assert!(!out.is_error);
        assert!(out.text.contains("write tests"));
        let structured = out.structured.expect("structured payload");
        assert_eq!(structured.as_array().expect("array").len(), 2);
        assert_eq!(structured[0]["state"], "in_progress");
    }

    #[tokio::test]
    async fn todo_rejects_duplicate_ids() {
        let cx = cx();
        let err = TodoTool
            .call(
                json!({"items": [
                    {"id": "1", "text": "a", "state": "pending"},
                    {"id": "1", "text": "b", "state": "done"},
                ]}),
                &cx,
            )
            .await
            .expect_err("must reject");

        match err {
            ToolError::Denied { why } => assert!(why.contains("duplicate todo id \"1\"")),
            other => panic!("expected Denied, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn todo_rejects_bad_state() {
        let cx = cx();
        let err = TodoTool
            .call(
                json!({"items": [
                    {"id": "1", "text": "a", "state": "later"},
                ]}),
                &cx,
            )
            .await
            .expect_err("must reject");

        match err {
            ToolError::Denied { why } => assert!(why.contains("invalid state \"later\"")),
            other => panic!("expected Denied, got {other:?}"),
        }
    }
}
