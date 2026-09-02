//! `expand`: retrieves archived tool output by id, with optional line bounds.
//! It is separate from archive storage so all access remains behind `Archive`.

use async_trait::async_trait;
use cox_protocol::{ArchiveId, Concurrency, Risk, Tool, ToolCx, ToolError, ToolOutput, ToolSpec};
use serde_json::{Value, json};

/// Reads a bounded selection from a prior archived tool result.
pub struct ExpandTool;

#[async_trait]
impl Tool for ExpandTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec { name: "expand".into(), description: "Retrieve archived tool output by archive id; optionally select a 1-based inclusive line range.".into(), input_schema: json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"},"lines":{"type":"string"}}}), deferred: false, risk: Risk::ReadOnly, concurrency: Concurrency::Parallel }
    }
    fn subject(&self, input: &Value) -> String {
        input
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into()
    }
    async fn call(&self, input: Value, cx: &ToolCx) -> Result<ToolOutput, ToolError> {
        let raw = self.subject(&input);
        let id: ArchiveId = raw.parse().map_err(|_| ToolError::Denied {
            why: "invalid archive id".into(),
        })?;
        let bytes = cx.archive.get(&id).await.map_err(|_| ToolError::NotFound)?;
        let text = String::from_utf8_lossy(&bytes);
        let lines = input
            .get("lines")
            .and_then(Value::as_str)
            .and_then(parse_range);
        let text = match lines {
            Some((start, end)) => text
                .lines()
                .enumerate()
                .filter(|(i, _)| *i + 1 >= start && *i + 1 <= end)
                .map(|(i, line)| format!("{}\t{line}", i + 1))
                .collect::<Vec<_>>()
                .join("\n"),
            None => text.into_owned(),
        };
        Ok(ToolOutput {
            text,
            is_error: false,
            diff: None,
            structured: None,
        })
    }
}
fn parse_range(s: &str) -> Option<(usize, usize)> {
    let (a, b) = s.split_once('-')?;
    let (a, b) = (a.parse().ok()?, b.parse().ok()?);
    (a > 0 && a <= b).then_some((a, b))
}
