# cox — instructions for agents

**What this is.** A modular terminal coding agent in Rust (the coxswain steers and calls the strokes). One binary, four surfaces: `cox` (TUI), `cox run -p` (headless, `--output-format stream-json`), `cox acp` (Agent Client Protocol server for Zed/JetBrains), `cox mcp` (built-in tools exposed as an MCP server). Every surface consumes the same `Event` stream from `cox-core`.

**Before you start.** Delegate one-off shell (build, test, git, cargo), API/HTTP calls, and file listings to a low-cost model; do not run those from the main context. `plan.md` §0 lists the decisions; do not re-argue them in a task.

**Read first.** `plan.md` — decisions, architecture, every open task with its Check. `research.md` — evidence and the fact-check ledger. `report.html` — the same evidence for humans. Do not add work that is not in `plan.md`; propose it as a plan amendment (`plan.md` §6).

**Toolchain.** Rust is pinned in `mise.toml`. Run everything as `mise exec -- cargo <cmd>` (or `mise activate`). Never install or switch a global toolchain.

## Commands

```bash
mise exec -- cargo test --workspace          # unit, snapshot and e2e; no network, no API key
mise exec -- cargo clippy --workspace --all-targets -- -D warnings
mise exec -- cargo fmt --check
mise exec -- cargo insta review              # after an intentional TUI/transcript change
COX_HOME=/tmp/cox-scratch mise exec -- cargo run -- doctor   # never against your real ~/.cox
```

## Layout

| Crate | Owns |
| --- | --- |
| `crates/cox` | clap surface and dispatch — nothing else |
| `crates/cox-protocol` | `Submission`, `Event`, `Item`, config and tool-schema types; every type that crosses a crate boundary |
| `crates/cox-core` | the agent loop as a state machine: turns, context assembly, compaction, permission engine, hooks, model routing, budget. No I/O except through traits |
| `crates/cox-provider` | `Provider` trait + Anthropic Messages, OpenAI Responses/Chat (also Ollama, vLLM, LM Studio, OpenRouter), `Replay`/`Scripted` providers for tests |
| `crates/cox-tools` | built-in tools and the sandbox (Seatbelt, Landlock/bwrap): read, edit, write, bash, grep, glob, outline, web, todo, ask_user, agent |
| `crates/cox-mcp` | MCP client over `rmcp`; `.mcp.json` / config discovery; OAuth |
| `crates/cox-store` | one SQLite file (`~/.cox/cox.db`): sessions, rollouts (JSONL), tool-output archive, memory, cost ledger |
| `crates/cox-ext` | instruction files (`AGENTS.md`/`CLAUDE.md` hierarchy), skills (`SKILL.md`), slash commands, subagent definitions, hook config |
| `crates/cox-tui` | ratatui app in TEA form (`State`, `update`, `view`); all terminal output |
| `crates/cox-acp` | Agent Client Protocol adapter over the same `Event` stream |
| `tests/` | end-to-end runs of the real binary against `COX_HOME` scratch trees and a scripted provider |

The rule that keeps crates honest: anything that talks to the network, the filesystem or a process lives behind a trait in `cox-protocol` and is implemented in `cox-provider`, `cox-tools`, `cox-mcp` or `cox-store`. If `cox-core` is about to open a socket or a file, you are in the wrong crate.

## Workflow

Tasks carry `Status:` (`open`|`in progress`) and `Model:`. Claim only `open`; set `in progress` + model; branch `cox/<task-id>`. ≤200 LOC, ≤3 files per task. Run the task's Check, then the three commands above, commit `<task-id>: <title>`. Done → move the task to `done.md` with `Status: done <date>` and its Check output. **Before you stop** (end/compaction/handoff): unfinished → `open`, `Model: -`.

## Conventions

- Every file opens with a `//!` header saying what the module owns and why it is separate. Comments explain *why*, never *what*.
- No `unwrap`, `expect`, `panic!`, `todo!` outside tests. Errors are `thiserror` enums per crate; `anyhow` only in `crates/cox`.
- Tests live in `#[cfg(test)] mod tests` at the bottom of the file, named as the claim they prove (`compaction_keeps_last_two_turns_verbatim`). Transcript and TUI tests are `insta` snapshots. A bug fix adds the narrowest regression test that fails without it.
- All terminal output goes through `cox-tui`; there is no `println!` outside it and `crates/cox`.
- No new dependency without a one-line reason in the commit message and a row in `plan.md` §1.

## Trust boundaries

Everything the model, a tool, an MCP server, a hook, a skill file or a repository writes is untrusted input. Reuse the guards that exist rather than adding new ones:

- `cox_core::permission::Engine` — the single place a tool call is allowed, denied or escalated. A tool never checks its own permission.
- `cox_tools::path::confine` — every path from the model passes through it; rejects escapes from the workspace roots.
- `cox_tools::sandbox::Policy` — a shell command runs under the platform sandbox unless the user chose `danger-full-access` for that session.
- `cox_tui::text::sanitize` — strips escape sequences and bidi overrides from anything the model or a tool prints. A tool result is the one place cox shows a whole file someone else wrote.

Simplicity never removes one of these. If a change makes a guard unnecessary, delete it deliberately and say why in the commit.

## Rules that never bend

- The core is a pure state machine: `Submission` in, `Event` out. Every surface (TUI, stream-json, ACP, rollout file) is a consumer of the same events, and a test replays events instead of calling a model.
- Lossless by default: any truncated tool output is retrievable by id (`cox expand <id>`), and the archive row exists before the model sees the shortened text.
- Cache-stable prefix: system prompt, tool schemas and instruction files are byte-stable within a session; anything volatile goes after the last cache breakpoint. A change that reorders them needs a `plan.md` amendment.
- A cost that is not a `usage` row in the ledger does not exist. Every request records model, input, output, cache read and cache write tokens.
- Compaction never rewrites the last two turns and never edits earlier turns in place; history is append-only.
- Fail open on extensions: a broken hook, skill, MCP server or plugin is warned about and skipped, never fatal.

## Models

Any provider. Runtime routing (`plan.md` D5) and agent work follow the same rule: **low-cost** (Haiku-class or a local model) for mechanical work, background agents, summaries, shell and HTTP calls, and anything a cheap model can finish; **mid-tier** (Sonnet 5, Opus 5 when the task is large) for coding; **Fable 5.1** only for hard thinking and only after the user confirms. Never switch up on your own.

## Before you call it done

1. `cargo test`, `cargo clippy`, `cargo fmt --check` clean under `mise exec`.
2. Non-trivial logic left a test behind that fails if the logic breaks.
3. You ran the real binary against a `COX_HOME` scratch tree if the change touches sessions, tools, the sandbox or config.
4. You reported what you did *not* do, if anything was skipped.
