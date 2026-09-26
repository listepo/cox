//! Scenario data for the `Scripted` provider: the TOML `[[turn]]` format
//! parsed into [`TurnSpec`]/[`ToolCallSpec`], and the pure event sequence
//! each turn expands to. Split out of `cox-provider::scripted` (T32.11) so
//! a scenario can be parsed and turned into events with no network deps;
//! the `Scripted` provider that streams those events over a channel and
//! estimates usage still lives in `cox-provider::scripted`, because it
//! needs `cox-provider::tokens::estimate`.
//!
//! **Format.** A scenario is one TOML file with one `[[turn]]` table per
//! provider call, in the order the test wants them:
//!
//! ```toml
//! [[turn]]
//! text = "I'll read that file."
//! [[turn.tool_calls]]
//! name = "read"
//! input = { path = "src/main.rs" }
//!
//! [[turn]]
//! tool_calls = [
//!   { name = "read", input = { path = "a.rs" } },
//!   { name = "glob", input = { pattern = "*.rs" } },
//! ]
//! ```
//!
//! `input` is a TOML inline table, translated to the call's JSON input.
//! A turn with no `tool_calls` ends the call without tool use. Per the
//! §1.2 StopReason convention already established by the real providers,
//! [`events_for`] only ever reports `EndTurn` — tool use is detected from
//! the `ToolUseStart` events, not from the stop reason, so a scripted turn
//! reports `EndTurn` even on a tool-call turn.

use cox_protocol::errors::ProviderError;
use cox_protocol::ids::CallId;
use cox_protocol::types::{ModelId, ProviderEvent, StopReason, Usage};

use figment::Figment;
use figment::providers::{Format, Toml};

/// One scripted tool call: the tool's name and its TOML inline table of
/// arguments, already translated to JSON.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ToolCallSpec {
    /// The tool name, matching a `ToolSpec.name`.
    pub name: String,
    /// The tool input.
    #[serde(default)]
    pub input: serde_json::Value,
}

/// One provider call in a scenario: optional assistant text, optional tool
/// calls.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TurnSpec {
    /// Assistant text to stream as one `TextDelta`, if any.
    #[serde(default)]
    pub text: Option<String>,
    /// Tool calls to emit for this call, in order.
    #[serde(default)]
    pub tool_calls: Vec<ToolCallSpec>,
    /// If set, the stream emits `ProviderEvent::Error` after any text/tools
    /// and `stream` returns `BadRequest` — a mid-stream failure (T2.1).
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(serde::Deserialize)]
struct Scenario {
    #[serde(default)]
    turn: Vec<TurnSpec>,
}

/// Parses a scenario TOML document into the turns `Scripted` replays.
pub fn parse_scenario(toml_text: &str) -> Result<Vec<TurnSpec>, ProviderError> {
    Figment::from(Toml::string(toml_text))
        .extract::<Scenario>()
        .map(|s| s.turn)
        .map_err(|e| ProviderError::BadRequest {
            message: format!("invalid scripted scenario: {e}"),
        })
}

/// One scripted call's events, in wire order: message start, then per call
/// `Start` / `InputDelta` / `End`, then `Stop`. `usage` is also an event
/// (§1.2), emitted last, already computed by the caller (`Scripted` needs
/// `tokens::estimate` for that, which is why it stays in `cox-provider`).
pub fn events_for(turn: &TurnSpec, model: ModelId, usage: Usage) -> Vec<ProviderEvent> {
    let mut events = vec![ProviderEvent::MessageStart { model }];
    if let Some(text) = &turn.text {
        events.push(ProviderEvent::TextDelta { text: text.clone() });
    }
    for call in &turn.tool_calls {
        events.push(ProviderEvent::ToolUseStart {
            id: CallId::new(),
            name: call.name.clone(),
        });
        events.push(ProviderEvent::ToolUseInputDelta {
            text: call.input.to_string(),
        });
        events.push(ProviderEvent::ToolUseEnd);
    }
    if let Some(message) = &turn.error {
        events.push(ProviderEvent::Error {
            error: ProviderError::BadRequest {
                message: message.clone(),
            },
        });
        return events;
    }
    events.push(ProviderEvent::Stop {
        stop: StopReason::EndTurn,
    });
    events.push(ProviderEvent::Usage { usage });
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_parses_tool_call_forms() {
        let toml = r#"
[[turn]]
text = "starting."
[[turn.tool_calls]]
name = "read"
input = { path = "src/main.rs" }

[[turn]]
tool_calls = [
  { name = "read", input = { path = "a.rs" } },
  { name = "glob", input = { pattern = "*.rs" } },
]
"#;
        let turns = parse_scenario(toml).expect("parses");
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].text.as_deref(), Some("starting."));
        assert_eq!(turns[0].tool_calls[0].input["path"], "src/main.rs");
        assert_eq!(turns[1].tool_calls.len(), 2);
        assert_eq!(turns[1].tool_calls[1].input["pattern"], "*.rs");
    }

    #[test]
    fn parse_scenario_rejects_bad_toml() {
        assert!(parse_scenario("[[turn][").is_err());
    }

    #[test]
    fn events_for_reports_end_turn_even_on_tool_call() {
        let turn = TurnSpec {
            text: None,
            tool_calls: vec![ToolCallSpec {
                name: "read".into(),
                input: serde_json::json!({"path": "a.rs"}),
            }],
            error: None,
        };
        let usage = Usage {
            input_tokens: 1,
            output_tokens: 1,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            estimated: true,
            cost_usd: 0.0,
            latency_ms: 0,
        };
        let events = events_for(&turn, ModelId("m".into()), usage);
        assert!(events.iter().any(|e| matches!(
            e,
            ProviderEvent::Stop {
                stop: StopReason::EndTurn
            }
        )));
    }
}
