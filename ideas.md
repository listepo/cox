# Ideas

Not approved yet. Move nothing from here into `plan.md` or `roadmap.md` without creator approval.

- Later gates from the 2026 field survey (`research.md` §8): voice input, remote control (drive a session from a phone or a second machine), Windows sandbox, MCP Apps and MCP elicitation (the elicitation would map onto the T22.1 question modal)
- A user-scripted status line (Claude Code `statusLine` command) on top of the T28.1 segments
- A theme editor in the TUI (Crush `Ctrl+E` style) on top of T24.2 theme files
- OpenAI Chat tool calls never reach the core: `openai/chat.rs` emits `ToolUseStart` and input deltas but never `ToolUseEnd`, and `turn::consume_provider` commits a call only on `ToolUseEnd` (same bug T30.6 fixed for Anthropic). Chat interleaves parallel calls by index, so the fix is to emit each call's start/deltas/end in order once `finish_reason` arrives, plus a live fixture test. Found by reading while fixing T30.6; not yet reproduced live
- Sign in with ChatGPT (Codex-style subscription login) for the OpenAI Responses backend — only once OpenAI publishes an OAuth client flow for third-party apps; today the only flow is Codex's own client id (R§4.3.1, ledger #37)

- Fill `Capabilities.adaptive_thinking` per catalog row from models.dev `reasoning_options` via `scripts/vendor`, replacing the name rule `cox_models::supports_adaptive_thinking` (follow-up to T30.25).

- A per-`AgentDef` permission-mode override, matching Claude Code's frontmatter `permissionMode` (P34/research.md §4.3.7) — cox's permission engine is a single global trust guard by design; a per-subagent override needs its own design pass.
- Plugin-provided agent definitions (P34/research.md §4.3.7) — not mentioned anywhere in `docs/design/plugins.md`; P33's v0.1 extension-point list does not promise it.
- Manual `@mention` subagent invocation, OpenCode-style (P34/research.md §4.3.7) — a UX nicety, not something the creator asked for.
- Cursor's Cloud Agents API (`api.cursor.com`, OpenAPI at `cursor.com/docs-static/cloud-agents-openapi.yaml`, research.md §4.3.8) as a possible background-task backend — durable, server-side agent runs billed to the caller's Cursor plan, a different shape from P35's local CLI subprocess. Not approved.
- Kill a detached `bash` from an *older* turn on quit (T34.11 follow-up): cancellation is turn-scoped, so after the user sends another prompt, `interrupt()` at TUI quit no longer reaches that shell's `ToolCx::cancel`; `wait_tasks_cleared` gives up after `SHELL_CANCEL_GRACE` and the process is orphaned (reproduced with `sleep 4003`, ppid 1). A session-scoped token that detached shell tasks also watch would close it for TUI, headless `--loop`, `/clear`, fork and handoff alike.
- T30.13 cox vs Claude Code vs Terminus 2 on the same 12 Terminal-Bench 2.0 tasks with the same local model — moved from `roadmap.md` v0.2 by the creator on 2026-09-26 (the run was stopped part-way; uncommitted `evals/` work is kept in `_worktrees/cox-t30.13`). The full card (plan, Check, Done when) is in `plan.md` at commit 855fe68. Its follow-up, a re-run after the optimization and refactoring pass compared with the baseline in `research.md` §5.3, moved with it.
