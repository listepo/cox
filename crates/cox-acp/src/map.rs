//! `Event` → ACP `SessionUpdate` mapping (T11.2 in plan numbering, T11.1
//! task): agent message and thought chunks, tool call start/progress/done
//! with locations, and the `todo` plan. Pure over one event plus the call
//! table the forwarder keeps; approvals travel the permission-request flow
//! in `server.rs`, never as updates.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use agent_client_protocol::schema::v1::{
    ContentBlock, ContentChunk, MessageId, Plan, PlanEntry, PlanEntryPriority, PlanEntryStatus,
    SessionUpdate, TextContent, ToolCall, ToolCallContent, ToolCallId, ToolCallLocation,
    ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
};
use cox_protocol::ids::CallId;
use cox_protocol::types::{Event, Risk, StopReason as CoxStop, ToolCall as CoxCall};

/// Remembers each live call's name and subject between `ToolCallRequested`
/// and `ToolCallDone` (the latter carries only an id).
#[derive(Debug, Default)]
pub struct CallTable {
    inner: HashMap<CallId, (String, String)>,
}

impl CallTable {
    /// Empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a requested call; returns its updates (start + locations).
    pub fn requested(&mut self, call: &CoxCall, cwd: &Path) -> Vec<SessionUpdate> {
        self.inner
            .insert(call.id, (call.name.clone(), call.subject.clone()));
        let mut tool = ToolCall::new(ToolCallId::new(call.id.to_string()), &call.subject)
            .kind(kind_for(&call.name, call.risk))
            .status(ToolCallStatus::InProgress);
        tool.locations = locations_for(&call.name, &call.subject, cwd);
        tool.raw_input = Some(call.input.clone());
        vec![SessionUpdate::ToolCall(tool)]
    }

    /// Looks a finished call up by id.
    pub fn get(&self, id: &CallId) -> Option<&(String, String)> {
        self.inner.get(id)
    }

    /// Forgets a finished call.
    pub fn remove(&mut self, id: &CallId) {
        self.inner.remove(id);
    }
}

/// Maps one `Event` to zero or more updates. `todo` results become `Plan`
/// updates; approvals map to nothing here (see `server.rs`).
pub fn updates_for(calls: &mut CallTable, event: &Event, cwd: &Path) -> Vec<SessionUpdate> {
    match event {
        Event::TextDelta { item, text } => {
            let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(text.clone())))
                .message_id(MessageId::new(item.to_string()));
            vec![SessionUpdate::AgentMessageChunk(chunk)]
        }
        Event::ThinkingDelta { item, text } => {
            let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(text.clone())))
                .message_id(MessageId::new(item.to_string()));
            vec![SessionUpdate::AgentThoughtChunk(chunk)]
        }
        Event::ToolCallRequested { call } => calls.requested(call, cwd),
        // Streaming stdout would spam one update per delta; the Done update
        // carries the result.
        Event::ToolCallOutput { .. } => vec![],
        Event::ToolCallDone { call_id, result } => {
            let name = calls
                .get(call_id)
                .map(|(n, _)| n.clone())
                .unwrap_or_default();
            calls.remove(call_id);
            let mut fields = ToolCallUpdateFields::new().status(Some(if result.ok {
                ToolCallStatus::Completed
            } else {
                ToolCallStatus::Failed
            }));
            fields.content = Some(vec![ToolCallContent::from(ContentBlock::Text(
                TextContent::new(result.visible.clone()),
            ))]);
            let update = ToolCallUpdate::new(ToolCallId::new(call_id.to_string()), fields);
            let mut out = vec![SessionUpdate::ToolCallUpdate(update)];
            if name == "todo"
                && result.ok
                && let Some(plan) = plan_from(&result.visible)
            {
                out.push(SessionUpdate::Plan(plan));
            }
            out
        }
        _ => vec![],
    }
}

/// cox stop reasons to ACP stop reasons. `Budget` and `Error` have no ACP
/// counterpart, so the driver sends the detail as a final message chunk and
/// reports `Refusal`.
pub fn map_stop(stop: &CoxStop) -> agent_client_protocol::schema::v1::StopReason {
    use agent_client_protocol::schema::v1::StopReason as AcpStop;
    match stop {
        CoxStop::EndTurn => AcpStop::EndTurn,
        CoxStop::MaxTurns => AcpStop::MaxTurnRequests,
        CoxStop::Interrupted => AcpStop::Cancelled,
        CoxStop::Budget | CoxStop::Refusal { .. } | CoxStop::Error => AcpStop::Refusal,
    }
}

/// Final message chunk for stops that carry detail, if any.
pub fn stop_detail(stop: &CoxStop) -> Option<String> {
    match stop {
        CoxStop::Refusal { detail } => Some(detail.clone()),
        CoxStop::Budget => Some("budget limit reached".to_string()),
        CoxStop::Error => Some("turn failed".to_string()),
        _ => None,
    }
}

fn kind_for(name: &str, risk: Risk) -> ToolKind {
    match name {
        "read" => ToolKind::Read,
        "edit" | "write" | "apply_patch" => ToolKind::Edit,
        "grep" | "glob" => ToolKind::Search,
        "bash" => ToolKind::Execute,
        "todo" => ToolKind::Think,
        "web_fetch" | "web-fetch" => ToolKind::Fetch,
        _ if name.starts_with("mcp__") => ToolKind::Other,
        _ => match risk {
            Risk::ReadOnly => ToolKind::Read,
            Risk::Write => ToolKind::Edit,
            Risk::Exec => ToolKind::Execute,
            Risk::Destructive => ToolKind::Delete,
        },
    }
}

/// Absolute file locations for file-shaped calls, so clients can follow
/// along; anything else gets no locations.
fn locations_for(name: &str, subject: &str, cwd: &Path) -> Vec<ToolCallLocation> {
    if !matches!(name, "read" | "edit" | "write" | "apply_patch") || subject.is_empty() {
        return Vec::new();
    }
    let path: PathBuf = if Path::new(subject).is_absolute() {
        PathBuf::from(subject)
    } else {
        cwd.join(subject)
    };
    vec![ToolCallLocation::new(path)]
}

/// Parses the `todo` tool's `[mark] id: text` list into a `Plan`.
fn plan_from(visible: &str) -> Option<Plan> {
    let entries: Vec<PlanEntry> = visible
        .lines()
        .filter_map(|line| {
            let (mark, rest) = line.strip_prefix('[')?.split_once("] ")?;
            let (_, text) = rest.split_once(": ")?;
            let status = match mark {
                "x" => PlanEntryStatus::Completed,
                "~" => PlanEntryStatus::InProgress,
                _ => PlanEntryStatus::Pending,
            };
            Some(PlanEntry::new(
                text.to_string(),
                PlanEntryPriority::Medium,
                status,
            ))
        })
        .collect();
    (!entries.is_empty()).then(|| Plan::new(entries))
}
