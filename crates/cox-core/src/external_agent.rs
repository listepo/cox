//! The stream-json driver's line mapper (EA§5, T35.4): turns each line
//! Cursor CLI's `agent -p --output-format stream-json` prints into the
//! `Event`s cox's own loop emits for the same thing, so every surface shows
//! an external agent's run the way it shows a child session. Pure and
//! host-side, not a plugin export: T35.2 spawns the CLI, T35.5 feeds its
//! stdout here one line at a time. Only the shapes research.md §4.3.8
//! documents are mapped; anything else is a `Notice(Warn)`, never an error
//! (D14), because a CLI release that adds an event must not fail the turn.
//!
//! `cox-core` may not depend on `cox-sanitize` (`crates/cox/tests/deps.rs`),
//! so the caller hands the guard in (`cox_sanitize::sanitize`) and it stays
//! one implementation. It covers the text this module composes from the raw
//! line (notices, the error message) and `ToolCallOutput`, whose contract is
//! "already sanitised"; item text stays raw, like a model's, and is
//! sanitized where it is rendered.

use std::collections::HashMap;

use cox_protocol::errors::{CoreError, ProviderError};
use cox_protocol::ids::{CallId, ItemId, TurnId};
use cox_protocol::types::{Event, ItemKind, Level, Risk, StopReason, ToolCall, ToolResult};
use serde_json::Value;

/// Longest slice of an unrecognised line quoted in its notice: the line may
/// be a megabyte of JSON nobody wants in the transcript.
const QUOTE_CHARS: usize = 200;

/// Maps one external agent run's stream-json lines onto cox events.
pub struct StreamJsonMapper {
    turn: TurnId,
    sanitize: fn(&str) -> String,
    /// Open calls by the CLI's `call_id`, so `completed` closes the item
    /// `started` opened.
    open: HashMap<String, (ItemId, CallId)>,
}

impl StreamJsonMapper {
    /// A mapper for the run behind `turn`; `sanitize` is the terminal-text
    /// guard (`cox_sanitize::sanitize`).
    pub fn new(turn: TurnId, sanitize: fn(&str) -> String) -> Self {
        Self {
            turn,
            sanitize,
            open: HashMap::new(),
        }
    }

    /// The events one stdout line stands for; empty for a blank or `user`
    /// line (cox already recorded the outbound turn).
    pub fn map_line(&mut self, line: &str) -> Vec<Event> {
        let line = line.trim();
        if line.is_empty() {
            return Vec::new();
        }
        serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|v| self.map_value(&v))
            .unwrap_or_else(|| vec![self.unrecognised(line)])
    }

    fn map_value(&mut self, v: &Value) -> Option<Vec<Event>> {
        match (v["type"].as_str()?, v["subtype"].as_str()) {
            ("system", Some("init")) => Some(vec![self.init(v)]),
            ("user", _) => Some(Vec::new()),
            ("assistant", _) => assistant(v),
            ("tool_call", Some("started")) => {
                let (id, name, input, _) = tool(v)?;
                Some(self.open_call(id, name, input))
            }
            ("tool_call", Some("completed")) => self.completed(v),
            ("result", _) => self.result(v),
            _ => None,
        }
    }

    fn init(&self, v: &Value) -> Event {
        let field = |k: &str| v[k].as_str().unwrap_or("unknown");
        Event::Notice {
            level: Level::Info,
            text: (self.sanitize)(&format!(
                "external agent: model {}, permission mode {}",
                field("model"),
                field("permissionMode")
            )),
        }
    }

    fn open_call(&mut self, id: &str, name: String, input: Value) -> Vec<Event> {
        let item = ItemId::new();
        // cox cannot classify another agent's tool and decides nothing on
        // it here, so it records the conservative "runs something" class.
        let call = ToolCall {
            id: CallId::new(),
            name,
            input,
            risk: Risk::Exec,
            subject: String::new(),
        };
        self.open.insert(id.to_owned(), (item, call.id));
        // Surfaces draw a call from `ToolCallRequested`, the rollout keeps
        // the item: the native loop emits both, so this does too.
        vec![
            Event::ItemStarted {
                item,
                kind: ItemKind::ToolCall { call: call.clone() },
            },
            Event::ToolCallRequested { call },
        ]
    }

    fn completed(&mut self, v: &Value) -> Option<Vec<Event>> {
        let (id, name, input, result) = tool(v)?;
        // A `completed` with no `started` still becomes one whole item.
        let mut events = if self.open.contains_key(id) {
            Vec::new()
        } else {
            self.open_call(id, name, input)
        };
        let (item, call_id) = self.open.remove(id)?;
        let visible = match result {
            None => String::new(),
            Some(Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
        };
        if !visible.is_empty() {
            events.push(Event::ToolCallOutput {
                call_id,
                delta: (self.sanitize)(&visible),
            });
        }
        // The documented shape carries no success flag, so a completed call
        // counts as ok; its result text says what happened.
        events.push(Event::ToolCallDone {
            call_id,
            result: ToolResult {
                ok: true,
                bytes: visible.len() as u64,
                visible,
                archive: None,
                duration_ms: 0,
                diff: None,
            },
        });
        events.push(Event::ItemDone { item });
        Some(events)
    }

    fn result(&self, v: &Value) -> Option<Vec<Event>> {
        let turn = self.turn;
        if !v["is_error"].as_bool()? {
            return Some(vec![Event::TurnDone {
                turn,
                stop: StopReason::EndTurn,
            }]);
        }
        let message = (self.sanitize)(v["result"].as_str().unwrap_or("no detail"));
        // The external agent stands where the child's provider would, so its
        // failure is shaped as one; the native loop ends the turn after it.
        Some(vec![
            Event::Error {
                error: CoreError::Provider {
                    error: ProviderError::BadRequest { message },
                },
                fatal: false,
            },
            Event::TurnDone {
                turn,
                stop: StopReason::Error,
            },
        ])
    }

    fn unrecognised(&self, line: &str) -> Event {
        let clean = (self.sanitize)(line);
        let mut quoted: String = clean.chars().take(QUOTE_CHARS).collect();
        if quoted.len() < clean.len() {
            quoted.push('…');
        }
        Event::Notice {
            level: Level::Warn,
            text: format!("external agent: skipped unrecognised stream-json line: {quoted}"),
        }
    }
}

