//! The `Provider` trait plus the Anthropic Messages, OpenAI Responses/Chat
//! (and Ollama/vLLM/LM Studio/OpenRouter) implementations, and the
//! `Replay`/`Scripted` providers used in tests. Separate from `cox-core` so
//! the agent loop never talks to the network directly.
