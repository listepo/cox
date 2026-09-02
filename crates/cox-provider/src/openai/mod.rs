//! The OpenAI-shaped providers. `responses` covers the Responses API
//! (`POST /v1/responses`), which is what OpenAI's own models use; `chat`
//! covers the Chat Completions subset that local servers speak (Ollama,
//! vLLM, LM Studio, llama.cpp, OpenRouter).
//!
//! Both are separate from `anthropic` (and from each other) rather than
//! generalised behind one translator: the wire formats disagree about where
//! system text, reasoning and tool results live, and a shared abstraction
//! would have to be re-specialised at every one of those points.

pub mod chat;
pub mod responses;
