//! Rebuild `Message` history from a rollout `Event` stream so resume
//! assembles the same `Request` a live session would have (plan.md T2.4).

use std::collections::{HashMap, HashSet};

use cox_protocol::ids::{CallId, ItemId};
use cox_protocol::types::{
    Content, Decision, Event, ItemKind, Level, Message, PermissionMode, Role, StopReason, ToolCall,
};

/// Reconstructed transcript plus the session flags resume must restore.
#[derive(Debug, Clone, PartialEq)]
pub struct History {
    /// Model-visible messages, in order.
    pub messages: Vec<Message>,
    /// Last permission mode; `Default` until T2.2 persists a mode event.
    pub permission_mode: PermissionMode,
    /// Persistent `(tool, subject)` grants from `AllowForSession`.
    pub grants: Vec<(String, String)>,
    /// True when the caller dropped a truncated last JSONL line.
    pub truncated: bool,
}

impl History {
    /// Rebuilds history from a complete event list.
    pub fn from_events(events: &[Event]) -> Self {
        Self::from_rollout(events, false)
    }

    /// Same as [`from_events`], plus a truncation flag from the JSONL reader.
    pub fn from_rollout(events: &[Event], truncated: bool) -> Self {
        let mut dropped = HashSet::new();
        for ev in events {
            if let Event::Compacted { dropped: ids, .. } = ev {
                dropped.extend(ids.iter().copied());
            }
        }

        let mut items: HashMap<ItemId, ItemKind> = HashMap::new();
        let mut messages = Vec::new();
        let mut pending_results: Vec<Content> = Vec::new();
        let mut calls: HashMap<CallId, ToolCall> = HashMap::new();
        let mut grants = Vec::new();

        for ev in events {
            match ev {
                Event::ItemStarted { item, kind } => {
                    flush_results(&mut messages, &mut pending_results);
                    items.insert(*item, kind.clone());
                }
                Event::TextDelta { item, text } => {
                    if let Some(ItemKind::AssistantMessage { text: acc }) = items.get_mut(item) {
                        acc.push_str(text);
                    }
                }
                Event::ItemDone { item } => {
                    let Some(kind) = items.remove(item) else {
                        continue;
                    };
                    if dropped.contains(item) {
                        continue;
                    }
                    match kind {
                        ItemKind::UserMessage { text, .. } => {
                            messages.push(Message {
                                role: Role::User,
                                content: vec![Content::Text { text }],
                            });
                        }
                        ItemKind::AssistantMessage { text } if !text.is_empty() => {
                            messages.push(Message {
                                role: Role::Assistant,
                                content: vec![Content::Text { text }],
                            });
                        }
                        _ => {}
                    }
                }
                Event::ToolCallRequested { call } => {
                    calls.insert(call.id, call.clone());
                    append_tool_use(&mut messages, call);
                }
                Event::ToolCallDone { call_id, result } => {
                    pending_results.push(Content::ToolResult {
                        call_id: *call_id,
                        content: result.visible.clone(),
                        is_error: !result.ok,
                    });
                }
                Event::ApprovalDecided {
                    call_id,
                    decision: Decision::AllowForSession,
                    ..
                } => {
                    if let Some(call) = calls.get(call_id) {
                        grants.push((call.name.clone(), call.subject.clone()));
                    }
                }
                Event::TurnDone { stop, .. } => {
                    if *stop == StopReason::Interrupted {
                        pending_results.clear();
                    } else {
                        flush_results(&mut messages, &mut pending_results);
                    }
                }
                _ => {}
            }
        }
        flush_results(&mut messages, &mut pending_results);

        Self {
            messages,
            permission_mode: PermissionMode::Default,
            grants,
            truncated,
        }
    }

    /// A `Notice` to emit when the last rollout line was truncated.
    pub fn truncated_notice(&self) -> Option<Event> {
        self.truncated.then(|| Event::Notice {
            level: Level::Warn,
            text: "last rollout line was truncated and dropped".into(),
        })
    }
}

fn flush_results(messages: &mut Vec<Message>, pending: &mut Vec<Content>) {
    if pending.is_empty() {
        return;
    }
    messages.push(Message {
        role: Role::User,
        content: std::mem::take(pending),
    });
}

fn append_tool_use(messages: &mut Vec<Message>, call: &ToolCall) {
    let use_block = Content::ToolUse {
        id: call.id,
        name: call.name.clone(),
        input: call.input.clone(),
    };
    if let Some(last) = messages.last_mut()
        && last.role == Role::Assistant
    {
        last.content.push(use_block);
        return;
    }
    messages.push(Message {
        role: Role::Assistant,
        content: vec![use_block],
    });
}

#[cfg(test)]
mod tests {
    use cox_protocol::ids::{CallId, ItemId, TurnId};
    use cox_protocol::types::{ItemKind, StopReason, ToolResult};

    use super::*;

    fn user_item(id: ItemId, text: &str) -> Event {
        Event::ItemStarted {
            item: id,
            kind: ItemKind::UserMessage {
                text: text.into(),
                attachments: vec![],
            },
        }
    }

    #[test]
    fn resume_truncated_last_line_emits_notice() {
        let h = History::from_rollout(&[], true);
        let Event::Notice { level, text } = h.truncated_notice().expect("notice") else {
            panic!("expected notice");
        };
        assert_eq!(level, Level::Warn);
        assert!(text.contains("truncated"));
        assert!(History::from_events(&[]).truncated_notice().is_none());
    }

    #[test]
    fn resume_compacted_dropped_items_skipped() {
        let keep = ItemId::new();
        let drop = ItemId::new();
        let events = vec![
            user_item(drop, "old"),
            Event::ItemDone { item: drop },
            user_item(keep, "new"),
            Event::ItemDone { item: keep },
            Event::Compacted {
                summary: ItemId::new(),
                dropped: vec![drop],
                before_tokens: 10,
                after_tokens: 2,
            },
        ];
        let h = History::from_events(&events);
        assert_eq!(h.messages.len(), 1);
        assert_eq!(
            h.messages[0].content,
            vec![Content::Text { text: "new".into() }]
        );
    }

    #[test]
    fn resume_interrupt_drops_unflushed_tool_results() {
        let user = ItemId::new();
        let asst = ItemId::new();
        let call = CallId::new();
        let events = vec![
            user_item(user, "go"),
            Event::ItemDone { item: user },
            Event::ItemStarted {
                item: asst,
                kind: ItemKind::AssistantMessage {
                    text: String::new(),
                },
            },
            Event::TextDelta {
                item: asst,
                text: "x".into(),
            },
            Event::ItemDone { item: asst },
            Event::ToolCallRequested {
                call: ToolCall {
                    id: call,
                    name: "echo".into(),
                    input: serde_json::json!({}),
                    risk: cox_protocol::types::Risk::ReadOnly,
                    subject: String::new(),
                },
            },
            Event::ToolCallDone {
                call_id: call,
                result: ToolResult {
                    ok: false,
                    visible: "cancelled".into(),
                    archive: None,
                    bytes: 0,
                    duration_ms: 0,
                    diff: None,
                },
            },
            Event::TurnDone {
                turn: TurnId::new(),
                stop: StopReason::Interrupted,
            },
        ];
        let h = History::from_events(&events);
        assert_eq!(h.messages.len(), 2);
        assert!(matches!(
            h.messages[1].content.last(),
            Some(Content::ToolUse { .. })
        ));
    }
}
