//! The `Provider` trait implementations: the Anthropic Messages API, the
//! OpenAI Responses/Chat APIs (also Ollama, vLLM, LM Studio, OpenRouter) and
//! the `Replay`/`Scripted` fakes used by tests. Separate from `cox-core` so
//! the agent loop never talks to the network directly (AGENTS.md, D3).
//!
//! Each backend is split the same way: a *pure* request translator (a
//! `Request` in, a `serde_json::Value` body out — no I/O, so it snapshot
//! tests without a key) next to the streaming client that sends it. The
//! wire formats live here and nowhere else; nothing above this crate knows
//! what `cache_control` or `output_config` are.
//!
//! - [`anthropic`] — Messages API: cache breakpoints, adaptive thinking, effort, refusal fallbacks.
//! - [`sse`] — generic Server-Sent-Events framing shared by every SSE-based provider.

#![warn(missing_docs)]

pub mod anthropic;
pub mod openai;
pub mod replay;
pub mod scripted;
pub mod sse;
pub mod tokens;
pub mod usage;

use cox_protocol::errors::ProviderError;
use cox_protocol::traits::Provider;

/// Builds a test-double [`Provider`] from `COX_PROVIDER`.
///
/// `None` means construct a real provider. `scripted` needs `COX_SCENARIO`;
/// `replay` needs `COX_CASSETTES`.
pub fn from_env() -> Result<Option<Box<dyn Provider>>, ProviderError> {
    match std::env::var("COX_PROVIDER") {
        Ok(name) if name.eq_ignore_ascii_case("scripted") => {
            Ok(Some(Box::new(scripted::Scripted::from_env()?)))
        }
        Ok(name) if name.eq_ignore_ascii_case("replay") => {
            Ok(Some(Box::new(replay::Replay::from_env()?)))
        }
        Ok(name) if name.is_empty() => Ok(None),
        Ok(name) => Err(ProviderError::Unsupported {
            feature: format!("COX_PROVIDER={name}"),
        }),
        Err(_) => Ok(None),
    }
}