/// An assistant line's text parts, joined; `None` if it has none.
fn assistant(v: &Value) -> Option<Vec<Event>> {
    let parts: Vec<&str> = v["message"]["content"]
        .as_array()?
        .iter()
        .filter(|p| p["type"] == "text")
        .filter_map(|p| p["text"].as_str())
        .collect();
    if parts.is_empty() {
        return None;
    }
    let item = ItemId::new();
    Some(vec![
        Event::ItemStarted {
            item,
            kind: ItemKind::AssistantMessage {
                text: parts.concat(),
            },
        },
        Event::ItemDone { item },
    ])
}

/// A `tool_call` line's `call_id`, tool name (`readToolCall` → `read`),
/// `args` and `result`.
fn tool(v: &Value) -> Option<(&str, String, Value, Option<&Value>)> {
    let id = v["call_id"].as_str()?;
    let (key, inner) = v["tool_call"].as_object()?.iter().next()?;
    let name = key.strip_suffix("ToolCall").unwrap_or(key).to_owned();
    let input = inner.get("args").cloned().unwrap_or(Value::Null);
    Some((id, name, input, inner.get("result")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stands in for `cox_sanitize::sanitize` (not a `cox-core` dependency):
    /// proves the mapper routes the raw line through the guard it is given.
    fn strip_controls(s: &str) -> String {
        s.chars().filter(|c| !c.is_control()).collect()
    }

    fn mapper() -> StreamJsonMapper {
        StreamJsonMapper::new(TurnId::new(), strip_controls)
    }

    #[test]
    fn stream_json_assistant_line_maps_to_cox_event() {
        let events = mapper().map_line(
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello "},{"type":"text","text":"there"}]},"session_id":"s1","timestamp_ms":1}"#,
        );
        let [
            Event::ItemStarted {
                item,
                kind: ItemKind::AssistantMessage { text },
            },
            Event::ItemDone { item: done },
        ] = events.as_slice()
        else {
            panic!("{events:?}");
        };
        assert_eq!(text, "Hello there");
        assert_eq!(item, done);
    }

    #[test]
    fn stream_json_tool_call_started_and_completed_pair_map_to_one_item() {
        let mut m = mapper();
        let mut events = m.map_line(
            r#"{"type":"tool_call","subtype":"started","call_id":"c1","tool_call":{"readToolCall":{"args":{"path":"a.rs"}}},"session_id":"s1"}"#,
        );
        events.extend(m.map_line(
            r#"{"type":"tool_call","subtype":"completed","call_id":"c1","tool_call":{"readToolCall":{"args":{"path":"a.rs"},"result":"fn main() {}"}},"session_id":"s1"}"#,
        ));
        let [
            Event::ItemStarted {
                item,
                kind: ItemKind::ToolCall { call },
            },
            Event::ToolCallRequested { call: requested },
            Event::ToolCallOutput {
                call_id: out,
                delta,
            },
            Event::ToolCallDone { call_id, result },
            Event::ItemDone { item: done },
        ] = events.as_slice()
        else {
            panic!("{events:?}");
        };
        assert_eq!(call.name, "read");
        assert_eq!(call.input["path"], "a.rs");
        assert_eq!(call, requested);
        assert_eq!((out, call_id, item), (&call.id, &call.id, done));
        assert_eq!(delta, "fn main() {}");
        assert!(result.ok);
        assert_eq!(result.visible, "fn main() {}");
    }

    #[test]
    fn unrecognised_stream_json_line_becomes_a_sanitized_notice() {
        let mut m = mapper();
        for line in ["\u{1b}[2Jnot json", r#"{"type":"mystery"}"#] {
            let events = m.map_line(line);
            let [
                Event::Notice {
                    level: Level::Warn,
                    text,
                },
            ] = events.as_slice()
            else {
                panic!("{events:?}");
            };
            assert!(!text.contains('\u{1b}'), "{text}");
            assert!(text.ends_with(&strip_controls(line)), "{text}");
        }
        // Fail-open, like `broken_hook_is_skipped_not_fatal`: the run goes on.
        let next = m.map_line(r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"ok"}]},"session_id":"s1"}"#);
        assert!(matches!(next.first(), Some(Event::ItemStarted { .. })));
    }

    #[test]
    fn result_line_ends_the_turn_and_an_error_result_reports_it_first() {
        let mut m = mapper();
        let turn = m.turn;
        assert!(m.map_line(r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"hi"}]},"session_id":"s1"}"#).is_empty());
        let ok = m.map_line(r#"{"type":"result","subtype":"success","duration_ms":5,"duration_api_ms":4,"is_error":false,"result":"done","session_id":"s1"}"#);
        assert_eq!(
            ok,
            [Event::TurnDone {
                turn,
                stop: StopReason::EndTurn
            }]
        );
        let failed = m.map_line(r#"{"type":"result","subtype":"success","duration_ms":5,"duration_api_ms":4,"is_error":true,"result":"quota","session_id":"s1"}"#);
        assert!(matches!(
            &failed[..],
            [
                Event::Error { fatal: false, .. },
                Event::TurnDone {
                    stop: StopReason::Error,
                    ..
                }
            ]
        ));
    }
}
