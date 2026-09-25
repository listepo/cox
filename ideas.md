# Ideas

Not approved yet. Move nothing from here into `plan.md` or `roadmap.md` without creator approval.

- Later gates from the 2026 field survey (`research.md` §8): voice input, remote control (drive a session from a phone or a second machine), Windows sandbox, agent teams / orchestration DSL, MCP Apps and MCP elicitation (the elicitation would map onto the T22.1 question modal)
- A user-scripted status line (Claude Code `statusLine` command) on top of the T28.1 segments
- A theme editor in the TUI (Crush `Ctrl+E` style) on top of T24.2 theme files
- OpenAI Chat tool calls never reach the core: `openai/chat.rs` emits `ToolUseStart` and input deltas but never `ToolUseEnd`, and `turn::consume_provider` commits a call only on `ToolUseEnd` (same bug T30.6 fixed for Anthropic). Chat interleaves parallel calls by index, so the fix is to emit each call's start/deltas/end in order once `finish_reason` arrives, plus a live fixture test. Found by reading while fixing T30.6; not yet reproduced live
- Sign in with ChatGPT (Codex-style subscription login) for the OpenAI Responses backend — only once OpenAI publishes an OAuth client flow for third-party apps; today the only flow is Codex's own client id (R§4.3.1, ledger #37)
