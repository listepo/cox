//! The `Scripted` provider: serves a TOML scenario one entry per provider
//! call, in order, verbatim, so the loop and every test run with no network
//! and no key (D12).
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
//! §1.2 StopReason convention already established by the real providers, a
//! provider only ever reports `EndTurn`/`Refusal` — tool use is detected
//! from the `ToolUseStart` events, not from the stop reason, so `Scripted`
//! reports `EndTurn` even on a tool-call turn.
//!
//! **The scenario is authoritative.** If the loop asks for more calls than
//! the scenario supplies, `stream` returns `ProviderError::Unsupported` —
//! the scenario is a test's expectation of exactly how many provider calls
//! its turn should make; a silent fallthrough would be a bug credited to a
//! network round-trip that never showed up in tests.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use cox_protocol::errors::ProviderError;
use cox_protocol::ids::CallId;
use cox_protocol::traits::Provider;
use cox_protocol::types::{Caps, ModelId, ProviderEvent, ProviderId, Request, StopReason, Usage};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use figment::Figment;
use figment::providers::{Format, Toml};

use crate::tokens::estimate;

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

/// A scripted provider: replays a [`TurnSpec`] per provider call, in order.
/// `turns` is a `Mutex<VecDeque>` because `Provider::stream` takes `&self`
/// and the state machine owns no other interior mutability.
pub struct Scripted {
    turns: Mutex<VecDeque<TurnSpec>>,
    /// The model id reported in `MessageStart`; empty means echo the
    /// request's model, which is what loop tests usually want.
    model: String,
}

impl Scripted {
    /// Builds one from a scenario document.
    pub fn from_toml(toml_text: &str, model: impl Into<String>) -> Result<Self, ProviderError> {
        Ok(Self {
            turns: Mutex::new(parse_scenario(toml_text)?.into()),
            model: model.into(),
        })
    }

    /// Loads a scenario file (`COX_SCENARIO` for `from_env`).
    pub fn from_path(
        path: impl AsRef<Path>,
        model: impl Into<String>,
    ) -> Result<Self, ProviderError> {
        let path = path.as_ref();
        let toml_text = std::fs::read_to_string(path).map_err(|e| ProviderError::BadRequest {
            message: format!("scripted scenario {}: {e}", path.display()),
        })?;
        Self::from_toml(&toml_text, model)
    }

    /// `COX_PROVIDER=scripted` plus `COX_SCENARIO=<path>`.
    pub fn from_env() -> Result<Self, ProviderError> {
        let path = std::env::var("COX_SCENARIO").map_err(|_| ProviderError::BadRequest {
            message: "COX_SCENARIO is required when COX_PROVIDER=scripted".into(),
        })?;
        Self::from_path(path, "")
    }

    /// One scripted call's events, in wire order: message start, then per
    /// call `Start` / `InputDelta` / `End`, then `Stop`. `Usage` is also an
    /// event (§1.2), emitted last, echoed in the return value.
    fn events_for(turn: &TurnSpec, model: ModelId, usage: Usage) -> Vec<ProviderEvent> {
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

    /// Estimated usage for the ledger for a scripted call: input = the
    /// request estimate (the same heuristic budget pre-checks use), output
    /// = bytes of the scripted text+inputs over the usual ~4 chars/token.
    /// `estimated` stays `true` — this is not a real bill.
    fn usage_for(turn: &TurnSpec, req: &Request) -> Usage {
        let input = estimate(req).tokens;
        let out_bytes = turn.text.as_deref().unwrap_or_default().len()
            + turn
                .tool_calls
                .iter()
                .map(|c| c.input.to_string().len())
                .sum::<usize>();
        Usage {
            input_tokens: input,
            output_tokens: (out_bytes as f64 / 4.0).ceil() as u32,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            estimated: true,
            cost_usd: 0.0,
            latency_ms: 0,
        }
    }
}

#[async_trait]
impl Provider for Scripted {
    fn id(&self) -> ProviderId {
        // A scripted run bills nothing; `Local` keeps the ledger row's
        // provider column honest (never matched to a price table).
        ProviderId::Local
    }

    fn capabilities(&self) -> Caps {
        Caps {
            cache: false,
            thinking: false,
            server_tools: false,
            count_tokens: true, // the byte-cost estimate is free
            max_context: u32::MAX,
        }
    }

    async fn stream(
        &self,
        req: Request,
        sink: mpsc::Sender<ProviderEvent>,
        cancel: CancellationToken,
    ) -> Result<Usage, ProviderError> {
        let turn = self
            .turns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
            .ok_or_else(|| ProviderError::Unsupported {
                feature: "scripted scenario ran out".into(),
            })?;
        let usage = Self::usage_for(&turn, &req);
        let model = if self.model.is_empty() {
            req.model.clone()
        } else {
            ModelId(self.model.clone())
        };
        for event in Self::events_for(&turn, model, usage) {
            // Same channel discipline as the real providers: honour cancel
            // before each send; bail if the receiving end hung up.
            if cancel.is_cancelled() {
                return Err(ProviderError::Cancelled);
            }
            if sink.send(event).await.is_err() {
                return Err(ProviderError::Cancelled);
            }
        }
        if let Some(message) = turn.error {
            return Err(ProviderError::BadRequest { message });
        }
        Ok(usage)
    }

