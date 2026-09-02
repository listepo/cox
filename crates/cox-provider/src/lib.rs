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
pub mod sse;
pub mod tokens;