    async fn count_tokens(&self, req: &Request) -> Result<u32, ProviderError> {
        Ok(estimate(req).tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    use cox_protocol::types::{Effort, Job, Thinking, Tier};

    fn req() -> Request {
        Request {
            tier: Tier::Code,
            job: Job::Main,
            model: ModelId("req-model".into()),
            system: vec![],
            tools: vec![],
            messages: vec![],
            effort: Effort::High,
            max_tokens: 1024,
            thinking: Thinking::Off,
            cache_breakpoints: vec![],
            stop_sequences: vec![],
        }
    }

    async fn drain(provider: &Scripted, req: Request) -> Result<Vec<ProviderEvent>, ProviderError> {
        let (tx, mut rx) = mpsc::channel(64);
        provider.stream(req, tx, CancellationToken::new()).await?;
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        Ok(events)
    }

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

    #[tokio::test]
    async fn scripted_text_turn_streams_events_in_order() {
        let provider =
            Scripted::from_toml("[[turn]]\ntext = \"hi there\"\n", "").expect("scenario");
        let events = drain(&provider, req()).await.expect("in-scenario");
        let kinds: Vec<&str> = events
            .iter()
            .map(|e| match e {
                ProviderEvent::MessageStart { .. } => "message_start",
                ProviderEvent::TextDelta { .. } => "text_delta",
                ProviderEvent::Stop { .. } => "stop",
                ProviderEvent::Usage { .. } => "usage",
                _ => "other",
            })
            .collect();
        assert_eq!(kinds, ["message_start", "text_delta", "stop", "usage"]);
        assert!(matches!(
            &events[0],
            ProviderEvent::MessageStart { model } if model.0 == "req-model"
        ));
        match &events[3] {
            ProviderEvent::Usage { usage } => assert!(usage.estimated),
            other => panic!("last event must be usage: {other:?}"),
        }
    }

    #[tokio::test]
    async fn scripted_tool_turn_emits_start_delta_end() {
        let toml = "[[turn]]\ntool_calls = [{ name = \"read\", input = { path = \"a.rs\" } }]\n";
        let provider = Scripted::from_toml(toml, "").expect("scenario");
        let events = drain(&provider, req()).await.expect("in-scenario");
        assert!(events.iter().any(|e| matches!(
            e,
            ProviderEvent::ToolUseStart { name, .. } if name == "read"
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            ProviderEvent::ToolUseInputDelta { text } if text.contains("a.rs")
        )));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ProviderEvent::ToolUseEnd))
        );
        // §1.2: providers report EndTurn even after tool use.
        assert!(events.iter().any(|e| matches!(
            e,
            ProviderEvent::Stop {
                stop: StopReason::EndTurn
            }
        )));
    }

    #[tokio::test]
    async fn scripted_model_override_wins_over_request() {
        let provider =
            Scripted::from_toml("[[turn]]\ntext = \"x\"\n", "script-model").expect("scenario");
        let events = drain(&provider, req()).await.expect("in-scenario");
        assert!(matches!(
            &events[0],
            ProviderEvent::MessageStart { model } if model.0 == "script-model"
        ));
    }

    #[tokio::test]
    async fn scripted_streams_turns_in_order_then_unsupported() {
        let toml = "[[turn]]\ntext = \"one\"\n[[turn]]\ntext = \"two\"\n";
        let provider = Scripted::from_toml(toml, "").expect("scenario");
        let one = drain(&provider, req()).await.expect("turn 1");
        let two = drain(&provider, req()).await.expect("turn 2");
        assert!(one.iter().any(|e| matches!(
            e,
            ProviderEvent::TextDelta { text } if text == "one"
        )));
        assert!(two.iter().any(|e| matches!(
            e,
            ProviderEvent::TextDelta { text } if text == "two"
        )));
        let err = drain(&provider, req())
            .await
            .expect_err("scenario is exhausted");
        assert!(matches!(err, ProviderError::Unsupported { .. }));
    }

    #[tokio::test]
    async fn scripted_cancel_before_first_send_errs() {
        let provider = Scripted::from_toml("[[turn]]\ntext = \"x\"\n", "").expect("scenario");
        let cancel = CancellationToken::new();
        cancel.cancel();
        let err = provider
            .stream(req(), mpsc::channel(8).0, cancel)
            .await
            .expect_err("cancelled");
        assert!(matches!(err, ProviderError::Cancelled));
    }

    #[tokio::test]
    async fn scripted_count_tokens_uses_estimate() {
        let provider = Scripted::from_toml("[[turn]]\ntext = \"x\"\n", "").expect("scenario");
        assert!(provider.capabilities().count_tokens);
        assert!(!provider.capabilities().cache);
        let n = provider.count_tokens(&req()).await.expect("estimate");
        assert_eq!(n, crate::tokens::estimate(&req()).tokens);
    }

    #[test]
    fn no_secrets_in_fixtures() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut hits = Vec::new();
        for dir in ["fixtures", "cassettes"] {
            walk_secrets(&root.join(dir), &mut hits);
        }
        assert!(
            hits.is_empty(),
            "secret-shaped bytes in {}",
            hits.iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    fn walk_secrets(dir: &Path, hits: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk_secrets(&path, hits);
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if crate::replay::redact_secrets(&text) != text {
                hits.push(path);
            }
        }
    }
}
