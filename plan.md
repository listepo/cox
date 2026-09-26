# cox

https://github.com/listepo/cox

A modular terminal coding agent in Rust (coxswain: steers work while models, tools, and extensions row). TUI, headless, ACP, MCP.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T30.13 | in progress | P3 | 3 | 5% | Claude Code / claude-sonnet-5 |
| T32.2 | todo | P2 | 4 | 0% | |
| T33.6 | todo | P1 | 4 | 0% | |
| T33.7 | todo | P2 | 3 | 0% | |
| T33.8 | todo | P2 | 3 | 0% | |
| T33.9 | todo | P2 | 4 | 0% | |
| T33.10 | todo | P2 | 4 | 0% | |
| T33.11 | todo | P2 | 4 | 0% | |
| T33.12 | todo | P2 | 4 | 0% | |
| T33.13 | todo | P2 | 4 | 0% | |
| T33.14 | todo | P2 | 4 | 0% | |
| T33.15 | todo | P2 | 3 | 0% | |
| T33.16 | todo | P2 | 3 | 0% | |
| T33.17 | todo | P2 | 3 | 0% | |
| T33.18 | todo | P2 | 5 | 0% | |
| T33.19 | todo | P2 | 4 | 0% | |
| T33.20 | todo | P2 | 5 | 0% | |
| T33.21 | todo | P2 | 4 | 0% | |
| T33.23 | todo | P2 | 4 | 0% | |
| T33.24 | todo | P2 | 3 | 0% | |
| T33.25 | todo | P2 | 3 | 0% | |
| T33.26 | todo | P2 | 4 | 0% | |
| T33.27 | todo | P2 | 4 | 0% | |
| T33.28 | todo | P2 | 4 | 0% | |
| T33.29 | todo | P2 | 3 | 0% | |
| T33.30 | todo | P2 | 2 | 0% | |
| T33.31 | todo | P2 | 4 | 0% | |
| T33.32 | todo | P2 | 3 | 0% | |
| T33.33 | todo | P2 | 3 | 0% | |
| T33.34 | todo | P2 | 4 | 0% | |
| T33.35 | todo | P3 | 2 | 0% | |
| T33.36 | todo | P2 | 4 | 0% | |
| T33.37 | todo | P3 | 1 | 0% | |
| T33.38 | todo | P2 | 3 | 0% | |
| T33.39 | todo | P2 | 3 | 0% | |
| T33.40.1 | todo | P1 | 5 | 0% | |
| T33.40.2 | todo | P2 | 3 | 0% | |
| T33.40.3 | todo | P2 | 4 | 0% | |
| T33.40.4 | todo | P2 | 4 | 0% | |
| T33.40.5 | todo | P2 | 3 | 0% | |
| T33.40.6 | todo | P2 | 4 | 0% | |
| T33.40.7 | todo | P2 | 3 | 0% | |
| T33.40.8 | todo | P2 | 4 | 0% | |
| T33.40.9 | todo | P2 | 4 | 0% | |
| T33.40.10 | todo | P3 | 3 | 0% | |
| T33.40.11 | todo | P2 | 2 | 0% | |
| T33.40.12 | todo | P2 | 3 | 0% | |
| T33.40.13 | todo | P2 | 3 | 0% | |
| T33.40.14 | todo | P2 | 3 | 0% | |
| T33.40.15 | todo | P2 | 3 | 0% | |
| T33.40.16 | todo | P2 | 2 | 0% | |
| T33.40.17 | todo | P3 | 2 | 0% | |
| T33.41 | todo | P3 | 2 | 0% | |
| T33.42 | todo | P2 | 3 | 0% | |
| T33.43 | todo | P1 | 2 | 0% | |
| T34.6 | in progress | P1 | 3 | 5% | Claude Code / claude-sonnet-5 |
| T34.8 | in progress | P2 | 2 | 5% | Claude Code / claude-sonnet-5 |
| T34.9 | todo | P1 | 3 | 0% | |
| T35.2 | todo | P1 | 4 | 0% | |
| T35.3 | todo | P2 | 5 | 0% | |
| T35.4 | todo | P2 | 4 | 0% | |
| T35.5 | todo | P1 | 4 | 0% | |
| T35.6 | todo | P2 | 2 | 0% | |
| T35.7 | todo | P2 | 4 | 0% | |
| T35.8 | todo | P2 | 2 | 0% | |
| T35.9 | todo | P2 | 2 | 0% | |
| T35.10 | todo | P3 | 2 | 0% | |

## Reference

Name: **cox** — the coxswain steers the boat and calls the strokes; the crew (models, tools, MCP servers) does the rowing. Binary `cox`, crates `cox-*`, home `~/.cox/`.

How to read this file: §0 decisions are settled; §1 is the design every task must conform to (types, schemas, algorithms, surfaces); §2 is how a task is worked; numbered tasks that are finished live in `done.md`; later approved work is in `roadmap.md`; §6 amendments; §7 risks. Active work, when any, is the table at the top of this file.

## 0. Decisions (read before any task)

| # | Decision | Why (evidence in research.md) |
|---|----------|-------------------------------|
| D1 | **One Cargo workspace, one static binary. A module is its own crate when it alone uses a heavy or platform-gated dependency, is a trust guard, is a ≥ 500-LOC leaf, or is needed by another crate without the rest of its own (`docs/design/crates.md`, A47); `crates/cox/tests/deps.rs` holds the graph. No dylib plugin host. One WASM plugin host (extism) from v0.2: `docs/design/plugins.md`; it reaches the core only through traits in `cox-protocol`.** Extensibility in v0.1 is *data and processes*: instruction files, `SKILL.md`, command and subagent markdown, hook subprocesses, MCP servers. A WASM host (extism) is v0.2. | Claude Code, Codex, Gemini CLI and Copilot all reach their ecosystems through markdown + hooks + MCP, not through in-process plugins (R§2). A plugin ABI is the one thing that cannot be changed later; defer it until the `Tool`/`Event` contract has survived a release. |
| D2 | **The core is a pure state machine: `Submission` in, `Event` out.** `cox-core` owns turns, context assembly, permissions, routing, compaction. It never touches the network, filesystem or a process except through traits defined in `cox-protocol`. TUI, `stream-json`, ACP and the JSONL rollout are four consumers of one event stream. | Codex's SQ/EQ protocol is the reason it ships a TUI, an `exec` mode, an app-server for IDEs and an MCP server from one core (R§1.2). It is also what makes the loop testable without a model: a scripted provider plus a golden event log. |
| D3 | **Own thin provider layer; no LLM framework crate.** `cox-provider` implements the Anthropic Messages API (streaming, tool use, `cache_control`, adaptive thinking, `effort`, `fallbacks`, `count_tokens`), the OpenAI Responses API, and OpenAI Chat Completions (Ollama, vLLM, LM Studio, llama.cpp, OpenRouter, DeepSeek). SSE via `eventsource-stream`. **Where wire types come from (A40):** (1) a maintained Rust SDK's *types* when one exists (OpenAI: `async-openai` types only), else (2) types generated with typify from the vendor's published spec, vendored in the repo (Anthropic), else (3) hand-written. Transport, retry, SSE state machine, `ProviderEvent` mapping and the ledger stay ours in every case; SDK code is a `wire` module inside the provider, extracted to a crate only when a second consumer appears. Login: API keys only — Claude subscription OAuth is forbidden to third parties (R§4.3.1); ChatGPT login waits for an OpenAI document permitting it. | rig/genai lag the wire formats that decide cost: cache breakpoints, thinking-block replay, server tools, per-message effort, refusal fallbacks (R§4.3). Each provider is ~500 LOC; a framework is a dependency on someone else's release cadence. Codex hand-rolls its client too and ships `eventsource-stream 0.2.3` (R§1.3). |
| D4 | **Adopt existing formats verbatim instead of inventing ones.** `AGENTS.md` (and `CLAUDE.md`) hierarchy; Agent Skills `SKILL.md`; Claude Code hook JSON protocol and `.claude/settings.json` permission-rule syntax (`Bash(npm run test:*)`), `.claude/commands/*.md`, `.claude/agents/*.md`; `.mcp.json`; Codex `apply_patch` (V4A) grammar; `--output-format stream-json`. cox-native equivalents live under `.cox/` with the same schemas. | A user with a Claude Code or Codex setup gets cox for free, and the rtok hook stack works unchanged (R§3). Every one of these is documented and already read by ≥ 2 agents. |
| D5 | **Route by job tier, never by guesswork, never up.** Three tiers in config: `cheap` (default `claude-haiku-4-5`; any local model), `code` (default `claude-sonnet-5`; `claude-opus-5` when the user picks it or the task is flagged large), `think` (`claude-fable-5-1`, only via `/think` or `--deep`, always confirmed). Jobs pinned to `cheap`: session title, compaction summary, tool-result summarisation, commit message, memory extraction, explore/search subagents, background shell and HTTP subagents, hook-driven LLM calls. Every request carries a `job` tag into the ledger. | User constraint. Claude Code's silent Haiku delegation is its most-cited complaint (R§2.1); Copilot's auto-routing is praised because it is explicit and discounted. Anthropic's own guidance: measure the capable model at lower `effort` before building a cascade, because caches are model-scoped (R§4.4). |
| D6 | **Token economy is core, not a plugin.** (a) every tool output is archived before the model sees it; the model sees head/tail + `expand <id>`; (b) identical read/grep within N turns returns "unchanged, see #id"; (c) `read` has `lines=` and `mode=outline` (tree-sitter); (d) tool schemas beyond the core eight are deferred and found through a `tool_search` tool; (e) prefix is byte-stable: tools → system → instruction files → last cache breakpoint → volatile; (f) compaction is append-only, keeps the last two turns verbatim, runs on `cheap`; (g) one per-request `usage` row with cache read/write; (h) session and monthly budget caps. Metric: *context-token-turns*. | rtok measured 3–40 % real savings from external hooks against 60–95 % vendor claims; the difference is that hooks cannot touch what the model sees. A native agent can (R§4.1–4.2). Minimum cacheable prefix is 512 tokens on the Claude 5 family and 4 096 on Haiku 4.5, so one volatile byte in the system prompt costs the whole cache (R§6 ledger #21). |
| D7 | **Sandbox on by default.** macOS: Seatbelt profiles via `sandbox-exec`. Linux: bubblewrap when present, else Landlock + seccomp. Sandbox modes `read-only` / `workspace-write` / `danger-full-access`; approval policies `untrusted` / `on-request` / `on-failure` / `never` (Codex vocabulary) combined with Claude-style allow/deny rules. `.git` and `.cox` stay read-only inside `workspace-write`. Windows: no sandbox, loud warning, `on-request` forced. | Both Codex and Claude Code converged on exactly this pair of mechanisms (R§1.4, R§2.1, ledger #11). Instruction files are requests; the sandbox is the guarantee. |
| D8 | **Edits are diff-shaped.** `edit` = exact `str_replace` with a whitespace-insensitive fallback and a uniqueness check; `apply_patch` = V4A grammar (Add/Update/Delete, `@@` context, progressive matching). `write` is for new files; rewriting an existing file over 200 lines is denied with a hint. | 5–20× fewer output tokens than whole-file writes (R§4.2); OpenAI models are trained on V4A and Claude on `str_replace`, so supporting both removes a class of edit failures. |
| D9 | **One SQLite file plus human-readable rollouts, through a sync ORM.** `~/.cox/cox.db` (Diesel 2.2 `sqlite` + bundled `libsqlite3-sys` 0.30 with FTS5, WAL): sessions, usage ledger, tool-output archive index, memory. Typed Diesel models and `schema.rs`; migrations embedded with `diesel_migrations`; FTS5 virtual tables via `diesel::sql_query` (Diesel cannot model `VIRTUAL TABLE`). `cox-store` is the only crate that contains SQL. Each session is also `~/.cox/sessions/<id>.jsonl` — the event stream itself — used for resume, replay tests and export. Archived payloads over 16 KiB live under `~/.cox/archive/`. | Same choice as rtok D13 (user request): typed models make the ledger queries (`stats`, budget, cache diagnostics) joins instead of hand-written SQL, and Diesel is sync, so hooks, tests and `cox stats` need no async runtime. Async ORMs (SeaORM, SQLx) would need a runtime per hook. Codex stores rollouts as JSONL; Claude Code uses JSONL; engram/claude-mem converge on SQLite+FTS5 (R§1.5). |
| D10 | **TUI = ratatui 0.30 + crossterm 0.29 in TEA form, inline viewport.** `State`, `update(State, Msg) -> State`, `view(&State, Frame)`. Inline (non-alternate-screen) rendering so native scrollback keeps the transcript. Every widget has an `insta` snapshot through `TestBackend`; end-to-end through `portable-pty` + `vt100`. | Codex made the same choices and tests them the same way (R§1.6). TEA makes `update` a pure function that a test can drive without a terminal. |
| D11 | **Four surfaces from day one: `cox` (TUI), `cox run -p` (headless; `text`/`json`/`stream-json`), `cox acp` (Agent Client Protocol 2.0 for Zed/JetBrains/neovim), `cox mcp` (built-in tools as an MCP server).** Each is ≤ 300 LOC over the event stream. | D2 makes them cheap; ACP is what gets a terminal agent into editors without an extension per IDE (R§3.2); `cox mcp` lets Claude Code or Codex borrow cox's tools. |
| D12 | **No test touches the network or needs an API key.** `Provider` has `Scripted` (fixtures) and `Replay` (recorded cassettes, re-recorded on demand with `cox record`) implementations; tools run in `tempfile` trees; the patch parser and `str_replace` have `proptest` suites; transcripts and TUI frames are `insta` snapshots; the real binary is driven by `assert_cmd` against `COX_HOME`. Evals (Terminal-Bench adapter) are a separate, opt-in `just eval`. | A coding agent is a distributed system with a nondeterministic component; the only cheap regression suite is one that replays events instead of models (R§5). |
| D13 | **One config file; every flag is a key.** `~/.cox/config.toml` < `<git root>/.cox/config.toml` < `COX_<SECTION>_<KEY>` < flags, via clap 4 (derive) + figment + toml_edit. `cox config show --sources` reports provenance. `.claude/settings.json` permissions and hooks are *imported* (read-only) when present. `.env` / `.env.local` (dotenvy, T0.7) are not a config layer: they inject unset process env before figment reads `COX_*`, and never override variables already set (CI, `COX_HOME=...` tests). | Same rule as rtok D12/D14; it worked. Headless and ACP runs are launched with fixed command lines, so flags alone cannot configure them. Local API keys live in `.env`, which gitignores. |
| D14 | **Everything not written by cox is untrusted, and extensions fail open.** Model output, tool results, MCP responses, hook stdout, skill files and repository instruction files pass the guards in `AGENTS.md` → Trust boundaries. A broken hook, server or skill is warned about and skipped. | Aider's credential leak and Claude Code's escape-sequence incidents are both "trusted text from the wrong side" bugs (R§2.2). |
| D15 | **Each component is designed against the field before it is built.** Every P-phase's first task is a ≤ 1-page `docs/design/<component>.md`: the problem in one measurable number, what Claude Code / Codex / Pi / OpenCode / aider do, what cox does and why it is at least as good, and what would falsify it. Written by the `code` tier; reviewed, not written, by `think`. | rtok D15. Copying a competitor caps cox at that competitor. |
| D16 | **Observability is `tracing` with an optional OpenTelemetry GenAI exporter.** Spans carry `gen_ai.operation.name`, `gen_ai.provider.name`, `gen_ai.request.model`, `gen_ai.usage.*`. Off by default; `cox stats` reads the ledger locally. | Codex ships opentelemetry 0.31 (R§1.3); the GenAI semconv is still experimental, so it stays behind a feature flag. |

Deferred to **v0.2+** (not rejected): LSP client (diagnostics into context); Gemini provider; image input and `ratatui-image`; web search provider abstraction beyond Anthropic server tools; A2A; voice; `gix` instead of shelling out to `git`; aider-style repo map with PageRank; two-model architect/editor mode.

## 1. Architecture

```
            ┌────────────────────────────── cox (one binary) ──────────────────────────────┐
 terminal ──┤ cox            cox-tui  ─┐                                                    │
 script  ───┤ cox run -p     stream-json┤  Submission ▶ ┌──────────┐ ▶ Event                │
 Zed/IDE ───┤ cox acp        cox-acp  ─┤───────────────▶│ cox-core │───────────────▶ rollout │
 other agent┤ cox mcp        (server) ─┘                └────┬─────┘  (.jsonl, ledger)       │
            │                                  traits in cox-protocol │                       │
            │        ┌──────────────┬──────────────┬──────────┴──────┬──────────────┐        │
            │   cox-provider    cox-tools       cox-mcp          cox-store      cox-ext        │
            │   Anthropic       read/edit/      rmcp client      SQLite +       AGENTS.md      │
            │   OpenAI Resp.    apply_patch     stdio/HTTP       archive        skills/cmds    │
            │   OpenAI Chat     bash+sandbox    OAuth            FTS5           hooks/agents   │
            │   Scripted/Replay grep/glob/outline                               settings.json  │
            └──────────────────────────────────────────────────────────────────────────────┘
```

### 1.1 Crates

| Crate | Owns | Key deps (pinned in T0.1; versions verified in R§4.5) |
|-------|------|------|
| `cox` | clap surface, dispatch, `doctor`, `config` (printing, and the flag layer built from `Cli`), `stats`, `expand`, `record`, `sessions`, `self update` | clap 4.6, anyhow, dotenvy 0.15 |
| `cox-config` | the one config owner (T32.16; split out of `cox`): figment layering (default/user/project/env/flag), validation, `cox config set` editing and the `docs/config.jsonschema` drift test. Errors are a `thiserror` enum | figment, toml_edit 0.25, thiserror |
| `cox-protocol` | `Submission`, `Event`, `Item`, `ToolCall`, `ToolResult`, `Usage`, `Config`, traits `Provider`, `Tool`, `Store`, `Hook` | serde, serde_json, schemars 1, thiserror 2 |
| `cox-core` | `Session` state machine, turn loop, context assembly, cache breakpoints, `Router` (job → tier → model), compaction, budget, subagent spawning | tokio 1, tracing 0.1 |
| `cox-models` | the model catalog: id → context window, max output, efforts, capabilities, price; built-in rows < config < user `prices.toml` (T30.24). Pure: parses embedded or caller-supplied strings only | serde, thiserror, figment |
| `cox-provider` | the provider registry and `from_env`; `Scripted` and `Replay` (the `Provider` glue over `cox-provider-testkit`); usage extraction; re-exports the wires at the old `anthropic` and `openai` paths | reqwest 0.12 (rustls) |
| `cox-provider-anthropic` | the Anthropic Messages wire (T32.13; split out of `cox-provider`): request building, stream parsing, wire types from the vendored spec, `schema/` | reqwest 0.12, typify 0.8 (build.rs, T30.10/T30.12) |
| `cox-provider-openai` | the OpenAI Responses and Chat wires (T32.14; split out of `cox-provider`) | reqwest 0.12, async-openai 0.42 (`response-types` only, T30.11) |
| `cox-tools` | `read`, `grep`, `glob`, `edit`, `apply_patch`, `write`, `bash`, `todo`, `ask_user`, `agent`, `tool_search`, `web_fetch`, `expand` | similar 3.2, nix |
| `cox-sandbox` | `path::confine`, `sandbox::{seatbelt,bwrap,landlock}` (T32.3; split out of `cox-tools`): path confinement to the workspace roots and the platform sandbox front door. `cox-tools` re-exports both as `path` and `sandbox` | landlock 0.4.7, seccompiler 0.5, nix |
| `cox-patch` | the V4A patch engine (T32.6; split out of `cox-tools`): `parse` text ↔ AST, `stage` progressive hunk matching. Pure: no filesystem, no `ToolCx`; the `apply_patch` `Tool` impl stays in `cox-tools` (`v4a::tool`) so `path::confine` keeps one call site. `cox-tools` re-exports it as `v4a` | proptest 1.11 (dev) |
| `cox-syntax` | tree-sitter and its grammars (T32.4; split out of `cox-tools`): `outline` (signature extraction for `read`'s outline mode) and `parse_bash` (the parser behind `bash`'s risk classifier). `cox-tools` re-exports `outline` at its old path | tree-sitter 0.27 + bash/rust/typescript/python/go grammars |
| `cox-tokens` | token counting (T32.10; split out of `cox-provider`): `estimate`, `count_openai` (tiktoken), `count_anthropic` (the count-tokens endpoint). `cox-provider` re-exports it at the old `tokens` path | tiktoken-rs 0.12, reqwest 0.12 |
| `cox-permission` | the permission `Engine` (T32.8; split out of `cox-core`): `Outcome`, the rule grammar, path rules. Pure; `cox-core` re-exports it at the old `permission` path | globset (path rules, T2.2) |
| `cox-search` | the grep and glob engines (T32.5; split out of `cox-tools`): `grep::search`, `glob::find`, `rank_by_query`, `workspace_files`. Pure; the `GrepTool`/`GlobTool` impls stay in `cox-tools` so `path::confine` keeps one call site | ignore 0.4.33, grep-searcher 0.1.17, grep-regex 0.1.14, globset, nucleo 0.5 |
| `cox-web` | the `web_fetch` engine (T32.7; split out of `cox-tools`): client, streaming GET with cancellation and a byte cap, HTML → text. `WebFetchTool` stays in `cox-tools` | reqwest 0.12 |
| `cox-telemetry` | tracing setup and the OpenTelemetry stack behind the `otel` feature (T32.9; split out of `cox`); `init` takes plain values, not `Config` | tracing-subscriber, tracing-appender 0.2, opentelemetry 0.32 (+ sdk, otlp, tracing bridge, appender), thiserror |
| `cox-provider-http` | HTTP plumbing shared by every wire (T32.12; split out of `cox-provider`): `http` (client, `resolve_key`, `resolve_key_with`, error mapping), `retry`, `sse`. `cox-provider` re-exports all three at their old paths | reqwest 0.12, keyring 4, eventsource-stream 0.2.3 |
| `cox-provider-testkit` | the pure scenario and cassette helpers behind `Scripted`/`Replay` (T32.11; split out of `cox-provider`): scenario parsing, event building, cassette hashing, secret redaction, cassette writing | figment, sha2 |
| `cox-mcp` | MCP client (stdio, Streamable HTTP, OAuth), server discovery (`.mcp.json`, config), tool namespacing `mcp__<server>__<tool>`, `cox mcp` server | rmcp 3.2 (`client`, `server`, `auth`, `transport-io`, `transport-child-process`, `transport-streamable-http-client-reqwest`), async-trait (server tools as `Tool` impls, T7.6), keyring 4 (OAuth tokens as `cox/mcp/<server>`, T22.5), reqwest 0.13 (the version rmcp implements its HTTP client trait for; the workspace row stays 0.12 for the providers) |
| `cox-store` | `~/.cox/cox.db` Diesel models, `schema.rs`, embedded migrations, rollout writer/reader, archive, FTS5 search (`sql_query`), ledger queries | diesel 2.2 (`sqlite`, `returning_clauses_for_sqlite_3_35`, `r2d2` off), diesel_migrations 2.2, libsqlite3-sys 0.30 (`bundled`), directories 6, keyring 4 |
| `cox-ext` | instruction-file hierarchy, `SKILL.md`, commands, subagent definitions, hook runner (Claude JSON protocol), `.claude/settings.json` import | serde_yaml (frontmatter), shlex, tokio + nix `signal` (hook runner: `sh -c` with a process-group kill on timeout, T7.4), regex 1 (hook `matcher` regexes, T22.3) |
| `cox-sanitize` | `sanitize`, `sanitize_with`, `truncate` (T5.6; split out of `cox-tui` by T32.1): strips escape sequences, C0 controls, bidi overrides and zero-width runs from untrusted text before it reaches the terminal; width-aware truncation. `cox-tui` re-exports it as `text` | unicode-width 0.2 |
| `cox-tui` | TEA app, composer (tui-textarea-2 0.13, the ratatui-0.30 fork of tui-textarea 0.7), transcript cells, streaming markdown (pulldown-cmark 0.13 → spans; the plan said 0.10, same Tag/TagEnd API), syntect 5 highlighting, diff view, approval modal, status line, `/` commands, `@` file picker, `text::sanitize`, OSC 11 background detection for `tui.theme = "auto"` (T22.6), theme files and `/theme` (T24.2) | ratatui 0.30.2 (`scrolling-regions`, T23.2), crossterm 0.29, nucleo 0.5, pulldown-cmark 0.13, syntect 5.3 (fancy-regex, no onig), two-face 0.3 (`syntect-fancy`; ~250 syntaxes, +0.33 MiB — T24.3), unicode-width 0.2, arboard 3, terminal-colorsaurus 1.0, toml_edit 0.25, similar 3.2 (word diffs, the approval modal's proposed edit — T24.5) |
| `cox-acp` | Agent Client Protocol 2.0 server: session/prompt, permission requests, client fs/terminal | agent-client-protocol 2.0 |
| `cox-plugin-api` | plugin manifest (`plugin.toml`), ABI v1 payloads, TUI widget tree, capability names; schemas `docs/plugin.schema.json` and `docs/plugin-abi.schema.json` with drift tests. Pure; builds for `wasm32-unknown-unknown` so the guest SDK can use it; `cox-protocol` re-exports it as `plugin` (A52, P33) | serde, serde_json, schemars 1, thiserror |
| `cox-plugin` | the WASM host: discovery, package digest, grant check, one worker per plugin, host functions (`cox:host/v1`), and the protocol-trait adapters `PluginHooks`, `WasmTool`, `PluginProvider`, `EventTap`, `Advisor` (A52, P33) | extism 1.30.0 (`default-features = false`: no ureq, no URL or file loading), wasmtime 43 (declared only for the `anyhow` feature extism needs without its defaults), sha2 (package digest), figment (`plugin.toml`); linked into `crates/cox` behind the default-on `plugins` feature (A55) |

Dev-deps (workspace): insta 1.48, proptest 1.11, wiremock 0.6, rstest 0.26, assert_cmd 2, predicates 3, assert_fs, tempfile 3, pretty_assertions, vt100 0.16, portable-pty 0.9, libfuzzer-sys 0.4 (fuzz crate only); tools: cargo-nextest, cargo-deny, cargo-audit, cargo-insta, cargo-dist, cargo-fuzz (nightly job only).

Dependency direction (enforced by a test in T0.1 that parses `cargo metadata`): `cox` → everything; `cox-tui`, `cox-acp` → `cox-core`, `cox-protocol`, `cox-sanitize`; `cox-sanitize` → no workspace crate; `cox-core` → `cox-protocol`, `cox-permission` (and may use `cox-models`); `cox-sandbox`, `cox-config`, `cox-models`, `cox-permission`, `cox-tokens`, `cox-patch`, `cox-web`, `cox-provider-http`, `cox-provider-testkit`, `cox-mcp`, `cox-store`, `cox-ext` → `cox-protocol` only; `cox-provider-anthropic`, `cox-provider-openai` → `cox-protocol`, `cox-models`, `cox-provider-http`; `cox-protocol` → `cox-plugin-api` only (the `plugin` re-export, T33.1); `cox-syntax`, `cox-search`, `cox-telemetry` → no workspace crate; `cox-tools` → `cox-protocol`, `cox-sandbox`, `cox-patch`, `cox-syntax`, `cox-search`, `cox-web`; `cox-provider` → `cox-protocol`, `cox-models`, `cox-tokens`, `cox-provider-http`, `cox-provider-testkit`, `cox-provider-anthropic`, `cox-provider-openai`; `cox-plugin-api` → no workspace crate; `cox-plugin` → `cox-protocol`, `cox-plugin-api`, `cox-sanitize`; only `cox-plugin` depends on extism (A52). No crate below `cox` depends on `cox-core`, and `cox-core` does not depend on `cox-plugin`.

### 1.2 The contract every crate shares (`cox-protocol`)

All types derive `Serialize, Deserialize, Debug, Clone, PartialEq`; enums are `#[serde(tag = "type", rename_all = "snake_case")]` so the rollout is greppable. Ids are newtypes over `String` (ULID): `SessionId`, `TurnId`, `ItemId`, `CallId`, `ArchiveId`.

```rust
pub enum Submission {
    UserTurn { text: String, attachments: Vec<Attachment>, confirm_think: bool },
    Approve { call_id: CallId, decision: Decision },          // Decision: Allow | AllowForSession | Deny { reason } | Edit { input }
    Interrupt,                                               // cancels the running turn; tools get the cancel token
    Compact { focus: Option<String> },
    SwitchModel { tier: Tier, model: Option<ModelId> },       // None = tier default
    SetPermissionMode(PermissionMode),                        // default | plan | auto | bypass
    Command(SlashCommand),                                    // parsed by the surface, executed by the core
    HookResult { hook_id: String, outcome: HookOutcome },     // hook runner is outside the core
    Background { call_id: CallId },                           // Ctrl+B: detach a running bash/agent call into a task (T27.1)
    UserShell { command: String, share: bool },               // composer `!`/`!!`: bash via the engine and sandbox; history only on share (T25.3)
    Redo,                                                     // /redo: rewind code to the last rewind's own pre-images, one step (T26.4)
    Shutdown,
}

pub enum Event {
    SessionStarted { session: SessionId, config_digest: String, cwd: PathBuf },
    TurnStarted   { turn: TurnId, job: Job, tier: Tier, model: ModelId },
    ItemStarted   { item: ItemId, kind: ItemKind },          // ItemKind: UserMessage | AssistantMessage | Thinking | ToolCall | ToolResult | Summary | Notice
    TextDelta     { item: ItemId, text: String },
    ThinkingDelta { item: ItemId, text: String },
    ToolCallRequested { call: ToolCall },                    // ToolCall { id, name, input: Value, risk: Risk, subject: String }
    ApprovalRequired  { call: ToolCall, why: Why },          // Why: RuleAsk { rule } | Risk(Risk) | SandboxDenied { detail } | Policy(ApprovalPolicy)
    ApprovalDecided   { call_id: CallId, decision: Decision, by: DecidedBy }, // User | Rule | Session | Policy | Hook
    ToolCallOutput    { call_id: CallId, delta: String },     // streaming stdout/stderr, already sanitised for display
    ToolCallDone      { call_id: CallId, result: ToolResult },// ToolResult { ok: bool, visible: String, archive: Option<ArchiveRef>, bytes: u64, duration_ms: u64, diff: Option<Diff> }
    ItemDone      { item: ItemId },
    Usage         { turn: TurnId, usage: Usage },
    Compacted     { summary: ItemId, dropped: Vec<ItemId>, before_tokens: u32, after_tokens: u32, reason: CompactReason }, // CompactReason: PreCall | PostTurn | Manual | ContextTooLong ("pre-call" …); absent in old rollouts = post-turn
    TaskCreated   { task: TaskId, label: String, tier: Tier },
    TaskCompleted { task: TaskId, result_item: ItemId, cost_usd: f64, exit_code: Option<i32>, archive: Option<ArchiveId> }, // exit code + archive: shell tasks (T27.1)
    ModelSwitched { tier: Tier, from: ModelId, to: ModelId },
    Notice        { level: Level, text: String },            // Level: Info | Warn | Budget | Security
    TurnDone      { turn: TurnId, stop: StopReason },        // EndTurn | MaxTurns | Interrupted | Budget | Refusal { detail } | Error
    Error         { error: CoreError, fatal: bool },
}

pub struct Usage {
    pub input_tokens: u32, pub output_tokens: u32,
    pub cache_read_tokens: u32, pub cache_write_tokens: u32,
    pub estimated: bool,                                     // true when the provider gave no usage and cox estimated
    pub cost_usd: f64, pub latency_ms: u64,
}

pub struct Request {                                         // provider-neutral; providers translate, nothing above knows a wire format
    pub tier: Tier, pub job: Job, pub model: ModelId,
    pub system: Vec<SystemBlock>,                            // SystemBlock { text, cache: bool }
    pub tools: Vec<ToolSpec>,                                // already filtered: deferred tools absent unless discovered
    pub messages: Vec<Message>,                              // Message { role: User | Assistant, content: Vec<Content> }
    pub effort: Effort, pub max_tokens: u32, pub thinking: Thinking, // Thinking: Off | Adaptive
    pub cache_breakpoints: Vec<usize>,                       // indices into system+messages, ≤ 3
    pub stop_sequences: Vec<String>,
}
pub enum Content { Text(String), Thinking { text, signature: Option<String> }, ToolUse { id, name, input }, ToolResult { call_id, content: String, is_error: bool }, Image { media_type, data_b64 }, Pointer { archive: ArchiveRef, summary: String } }

pub enum ProviderEvent { MessageStart { model }, TextDelta(String), ThinkingDelta(String), ToolUseStart { id, name }, ToolUseInputDelta(String), ToolUseEnd, Stop(StopReason), Usage(Usage), Retrying { attempt, after_ms }, Error(ProviderError) }

pub struct ToolSpec { pub name: String, pub description: String, pub input_schema: Value, pub deferred: bool, pub risk: Risk, pub concurrency: Concurrency } // Risk: ReadOnly | Write | Exec | Destructive; Concurrency: Parallel | Exclusive

#[async_trait] pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn capabilities(&self) -> Caps;                          // Caps { cache: bool, thinking: bool, server_tools: bool, count_tokens: bool, max_context: u32 }
    async fn stream(&self, req: Request, sink: mpsc::Sender<ProviderEvent>, cancel: CancellationToken) -> Result<Usage, ProviderError>;
    async fn count_tokens(&self, req: &Request) -> Result<u32, ProviderError>;
}
#[async_trait] pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    fn subject(&self, input: &Value) -> String;              // what permission rules match on: path, command line, url, mcp name
    async fn call(&self, input: Value, cx: &ToolCx) -> Result<ToolOutput, ToolError>;
}
pub struct ToolCx { pub roots: Vec<PathBuf>, pub cwd: PathBuf, pub sandbox: SandboxPolicy, pub archive: Arc<dyn Archive>, pub cancel: CancellationToken, pub output: mpsc::Sender<String>, pub session: SessionId, pub call: CallId }
pub struct ToolOutput { pub text: String, pub is_error: bool, pub diff: Option<Diff>, pub structured: Option<Value> } // text is untruncated; the core archives and truncates

pub trait Store: Send + Sync {                               // sync on purpose (D9)
    fn open(home: &Path) -> Result<Self, StoreError> where Self: Sized;
    fn session_create(&self, s: &SessionRow) -> Result<(), StoreError>;
    fn rollout_append(&self, id: &SessionId, ev: &Event) -> Result<u64, StoreError>;
    fn rollout_read(&self, id: &SessionId) -> Result<Vec<Event>, StoreError>;
    fn usage_insert(&self, row: &UsageRow) -> Result<(), StoreError>;
    fn archive_put(&self, a: &ArchivePut) -> Result<ArchiveId, StoreError>;
    fn archive_get(&self, id: &ArchiveId) -> Result<Vec<u8>, StoreError>;
    fn memory_search(&self, q: &str, limit: usize) -> Result<Vec<MemoryHit>, StoreError>;
}
#[async_trait] pub trait Hook: Send + Sync { async fn run(&self, event: HookEvent, payload: Value, timeout: Duration) -> HookOutcome; } // Continue | Block { reason } | Modify { input } | Failed { error }
```

### 1.3 The turn loop (`cox-core`)

States of `Session`: `Idle → Assembling → Streaming → (AwaitingApproval ⇄ RunningTools) → Streaming … → Finishing → Idle`, plus `Compacting` (entered from `Idle` after `TurnDone`) and `Interrupted` (from any state; drains tools, emits `TurnDone{Interrupted}`).

```
on Submission::UserTurn(text):
  1. hooks: UserPromptSubmit (may block or rewrite text)
  2. history.push(UserMessage(text)); turn = new TurnId
  3. loop:
     a. req = assemble(history, config)                 # §1.9 order; exactly one movable breakpoint
     b. (tier, model, effort) = router.pick(job=Main)   # think requires confirm_think == true
     c. budget.check(estimate(req)) else TurnDone{Budget}
     d. provider.stream(req) → forward deltas as Events; collect tool_use blocks; usage row
     e. if stop == EndTurn: break
        if stop == MaxTokens: push assistant partial, continue once, else break with Notice
        if stop == Refusal: TurnDone{Refusal}; break
        if stop == ToolUse:
           calls = collected tool_use blocks (1..n)
           for each call (in parallel up to core.parallel_tools, Exclusive tools serialised):
             i.   hooks: PreToolUse (Block → result is_error with reason; Modify → new input)
             ii.  decision = engine.decide(call)         # §1.8; Ask → emit ApprovalRequired, await Submission::Approve
             iii. if Deny: result = error("denied: <why>")
             iv.  else run tool under sandbox policy; stream ToolCallOutput; on SandboxDenied and policy==on-failure → ApprovalRequired{SandboxDenied} → rerun unsandboxed only if approved
             v.   archive full output BEFORE truncation; visible = truncate(head/tail, pointer trailer)
             vi.  dedup: if hash(name,input) seen within dedup_window and no write to its subject since → visible = "unchanged since #<id>"
             vii. hooks: PostToolUse / PostToolUseFailure
           history.push(UserMessage(all tool results, in call order))   # one message; parallel tool use breaks otherwise
           continue loop
  4. hooks: Stop; TurnDone{EndTurn}
  5. if context_tokens ≥ compact_at × max_context: enter Compacting (§1.10) on cheap tier, then Idle
```

Rules the loop enforces, testable one by one: (1) all tool results for one assistant message go back in one user message, in the order the calls were emitted; (2) an `ApprovalRequired` blocks only that call — other approved parallel calls proceed; (3) `Interrupt` cancels the provider stream and every running tool via the shared token, then emits the partial assistant item and `TurnDone{Interrupted}`; (4) no `Event` is emitted after `TurnDone` for that turn; (5) the archive row exists before the model sees truncated text; (6) the request built after resume from the rollout is byte-identical to the one a live session would have built.

### 1.4 Routing table (D5)

| Job | Tier | Default model | Effort | Note |
|-----|------|---------------|--------|------|
| main coding turn | `code` | `claude-sonnet-5` | `high` | `/model opus` switches for the session; never auto |
| large refactor flagged by user | `code` | `claude-opus-5` | `xhigh` | user picks |
| `/think`, `--deep` plan | `think` | `claude-fable-5-1` | `high` | confirm prompt shows price ($10/$50 per MTok) |
| compaction summary | `cheap` | `claude-haiku-4-5` | — | output ≤ 2 k tokens |
| tool-result summary, title, commit message, memory extraction | `cheap` | `claude-haiku-4-5` | — | batched where possible |
| explore / search subagent | `cheap` | `claude-haiku-4-5` | — | read-only tools; result ≤ 1 k tokens |
| background shell / HTTP subagent | `cheap` | `claude-haiku-4-5` or local | — | `bash`, `web_fetch` only |
| local-only mode | all | Ollama/vLLM model | — | `cox --provider local` |

Prices for the ledger (Anthropic first-party, from the Claude API reference; re-verify in T1.7): Haiku 4.5 $1/$5, Sonnet 5 $2/$10, Opus 5 $5/$25, Fable 5.1 $10/$50 per MTok; cache write 1.25×, cache read 0.1× of input (Fable 5.1 cache read $0.25/MTok). `config/prices.toml` carries `verified_on` per row; a row older than 90 days makes `cox doctor` warn.

### 1.5 Testing pyramid (D12)

| Level | What | How | Where |
|-------|------|-----|-------|
| unit | parsers (SSE, V4A, frontmatter, permission rules), `str_replace`, truncation, cache-order assembly | plain tests + `proptest` | each crate |
| contract | `Provider` against recorded HTTP | `wiremock` serving `fixtures/<provider>/*.sse` | `cox-provider` |
| loop | full turns with `Scripted` provider; golden `Event` JSONL | `insta` on the event stream | `cox-core/tests` |
| tool | every tool in a tempdir; sandbox denial paths | `tempfile`, `assert_fs` | `cox-tools/tests` |
| TUI | each widget and whole frames | `TestBackend` + `insta` | `cox-tui` |
| binary | `cox run -p` and `cox` under a PTY | `assert_cmd`, `portable-pty` + `vt100` | `tests/` |
| eval (opt-in) | Terminal-Bench adapter, 10 in-repo tasks | `just eval`, real provider, ledger diff | `evals/` |

Fixture conventions: `fixtures/<provider>/<name>.sse` is the raw SSE body; `<name>.request.json` the request cox sent; `<name>.events.jsonl` the golden `ProviderEvent`s. Loop fixtures: `cox-core/tests/scenarios/<name>.toml` (scripted replies per turn) + `<name>.events.snap` (insta). Secrets are redacted at record time (`cox record --redact` replaces `sk-…` and `Bearer …` with `«redacted»`); a test in T1.5 greps fixtures for key patterns.

### 1.6 Configuration schema (`config/default.toml`, embedded; every key documented in `docs/config.md` by test)

```toml
[core]
home = "~/.cox"                 # COX_HOME overrides
workspace_roots = []            # empty = git root of cwd, else cwd; extra roots via --add-dir
max_turns = 200                 # per UserTurn, counts provider calls
parallel_tools = 4
log_level = "info"              # tracing filter; file log at ~/.cox/logs/cox.log

[tiers.cheap]
provider = "anthropic"
model = "claude-haiku-4-5"
effort = "low"
max_tokens = 4096

[tiers.code]
provider = "anthropic"
model = "claude-sonnet-5"
effort = "high"
max_tokens = 16384
thinking = "adaptive"

[tiers.think]
provider = "anthropic"
model = "claude-fable-5-1"
effort = "high"
max_tokens = 32768
thinking = "adaptive"
confirm = true                  # cannot be set false in project config

[jobs]                          # job → tier; values must name a tier
main = "code"
plan = "think"
compact = "cheap"
title = "cheap"
summarize = "cheap"
commit = "cheap"
memory = "cheap"
explore = "cheap"
shell = "cheap"
hook = "cheap"

[providers.anthropic]
base_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"   # else keyring entry "cox/anthropic"
cache_ttl = "5m"                    # "5m" | "1h"
fallbacks = true                    # fallbacks: "default" + beta header
timeout_s = 120
max_retries = 4

[providers.openai]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
api = "responses"                   # "responses" | "chat"

[providers.local]
base_url = "http://localhost:11434/v1"
api = "chat"
model = "qwen3-coder"
context_window = 32768              # local servers do not report it

[context]
compact_at = 0.75                   # fraction of max_context
keep_turns = 2
microcompact_after_turns = 6
tool_output_visible_bytes = 8192
tool_output_head_lines = 60
tool_output_tail_lines = 20
dedup_window_turns = 8
instruction_budget_tokens = 8000
memory_budget_tokens = 800
deferred_tools = true

[permissions]
mode = "default"                    # default | plan | auto | bypass (bypass only via flag)
approval = "on-request"             # untrusted | on-request | on-failure | never
allow = []                          # rule strings, §1.8
ask = []
deny = ["Read(~/.ssh/**)", "Read(~/.aws/**)", "Bash(rm -rf /*)"]
import_claude_settings = true
allow_for_session_persists = false

[sandbox]
mode = "workspace-write"            # read-only | workspace-write | danger-full-access
network = false
writable = []                       # extra writable roots
readonly_in_workspace = [".git", ".cox", ".claude"]
linux_backend = "auto"              # auto | bwrap | landlock | none

[budget]
session_usd = 5.0
monthly_usd = 100.0
warn_at = 0.8
cheap_counts = true

[tui]
vim = false
theme = "auto"                      # auto | dark | light
inline = true
show_thinking = "collapsed"         # collapsed | hidden | full
mouse = true

[hooks]
timeout_s = 60
fail_open = true

[mcp]
timeout_s = 30
deferred = true
servers = {}                        # [mcp.servers.<name>] command/args/url/env — same shape as .mcp.json

[memory]
enabled = true
extract = false                     # end-of-session extraction on cheap tier
dir = ""                            # default ~/.cox/projects/<slug>/memory

[telemetry]
otel = false
endpoint = ""

[record]
redact = true
```

Precedence (D13): embedded defaults < `~/.cox/config.toml` < `<git root>/.cox/config.toml` < `.claude/settings.json` (permissions/hooks/env only, imported) < `COX_<SECTION>_<KEY>` (e.g. `COX_TIERS_CODE_MODEL`) < CLI flags. Before figment runs, `dotenvy` loads `.env` then `.env.local` walking up from cwd (T0.7); missing files are ignored; already-set variables are left alone, so a key that arrived only via `.env` still shows as `env` in `cox config show --sources`. Project config may not raise `budget.*`, set `permissions.mode = "bypass"`, set `sandbox.mode = "danger-full-access"` or set `tiers.think.confirm = false`; violations are reported by `cox config show` and ignored. `cox config show --sources` prints every effective key with its origin; `cox config set <key> <value>` edits the user file with `toml_edit` preserving comments.

### 1.7 Storage schema (`cox-store`)

Directory layout under `COX_HOME`:

```
~/.cox/
  config.toml
  cox.db                     # SQLite, WAL, FTS5
  sessions/<ulid>.jsonl      # rollout: one Event per line
  archive/<ulid>             # archived tool outputs > 16 KiB (smaller ones inline in the db)
  logs/cox.log               # tracing-appender, daily rotation
  projects/<slug>/memory/    # MEMORY.md + one file per fact (Claude Code layout)
  cassettes/<name>/          # cox record output
```

```sql
CREATE TABLE migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
CREATE TABLE sessions (
  id TEXT PRIMARY KEY, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
  cwd TEXT NOT NULL, project_slug TEXT NOT NULL, title TEXT, parent_id TEXT,
  rollout_path TEXT NOT NULL, turns INTEGER NOT NULL DEFAULT 0,
  cost_usd REAL NOT NULL DEFAULT 0, state TEXT NOT NULL CHECK (state IN ('open','closed','error'))
);
CREATE TABLE usage (
  id INTEGER PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id), turn INTEGER NOT NULL,
  job TEXT NOT NULL, tier TEXT NOT NULL, provider TEXT NOT NULL, model TEXT NOT NULL,
  input_tokens INTEGER NOT NULL, output_tokens INTEGER NOT NULL,
  cache_read_tokens INTEGER NOT NULL, cache_write_tokens INTEGER NOT NULL,
  estimated INTEGER NOT NULL DEFAULT 0, cost_usd REAL NOT NULL, latency_ms INTEGER NOT NULL,
  context_tokens INTEGER NOT NULL,            -- what the model saw this call (for context-token-turns)
  created_at TEXT NOT NULL
);
CREATE INDEX usage_session ON usage(session_id, turn);
CREATE INDEX usage_day ON usage(created_at);
CREATE TABLE archive (
  id TEXT PRIMARY KEY, session_id TEXT NOT NULL, call_id TEXT NOT NULL, tool TEXT NOT NULL,
  subject TEXT, bytes INTEGER NOT NULL, sha256 TEXT NOT NULL,
  inline BLOB, path TEXT, created_at TEXT NOT NULL,
  CHECK ((inline IS NULL) <> (path IS NULL))
);
CREATE TABLE memory (
  id INTEGER PRIMARY KEY, project_slug TEXT NOT NULL, name TEXT NOT NULL, path TEXT NOT NULL,
  kind TEXT NOT NULL, updated_at TEXT NOT NULL, UNIQUE(project_slug, name)
);
CREATE VIRTUAL TABLE memory_fts USING fts5(name, body, project_slug UNINDEXED);
CREATE VIRTUAL TABLE rollout_fts USING fts5(session_id UNINDEXED, turn UNINDEXED, text);
```

ORM rules (D9): the DDL above is `migrations/<stamp>_init/up.sql` + `down.sql`, embedded with `diesel_migrations::embed_migrations!` and applied on `Store::open`; `schema.rs` is generated by `diesel print-schema` and committed (a test asserts it matches the migrations); each table has a `Queryable`/`Insertable` model in `cox-store/src/models.rs`; FTS5 tables are created and queried with `diesel::sql_query` and `QueryableByName` structs; one `SqliteConnection` behind a `Mutex` (no pool — a single-process CLI), `PRAGMA`s set on open. No other crate may depend on `diesel`; the direction test in T0.1 also asserts that.

Rollout line format: `{"seq":17,"ts":"2026-09-02T10:11:12.345Z","event":{"type":"text_delta","item":"…","text":"…"}}`. `seq` is monotonic per session; the reader tolerates a truncated last line (crash during write). `TextDelta`/`ThinkingDelta`/`ToolCallOutput` are coalesced on read into their items; resume rebuilds `history` from `ItemStarted`/`ItemDone` pairs and `Compacted`.

### 1.8 Permission rules and the decision algorithm (`cox_core::permission::Engine`)

Rule grammar (Claude Code's, verbatim): `Tool`, `Tool(subject)`, `Tool(prefix:*)`; file tools take a glob (`Read(~/.ssh/**)`, `Edit(src/**)`), `Bash` takes a command prefix (`Bash(npm run test:*)`, `Bash(git commit:*)`), MCP tools match `mcp__<server>__<tool>` or `mcp__<server>__*`, `WebFetch(domain:example.com)`. Tool names are matched case-insensitively against cox's names and their Claude aliases (`Read`=`read`, `Edit`=`edit`, `Write`=`write`, `Bash`=`bash`, `Grep`=`grep`, `Glob`=`glob`, `WebFetch`=`web_fetch`, `Agent`=`agent`).

Decision order for a `ToolCall` with `risk` and `subject`:

1. `deny` rules (user, project, imported): first match → `Deny`.
2. `PermissionMode::Bypass` → `Allow` (flag-only mode; banner shown).
3. `PermissionMode::Plan`: `Risk::ReadOnly` → `Allow`; everything else → `Deny("plan mode")` — no prompt, so the model learns to plan.
4. `allow` rules: first match → `Allow`.
5. `ask` rules: first match → `Ask(RuleAsk)`.
6. Session grants (`AllowForSession` with the same tool + subject prefix) → `Allow`.
7. By risk: `ReadOnly` → `Allow`; `Write` → `Allow` in `auto`, else `Ask(Risk)`; `Exec` → `Ask(Risk)` unless the command is classified safe (T3.7 classifier: read-only commands like `ls`, `cat`, `git status`, `cargo test`, no redirects, no `sudo`) and mode is `auto`; `Destructive` → `Ask` in every mode except `Bypass`.
8. Approval policy adjusts step 7: `untrusted` → anything not from an `allow` rule asks; `on-request` → as above; `on-failure` → `Exec` runs sandboxed without asking and asks only when the sandbox denies; `never` → any `Ask` becomes `Deny` (headless default unless `--approve on-request`).
9. A `Deny` or `Ask` carries `Why`; the model sees the reason in the tool result so it can choose another approach.

The engine is pure: `decide(&self, call, mode, policy, grants) -> Decision`. Rules are compiled once (`globset` for paths, tokenised prefix for bash). Table-driven tests (T2.2) cover 30 rule/call pairs; `proptest` checks that adding a `deny` never turns a `Deny` into anything else.

### 1.9 Context assembly and cache layout

```
 ┌ system[0]  tool specs, non-deferred, sorted by name, canonical JSON        ┐ byte-stable for the session
 │ system[1]  cox system prompt (versioned string, no date, no cwd)           │  cache breakpoint 1 (after system[2])
 │ system[2]  instruction files: AGENTS.md/CLAUDE.md chain, skills index      ┘
 │ system[3]  volatile: date, cwd, git branch, memory index, permission mode      no cache (changes daily / per turn)
 │ messages   [Summary item if compacted]
 │            history … (older tool results microcompacted to pointers)         cache breakpoint 2 = end of previous turn
 │            this turn's user message + tool results                           cache breakpoint 3 = last message (moves every call)
 └
```

Invariants: bytes of `system[0..=2]` are identical across all calls of a session unless the user changes instruction files or tools are discovered via `tool_search` (discovered tools are appended to `system[0]`, which invalidates breakpoint 1 once; `Notice` explains it). Anthropic allows 4 breakpoints; cox uses 3 so a fourth is free for experiments. OpenAI providers ignore breakpoints (automatic prefix caching) but still benefit from the stable order. `Request.cache_breakpoints` are indices; the Anthropic translator turns them into `cache_control: {"type": "ephemeral", "ttl": …}`.

Token accounting per call writes `context_tokens` (input + cache read + cache write) to the ledger; `context-token-turns` for a session is the sum. T8.5 measures each D6 mechanism by toggling it and replaying recorded sessions.

### 1.10 Compaction and microcompaction

Trigger: after `TurnDone`, when `context_tokens_last_call ≥ context.compact_at × max_context` (`post-turn`, checked at the next turn's start), or on `/compact [focus]` (`manual`), or when a provider returns a context-length error (`context-too-long`; compaction runs before retrying once), or before any provider call inside a turn whose assembled request estimates at or over that threshold (`pre-call`, T28.3: ⌈bytes/4⌉, refined by `Provider::count_tokens` within 10 % of it; microcompaction of every result outside `keep_turns` first, then compaction and one re-assembly; still over → `Notice(Budget)` + `TurnDone{Budget}`). `Compacted.reason` names which.

Algorithm (append-only, D6f):
1. `PreCompact` hooks run with `{trigger, focus}`; a hook may `Block` (compaction skipped, notice shown).
2. Items to summarise = all items older than the last `keep_turns` turns, excluding items already dropped. Pointers replace archived tool results in the summariser input.
3. Request on the `compact` job (cheap tier): system = "You are compacting a coding session…" + focus; user = the items rendered as a transcript; output ≤ 2 048 tokens with fixed sections: Goal · Decisions · Files touched (paths) · Open todo · Errors seen · Next step.
4. On success: append `Item::Summary`, emit `Compacted{summary, dropped, before, after}`. The rollout keeps every original line; `dropped` ids are skipped when building requests. On failure: `Notice(Warn)`, nothing changes.
5. `PostCompact` hooks; instruction files are re-read (Claude Code behaviour) but only re-emitted if their bytes changed.

Microcompaction (no model call): when building a request, tool results older than `microcompact_after_turns` turns are replaced by `Content::Pointer { archive, summary: "<tool> <subject>: N bytes, exit 0" }`. The rollout is untouched.

### 1.11 Tool catalogue (core eight are non-deferred; the rest are found by `tool_search`)

| Tool | Input schema (required first) | Risk | Output | Notes |
|------|-------------------------------|------|--------|-------|
| `read` | `path`; `lines: "a-b"`; `mode: "text"\|"outline"` | ReadOnly | text with line numbers, or outline | size cap → pointer; binary → refuse with hint; images v0.2 |
| `grep` | `pattern`; `path`; `glob`; `context: n`; `max_results` (100) | ReadOnly | `path:line: text` | ripgrep libs; respects `.gitignore`; cap → pointer |
| `glob` | `pattern`; `path`; `limit` (200) | ReadOnly | paths sorted by mtime, nucleo-ranked when `query` given | |
| `edit` | `path`, `old`, `new`; `replace_all: bool` | Write | unified diff | exact → whitespace-insensitive; ambiguity is an error listing match lines |
| `apply_patch` | `patch` (V4A text) | Write | per-file diff summary | Add/Update/Delete/Move; `Destructive` if it deletes > 5 files |
| `write` | `path`, `content` | Write | bytes written | existing file > 200 lines → error "use edit" |
| `bash` | `command`; `shell` (`sh`\|`bash`\|`zsh`\|`fish`\|`dash`\|`ksh`\|`tcsh`\|`nu`\|`pwsh`, default `sh`); `timeout_s` (120); `background: bool` | Exec / Destructive (classified) | streamed stdout+stderr, exit code | sandboxed; env allowlist; cwd = workspace; the shell enum is the allowlist and resolves in fixed dirs, never `PATH` |
| `todo` | `items: [{id, text, state}]` | ReadOnly | rendered list | state drives the TUI todo panel |
| `expand` | `id` (archive id); `lines: "a-b"` | ReadOnly | archived bytes (capped, further pointers) | deferred: false (always present, tiny schema) |
| `ask_user` | `question`; `options: [..]` | ReadOnly | the answer | blocks the turn; headless → error unless `--answer` |
| `tool_search` | `query` | ReadOnly | up to 5 matching deferred tool specs, appended to `system[0]` | BM25 over name + description |
| `web_fetch` | `url`; `max_bytes` | ReadOnly (network) | readable text | Anthropic server tool passthrough when available; else reqwest + readability; domain rules |
| `agent` | `task`; `preset: "explore"\|"shell"\|<name>`; `tier`; `tools: [..]`; `budget_usd`; `background: bool` | inherits max of its tools | result text ≤ cap, summarised on cheap if over | subagent = nested `Session` with its own rollout, parent id set |
| `memory_save` / `memory_search` | `name, body` / `query` | Write / ReadOnly | id / hits | P10 |
| `mcp__<server>__<tool>` | server's schema | from server annotations, default Write | server result, archived like any tool | deferred by default |

Every tool's `subject()` is what rules match on: the confined path, the command line, the URL, or the namespaced MCP name.

### 1.12 CLI surface (`crates/cox`)

```
cox [PROMPT] [--continue | --resume <id>]           interactive TUI; PROMPT is the first turn
cox run -p <prompt> [--output-format text|json|stream-json] [--max-turns N] [--allowed-tools a,b]
        [--permission-mode default|plan|auto|bypass] [--approve never|on-request] [--answer <text>]
        [--continue | --resume <id>] [--deep]        headless; exit 0 ok · 1 error · 2 denied · 3 budget · 4 interrupted
cox sessions [--grep <q>] [--json] [--limit N]       list / search rollouts
cox expand <archive-id> [--lines a-b]                print archived tool output
cox stats [--session <id> | --day | --month] [--cache] [--json | --csv]
cox config show [--sources] | get <key> | set <key> <value> | path
cox doctor [--json]
cox record <name> -p <prompt> [--redact] [--provider ...] re-record a cassette with a real key
cox mcp [--allow-write] [--tools a,b]                serve built-in tools over MCP stdio
cox acp                                              Agent Client Protocol server on stdio
cox ext list [--json]                                instruction files, skills, commands, agents, hooks, MCP servers in effect
cox self update [--version v]
Global: --provider <name> --model <id> --tier <tier>=<model> --sandbox <mode> --budget <usd> --cwd <dir> --add-dir <dir>
        --home <dir> -v/-vv --json (machine output where supported) --no-hooks --no-mcp
```

Every flag maps to a config key (T0.3 test); `--permission-mode bypass` and `--sandbox danger-full-access` are flag-only and print a persistent banner.

### 1.13 TUI keymap and slash commands (`cox-tui`)

| Key | Action | Key | Action |
|-----|--------|-----|--------|
| `Enter` | send | `Shift+Enter` / `Alt+Enter` / `Ctrl+Enter` | newline (`Ctrl+Enter` reserved for send-now, T25.1) |
| `Esc` | interrupt turn / close modal | `Ctrl+C` ×2 within 1 s | quit |
| `Shift+Tab` | cycle permission mode default → plan → auto; the prompt glyph follows (`>` `▷` `»` `!`); `Tab` completes an `@`/`/` token | `Ctrl+O` | transcript overlay (full scrollback, search `/`) |
| `Ctrl+T` | toggle thinking visibility | `Ctrl+E` | expand last tool output |
| `@` | file picker (nucleo) | `/` at line start | command palette |
| `y` / `s` / `n` / `e` in approval modal | allow / allow for session / deny / edit command | `Ctrl+R` | prompt history search |
| `PageUp/PageDown`, mouse wheel | scroll transcript | `Ctrl+L` | redraw |
| `Ctrl+G` | diff view: the working tree against `HEAD`, per-file blocks; `PageUp/PageDown` scroll, `Esc` closes (T15.3) | `Ctrl+B` | move the running `bash`/`agent` call to the background; the turn goes on (T27.1) |

Slash commands (parsed in the surface, executed as `Submission::Command`): `/model [tier] [model]`, `/effort [low|high|xhigh]` (session-wide, clamped per model, T16.4), `/think <prompt>` (confirm dialog with price), `/compact [focus]`, `/cost`, `/permissions`, `/sandbox <mode>`, `/resume`, `/sessions`, `/expand <id>`, `/agents` (live sessions in this workspace, T16.3), `/skills`, `/hooks`, `/mcp`, `/doctor`, `/clear` (new session, same cwd), `/vim`, `/help`, `/quit`. Markdown files in `.claude/commands` and `.cox/commands` appear in the same palette (T7.3).

Status line (one row): `sonnet-5 · ctx 41% · $0.83 · workspace-write · 2 tasks · [plan]`.

### 1.14 Error taxonomy

| Crate | Enum | Variants |
|-------|------|----------|
| `cox-provider` | `ProviderError` | `Auth`, `RateLimited { retry_after }`, `Overloaded`, `BadRequest { message }`, `ContextTooLong { max, got }`, `Refusal { detail }`, `Network`, `Timeout`, `Cancelled`, `Parse { line }`, `Unsupported { feature }` |
| `cox-tools` | `ToolError` | `Denied { why }`, `Confined { path, root }`, `SandboxDenied { detail }`, `Timeout`, `NotFound`, `Ambiguous { matches }`, `TooLarge { bytes, cap }`, `Binary`, `Io`, `Cancelled` |
| `cox-core` | `CoreError` | `Budget { spent, cap }`, `Interrupted`, `Provider(ProviderError)`, `Tool { call, error }`, `Compaction`, `Config { key, message }`, `Store(StoreError)`, `Hook { id, error }` |
| `cox-store` | `StoreError` | `Open`, `Migrate { from, to }`, `Corrupt { path }`, `NotFound`, `Io`, `Sqlite` |
| `cox-ext` | `ExtError` | `Frontmatter { path, line }`, `HookTimeout`, `HookCrashed { status }`, `TooLarge { path, budget }`, `Cycle { path }` |
| `cox-mcp` | `McpError` | `Spawn`, `Handshake`, `Auth`, `Timeout`, `Transport`, `ToolFailed { server, tool }` |

Retryable: `RateLimited`, `Overloaded`, `Network`, `Timeout` (provider) — exponential backoff 1 s × 2ⁿ, jitter, max 4, honouring `retry-after`. Fatal to the turn, not the session: everything else. Fatal to the session: `StoreError::Corrupt`, `Config`.

### 1.15 Cross-cutting invariants (each is a named test somewhere in §3)

1. `prefix_bytes_identical_between_turns` (T2.3) · 2. `truncate_is_lossless_via_archive` (T2.5) · 3. `all_tool_results_return_in_one_message` (T2.1) · 4. `deny_beats_allow` (T2.2) · 5. `compaction_keeps_last_two_turns_verbatim` (T8.1) · 6. `resume_builds_identical_request` (T2.4) · 7. `no_event_after_turn_done` (T2.1) · 8. `every_request_has_a_usage_row` (T1.7) · 9. `think_requires_confirmation` (T9.1) · 10. `broken_hook_is_skipped_not_fatal` (T7.4) · 11. `sandbox_denies_write_outside_workspace` (T4.1/T4.2) · 12. `every_flag_has_a_config_key` (T0.3) · 13. `no_crate_below_cox_depends_on_core` (T0.1) · 14. `sanitize_strips_escapes` (T5.6) · 15. `plugin_tool_specs_frozen_within_session` (T33.12) · 16. `plugin_grant_reasked_on_digest_or_widening` (T33.6) · 17. `plugin_failure_is_skipped_not_fatal` (T33.3).

## 2. Working agreement for agents

See `AGENTS.md`. In short: claim `open` tasks only; ≤ 200 LOC and ≤ 3 files per task (tests count; fixtures and snapshots do not); the Check is not optional; `Model:` records who did it. Task model guidance: **haiku** for scaffolding, fixtures, snapshot updates, docs, shell/CI; **sonnet** for most code; **opus** for the state machine, permission engine, sandbox, compaction, V4A, MCP client, ACP; **fable** never writes code — it reviews `docs/design/*.md` when a phase gate asks for it.

Task block format used in §3:

```
#### T<phase>.<n> <title>
Model: <tier> · Status: open · Depends: <task ids> · Size: ~<LOC>
Goal: one sentence, measurable.
Files: the files this task creates or edits (≤ 3 source files).
Steps: numbered; each step is something a reviewer can see in the diff.
Check: a bash block that exits 0 when the task is done; run under `mise exec`.
Done when: the observable state after the Check, plus what must be in done.md.
Out of scope: what the next task does, so the agent does not do it here.
```

Work only on `main` — no `cox/<task-id>` or other task branches. Commit `<task-id>: <title>` on `main`; any new dependency needs a row in §1.1 and a reason in the commit. If the Check cannot pass without exceeding the size limit, split the task with an amendment in §6 and do the first half. Skipped or failing steps are reported in `done.md`, never silently. The human is the only author: no agent adds a `Co-Authored-By` trailer, a "Generated with …" line or itself as author to a commit, merge or PR, whatever its harness defaults to.

Don't duplicate code or logic — find the existing helper and reuse it, or extract one shared helper at the responsible layer. Never a per-caller guard-patch. A new snippet is checked with `jscpd` (`check_duplication`) before committing.

Every implemented task is marked `Status: done <date>` and moved to `done.md` with its Check output. Code in the tree whose task still sits in `plan.md` as `open` or `in progress` is unfinished work.

## 3. Phases and tasks

### 3.0 Dependency graph and critical path

```
P0 ─▶ P1 ─▶ P2 ─▶ P3 ─▶ P4 ─▶ P5(rest) ─▶ P6 ─▶ P7 ─▶ P8 ─▶ P9 ─▶ P10 ─▶ P11 ─▶ P12
        │            └──▶ T5.1–T5.3 (TUI slice, after T2.4)
        └──▶ T8.3, T8.4 (ledger tooling) can start after T1.7
```

Critical path to M1 ("talks"): T0.1 → T0.2 → T0.3 → T0.4 → T1.1 → T1.2 → T1.5 → T2.1 → T2.3 → T2.4 → T5.1 → T5.2 → T5.3. Everything else in P0–P2 can run in parallel with it (T0.5, T0.6, T0.7, T1.3, T1.4, T1.6–T1.8, T2.2, T2.5–T2.8).

P22–P30 (A27): P22 first; T22.1–T22.6 may proceed independently, then T22.7 follows and depends on all six. Then P24.1 (colour tokens) → P24.2 (themes) and P23.0 (`Caps`) → P23.1 (Kitty keys) → P25.1/P25.2 (queue, `Shift+Tab`). P26.1 → P26.2 → P26.3/P26.4 is the checkpoint chain. P23, P27 and P28 are independent of each other and of P26; P29 and P30 come last. Complexity per task is in the top table (1 easiest … 5 hardest).

### P0 — Scaffold (goal: `cox --version`, config, doctor, CI green)

### P1 — Provider (goal: one real streamed turn with tool use through each wire format, all replayable)

### P2 — Core loop (goal: a session that runs turns, calls tools, asks permission, resumes)

### P3 — Tools (goal: the eight core tools, diff-shaped edits, everything confined)

### P4 — Sandbox (goal: `workspace-write` enforced on macOS and Linux)

### P5 — TUI (goal: a daily-driver terminal UI with snapshots for every state)

### P6 — Headless and MCP server (goal: scripts and other agents can drive cox)

### P7 — Extensions (goal: a Claude Code or Codex user's setup works unchanged)

### P8 — Context economy (goal: measured savings, cache hit rate visible)

### P9 — Routing and subagents (goal: D5 enforced end to end)

### P10 — Memory (goal: cross-session memory with zero model cost by default)

### P11 — ACP and IDE (goal: cox inside Zed and JetBrains)

### P12 — Quality and release (goal: v0.1 installable and measured)

### P13 — Observability follow-up (goal: the exporter honours the standard resource variables and the documented smoke commands run as written)

Rationale in §6 A10. T13.4 and T13.5 are in `done.md`.

### P14 — TUI presentation (goal: the TUI renders correctly on any terminal font, glyph set, colour depth and language)

Rationale in §6 A11. What already exists and is *not* redone here: syntect highlighting of fenced code blocks (T5.3), `unicode-width` wrapping/truncation of wide and combining text (T5.3, T5.6), `tui.theme = auto|dark|light` resolved in `crates/cox/src/session.rs`.

### P15 — Git in the surfaces (goal: the branch, what changed and the diff are visible without leaving cox)

Rationale in §6 A13. Not redone here: unified-diff rendering (`cox-tui/src/diff.rs`, T5.4), the nucleo picker (T5.2), running git (the `bash` tool, A12).

### P16 — Concurrent sessions on one workspace (goal: every cox process on a workspace knows what the others are doing, and the TUI shows it)

Rationale in §6 A14. Not redone here: the `/` palette with nucleo ranking (T5.2 — `/sessions`, `/agents`, `/model` already complete), hooks (T7.4), `cox sessions` (T10.3/A13), subagent tasks (T9.2).


### P17 — Surface leftovers (goal: §1.12 resume/continue/first prompt, OpenAI retry, doctor prices, website matches the shipped binary)

Rationale in §6 A20. T17.1–T17.7 are in `done.md`.


### P18 — TUI resume and the remaining website pages (goal: `cox --resume` opens the TUI; the site covers tools, compat, IDE and the walkthrough)

Rationale in §6 A21. T18.1–T18.6 are in `done.md`.

### P19 — v0.2 scoping gates (goal: each roadmap v0.2 line gets a phase-gate design doc; no runtime code yet)

Rationale in §6 A23. Branch `plan/v0.2-scoping`, one commit per task, one draft PR into `main` (PR #24).
Out of scope for the whole phase: any change under `crates/` — only `docs/design/v0.2-*.md`,
`plan.md`, and `roadmap.md` move here.

### P20 — ketch-model release (goal: releases publish first, tag last, installable via ketch and Homebrew)

Rationale in §6 A24. Branch `release/ketch-model`, one commit per task, PR #26 into `main`; P19 (PR #24) stays untouched.

### P21 — TypeSafe Jev as a decision model (goal: typed choice/score/boolean calls with probabilities where cox routes, classifies and gates — design doc first, no runtime code yet)

Rationale in §6 A25. Jev is a decision model (System One), not a chat or coding agent: state + typed questions in, choice/score/boolean with probabilities and confidence out. It fits cox at the exact points where cox already reduces a turn to a narrow judgment — router tier pick, permission risk, compaction triggers, memory salience, skill/command matching. No new dependency lands until the design doc fixes the boundary: Jev answers never bypass the permission engine, never touch the filesystem, and lose to the local default whenever the key, the network or the confidence is missing (fail open on extensions, same as hooks/skills/MCP).
Out of scope for the whole phase: any change under `crates/` — only `docs/design/v0.2-jev.md`, `plan.md`, and `roadmap.md` move here, mirroring the P19 scoping-gate shape. T21.1–T21.2 (below) are the implementation the gate allowed: provider wiring only, no call sites yet.

### P22 — Trust (goal: every config key, hook event and documented command does what the docs say; evidence in research.md §8.5 #32)

### P23 — Terminal capabilities (goal: one probe, every feature optional, `doctor` shows the verdict)

### P24 — Looks (goal: a reviewer calls it beautiful; every state has a snapshot and an SVG)

Done when: the two snapshots show highlighting and the release binary grows by less than 1 MiB (number in the commit message).
Out of scope: language auto-detection beyond file extension and first-line shebang.

### P25 — Composer and flow (goal: the keys a Claude Code or Codex user already has in their fingers)

### P26 — Checkpoints and rewind (goal: `/rewind` that also covers what the shell changed)

### P27 — Agents you can see (goal: no "raw scaffolding noise")

### P28 — Context and cost visibility (goal: the ledger and the routing are visible, not just recorded)

### P29 — Accessibility (goal: usable with a screen reader and without motion)

### P30 — Lean profile and footprint (goal: numbers cox can publish that no vendor does)

#### T30.13 cox vs Claude Code vs Terminus 2 on the same local model

Depends: T30.14 (done) · Size: ~120
Goal: a like-for-like Terminal-Bench 2.0 baseline, taken before the optimization and refactoring pass and repeated after it (roadmap): the same 12 tasks, the same local model, one attempt each, three agents — cox (`cox_evals.tbench:CoxAgent`), Harbor's built-in `claude-code` and `terminus-2`. The difference in pass rate, tokens and wall time is then the agent's, not the model's. The 3/3 in R§5.3 says nothing about this: those were 3 of the dataset's 4 `easy` tasks (55 are `medium`, 30 `hard`).
Model: `prism-ml/bonsai-27b` (Qwen 3.5 architecture, 2-bit MLX, 8.5 GB), already in LM Studio, chosen by the creator. No API spend.
Tasks (fixed; `random.Random(3013).sample` over the `difficulty` field of each `task.toml` at terminal-bench-2 `69671fb`): medium `build-cython-ext`, `build-pmars`, `compile-compcert`, `mteb-leaderboard`, `query-optimize`, `regex-log`, `sanitize-git-repo`, `tune-mjcf`; hard `dna-assembly`, `password-recovery`, `path-tracing-reverse`, `regex-chess`.
Plan:
1. Serve the model: LM Studio's server on the host with the context raised well above its default (agents' prompts do not fit 4k); read the loaded context back from `GET /api/v1/models` (`lms load --context-length` was not honoured in T30.14's check, R§4.3.2) and record it with the LM Studio version. One cheap call per API shape: OpenAI Chat `/v1/chat/completions` (cox, Terminus 2 via LiteLLM) and Anthropic Messages `/v1/messages` (Claude Code via `ANTHROPIC_BASE_URL`). If LM Studio has no Messages endpoint, check its docs for one first, then stop and ask before adding a proxy.
2. Reach it from the containers: cox and Claude Code run inside the task container, so they need the host address colima exposes to its VM (`host.lima.internal`); Terminus 2 runs on the host and uses `localhost`. Verify with `curl` from a throwaway container.
3. Teach `CoxAgent` the local provider: a `base_url` kwarg that writes `[providers.local]` into the container's fresh `COX_HOME/config.toml` and runs `--provider local`, no key required for it; tests in `evals/tests/test_tbench.py`.
4. Disk: at least 15 GB free before building 12 images; `docker image prune` between agents if needed; colima with a capped disk as in T30.9. Rebuild the Linux cox from current `main` and record the commit.
5. Run the three agents one after another, same `-i` list, `-n 1` (one model server serializes requests anyway), `--force-build`, jobs dir under `~/.cache/cox-evals/tb-jobs`. The task timeouts are the only cap for all three; cox gets no `--budget` limit that the others lack.
6. Record in R§5.3 a per-task table (pass, tokens, wall time for each agent), the totals, the cox commit, Harbor, LM Studio and model versions, the dataset commit and the reproduce commands, with sources; stop colima and unload the model.
Check: R§5.3 has the table; `just test-evals` green.
Done when: the three agents' results on the 12 tasks with `bonsai-27b` are in R§5.3.
Out of scope: leaderboard submission (5 attempts × 89 tasks); paid models; the repeat run after the refactoring (roadmap).
Postponed by the creator (lowest priority). A first run started on 2026-09-25 and was stopped mid-way. Its partial job dirs are under `~/.cache/cox-evals/tb-jobs/2026-09-25__23-4*`; they are not a result. The provider work (T30.21–T30.26) lands before this run, so the baseline will not be taken before that refactoring. The first run prompted for the macOS login password to read the key from the keychain; that is this card's problem, solved when it is picked up (read the key once per run, not per task).

### P32 — Crate split (goal: every crate exists for a reason in `docs/design/crates.md`; D1 as amended by A47)

Every card in this phase:

1. `git mv`s the named files into `crates/<crate>/`. The new `lib.rs` opens with a `//!` header, and `Cargo.toml` takes only the dependencies those files use.
2. Leaves a `pub use` at the old path, so callers and the guard names keep working.
3. Adds the crate's rule to `crates/cox/tests/deps.rs`.
4. Updates the AGENTS.md layout table (and the trust list for a guard) and the plan.md §1 crate list.

No logic changes. At most three crates are touched. Moved lines do not count toward the 200-LOC limit; edited lines do.
Common check: the three commands in AGENTS.md are green, `deps.rs` has the crate's rule, and the card's own line holds.

#### T32.2 `cox-render`: themes, colour, markdown, diff, SVG and glyphs

Depends: T32.1 · Moves: `theme.rs`, `color.rs`, `svg.rs`, `markdown.rs`, `diff.rs`, `glyph.rs` from `cox-tui` (~2.6k).
Why: dependencies (a), namely syntect, two-face, pulldown-cmark and terminal-colorsaurus.
Plan:
1. Before the move, record `cargo build --timings` for two builds: a clean `cox-tui`, and an incremental build after touching `state.rs`.
2. Move the modules.
3. Record the same timings again, both in R§4.3.4.
4. Apply the falsifier in `docs/design/crates.md`.

Check: syntect, two-face and pulldown-cmark appear only in `cox-render/Cargo.toml`; the TUI snapshots are unchanged.

### P33 — WASM plugins (goal: one package adds a status segment, a hook, a deferred tool and a provider without a cox release; §1.15 invariants 1, 8, 10 and 15–17 green; ≤ 50 ms warm start per plugin)

Rationale in §6 A52; the design is `docs/design/plugins.md` (cited below as PL§n).

Every card in this phase:

- stays within 200 LOC and 3 source files (manifests, generated schemas, snapshots and fixtures do not count);
- leaves a test that fails without it;
- documents what it adds (`docs/design/plugins.md` if the design moves, `docs/config.md` through the drift test for new keys, `docs/plugins.md` user docs from T33.27 on);
- runs the three standard commands.

Host unit tests use inline WAT (R§4.3.5 P15); no `.wasm` is ever committed. **Blockers** (everything after them depends on them): T33.1, T33.2, T33.3, T33.5, T33.6.

#### T33.6 Grant check and granted-only loading — blocker

Depends: T33.4, T33.5 · Size: ~170 · Files: `crates/cox-plugin/src/grant.rs`, `crates/cox/src/session.rs`
Goal: the pure `grant::check(manifest, digest, stored) -> Verdict { Granted | NeedsApproval { added, removed } | Disabled }`. Session open loads only `Granted` plugins. Headless and ACP print one `Notice(Warn)` per ungranted plugin naming the command to run. `plugins.enabled` is a config key with an env var and `--no-plugins` (D13).
Check: `narrower_request_needs_no_reapproval`, `new_digest_needs_approval`, `widened_capability_is_reported_in_added`, `headless_never_loads_ungranted_plugin` (e2e, scratch `COX_HOME`); `every_flag_has_a_config_key` still green.

#### T33.7 `cox plugin install | enable | disable`

Depends: T33.6 · Size: ~180 · Files: `crates/cox/src/plugin_cmd.rs`, `crates/cox/src/cli.rs`
Goal:
- `install <dir>` copies into `versions/<digest12>/` and writes `current` atomically; the only v1 source is a local path, recorded in the grant's `source`.
- `enable [--project] [--yes]` prints the capabilities in words and asks on stdin.
- `disable` clears `enabled`.
Check: e2e in a scratch `COX_HOME`: install → enable `--yes` → `list` shows `loaded`; `disable` → `not loaded`; `project_plugin_needs_project_grant`.

#### T33.8 TUI grant dialog

Depends: T33.6 · Size: ~150 · Files: `crates/cox-tui/src/state.rs`, `crates/cox-tui/src/modal.rs`, `crates/cox/src/session.rs`
Goal: `Modal::PluginGrant` asks for each `NeedsApproval` plugin at session open, queued in the single modal slot. `y` grants that digest and `n` skips it for this session. Project plugins show their repository in warning style. Every manifest string is sanitized.
Check: insta snapshots (a new plugin, a widened plugin with the diff, a project plugin); `grant_dialog_sanitizes_description`.

#### T33.9 Base host functions and the context snapshot

Depends: T33.6 · Size: ~190 · Files: `crates/cox-plugin/src/hostfn.rs`, `src/context.rs`
Goal: `cox_log`, `cox_notify` (level capped at `Warn`), `cox_kv_*` (through `PluginStore`), `InitIn.config` from `[plugins.<id>]` (the `PluginsConfig` flatten, following `HooksConfig`), and `cox_context` folded from events. Each host function checks the grant and the calling context (PL§4).
Check: `notify_cannot_raise_security_level`, `kv_denied_without_capability`, `context_snapshot_is_redacted` (a secret in a tool result does not reach the snapshot); the config docs drift test is green with the new section.

#### T33.10 Events: the tap, the rings, `cox_on_event`

Depends: T33.9 · Size: ~180 · Files: `crates/cox-protocol/src/traits.rs`, `crates/cox-core/src/session.rs`, `crates/cox-plugin/src/events.rs`
Goal: `EventTap` in `cox-protocol`, and `Session::set_event_tap`, called in `emit` after the rollout append with the scrubbed event. A per-plugin ring of 256 with drop-oldest and a counter, delivered in batches with `first_seq`/`dropped`. `Effects.redraw` is forwarded.
Check:
- `tap_never_blocks_emit`: a plugin that sleeps in `cox_on_event` does not slow a scripted 50-turn session beyond noise;
- `ring_drops_oldest_and_counts`;
- `plugin_sees_only_subscribed_kinds`;
- invariant 7 `no_event_after_turn_done` still green.

#### T33.11 Hooks: plugins as a hook source

Depends: T33.9 · Size: ~190 · Files: `crates/cox-ext/src/hooks.rs`, `crates/cox-core/src/hooks.rs`, `crates/cox-plugin/src/hooks.rs`
Goal:
- `Hook::interested(event)` with the config check as the default, so `fire_configured` asks the hook instead of `[hooks]`.
- The `ShellHooks` chaining loop becomes one shared function used by a new `HookChain`.
- `PluginHooks` implements `Hook` through `cox_hook`.
- Order: shell hooks first, then plugins by id; the deadline is the smaller of `hooks.timeout_s` and `limits.call_ms`.
Check: `plugin_hook_fires_without_hooks_config`, `shell_block_wins_over_plugin`, `plugin_modify_chains_into_next_hook`, `plugin_hook_timeout_fails_open_with_notice`; invariant 10 `broken_hook_is_skipped_not_fatal` still green.

#### T33.12 Tools from plugins

Depends: T33.9 · Size: ~170 · Files: `crates/cox-plugin/src/tool.rs`, `crates/cox/src/session.rs`
Goal: `WasmTool` implements `Tool` as `wasm__<id>__<tool>`. It is always deferred, its specs are frozen at `cox_init`, and tools are sorted by (id, tool) and appended after MCP. `Concurrency::Exclusive` per plugin. `cox_output` and `cox_cancelled` inside `cox_tool_call`.
Check: `plugin_tool_specs_frozen_within_session` (new invariant 15); `prefix_bytes_identical_between_turns` with a plugin tool discovered mid-session; `plugin_tool_output_is_archived_before_truncation`.

#### T33.13 `cox_invoke_tool` through the engine

Depends: T33.12 · Size: ~150 · Files: `crates/cox-plugin/src/hostfn.rs`, `crates/cox-core/src/turn.rs` (a plugin-origin entry point that reuses the `PreToolUse` → engine → sandbox → archive path)
Goal: a plugin calls only the tools listed in `invoke`, and each call passes `PreToolUse`, `Engine::decide`, the sandbox and the archive. `Ask` shows "plugin <id> asks to run …". Headless mode denies it. Hook, decide, provider and render contexts get `NotInThisContext`.
Check: `plugin_invoke_denied_by_rule`, `plugin_invoke_outside_grant_is_refused`, `invoke_from_hook_context_is_refused`.

#### T33.14 `cox_http` and filesystem preopens

Depends: T33.9, T33.43 (preopens stay off until wasmtime ≥ 48, A55) · Size: ~180 · Files: `crates/cox-plugin/src/net.rs`, `src/fs.rs`
Goal:
- `cox_http` over reqwest: the host must match the allow-list, the body is capped, and a `net` entry equal to a configured provider host is refused at validation (PL§7d).
- WASI preopens come only from `fs` and pass `confine`; reads mount `ro:`; `.git` and `.cox` are never writable. WASI is on only when `wasi = true` or `fs` is set.
Check: wiremock `http_outside_allow_list_is_refused`, `net_entry_matching_provider_host_is_rejected`, `fs_write_to_dot_git_is_refused`, `wasi_ctx_has_no_env`.

#### T33.15 `cox_model_call` and `Job::Plugin`

Depends: T33.9 · Size: ~170 · Files: `crates/cox-protocol/src/types.rs` (`Job::Plugin`), `crates/cox-core/src/router.rs`, `crates/cox-plugin/src/hostfn.rs`
Goal: a plugin's model call goes through the router at or below its granted tier (never `think`), passes the budget gate and writes one `usage` row with job `plugin:<id>`.
Check: `plugin_model_call_writes_usage_row` (Scripted provider), `plugin_model_call_blocked_by_budget`, `plugin_cannot_reach_think_tier`; invariant 8 green.

#### T33.16 Models: the plugin catalog layer

Depends: T33.6, T30.24 · Size: ~150 · Files: `crates/cox-models/src/catalog.rs`, `crates/cox/src/session.rs`
Goal: `Catalog::load` takes plugin rows. The layer order is built-in < plugin (fill-only for existing ids) < config < user prices. A price conflict is ignored with a notice. Two plugins defining the same new id: the lower id wins. `ModelRow.source` records where each row came from.
Check: `plugin_cannot_override_builtin_price`, `plugin_fills_missing_context_window`, `config_overrides_plugin_row`, `duplicate_plugin_model_lower_id_wins`.

#### T33.17 Providers, declarative form

Depends: T33.16 · Size: ~130 · Files: `crates/cox/src/session.rs`, `crates/cox-plugin/src/provider.rs`
Goal: a `[[provider]]` with `api = "chat" | "responses"` merges into `providers.custom` as a `CompatibleProviderConfig`. A user config section of the same name wins. The key comes through `resolve_key(api_key_env, name)`.
Check: `plugin_chat_section_builds_openai_shaped_client` (wiremock), `config_section_shadows_plugin_section`, `plugin_provider_request_has_usage_row`.

#### T33.18 Providers, ABI form (`PluginProvider`)

Depends: T33.14, T33.17 · Size: ~190 · Files: `crates/cox-plugin/src/provider.rs`, `src/net.rs`, `crates/cox-core/src/router.rs`, `crates/cox-protocol/src/types.rs`, `crates/cox/src/session.rs`
Goal: with `api = "plugin"`, `stream()` calls `cox_provider_stream` and forwards `ProviderEvent`s. The guest's `cox_http` is limited to `base_url`'s host, and the host injects the `auth` header from `resolve_key`, so the key never enters wasm memory. Missing usage is estimated; usage below half of cox's estimate is replaced by the estimate with one warning.
Plan (amended 2026-09-26 for the Jev use case, R§4.3.6 J§4.3): `Router::pick` and `backend_for_with` register ABI provider sections by name, so a tier — including a legacy `typesafe` tier — resolves to them, not only to `providers.custom`. The ledger gets a `ProviderId::Plugin` bucket whose provider string is the section name, the same shape `Local` uses for compatible providers; `provider_name` returns it. `COX_PROVIDER=scripted`/`replay`, which short-circuits provider construction for the main turn, still builds plugin providers, so a scripted-main e2e can reach a real (wiremocked) plugin provider.
Check: `provider_key_never_reaches_guest` (the WAT guest echoes its request headers, and the test asserts the key is absent); `underreported_usage_is_replaced_by_estimate`; `every_request_has_a_usage_row` with a plugin provider; `plugin_provider_section_resolves_by_name`; `scripted_provider_mode_still_builds_plugin_providers`.

#### T33.19 MCP servers from plugins

Depends: T33.6, T32.3 · Size: ~160 · Files: `crates/cox-mcp/src/discovery.rs`, `crates/cox/src/session.rs`
Goal: `[[mcp]]` entries become the lowest-precedence discovery source (`plugin:<id>`), named `<id>-<name>`. `crates/cox` wraps a stdio command with the sandbox policy before `cox-mcp` spawns it. An in-package command is covered by the digest; an HTTP `url` must be in `net`.
Check: `project_mcp_json_shadows_plugin_server`, `plugin_stdio_server_runs_under_sandbox` (it writes outside the workspace and is denied; macOS and Linux paths as in T4.1/T4.2), `changing_bundled_server_binary_changes_digest`.

#### T33.20 Decision points: the `Advisor` trait and `route`

Depends: T33.15 · Size: ~190 · Files: `crates/cox-protocol/src/traits.rs` (+ `Event::Advised` in `types.rs`), `crates/cox-core/src/router.rs`, `crates/cox-plugin/src/advisor.rs`
Goal: `Advisor` set on `Session` like the hook, with `[plugins.decide]` naming one plugin per point plus `min_confidence` and a latency budget. `route` offers only tiers at or below the static pick and never `think`. Every answer is an `Event::Advised { applied }`. On silence, lateness or low confidence the static pick is used.
Plan note (amended 2026-09-26, R§4.3.6 J20): a turn a decision plugin routes down must strip thinking blocks in its own `Request` only — never rewrite `inner.history` in place, and never emit `ModelSwitched` for a same-turn tier offer. That stripping, and the cache-aware filter that decides whether `cheap` is even offered, are implemented by T33.40.8; this card only wires the `Advisor` trait and the `route` offer through it.
Check: `route_advice_never_routes_up`, `late_advice_falls_back_to_static_pick`, `advised_event_in_rollout_replays_identically`; `docs/protocol.jsonschema` regenerated.

#### T33.21 Decision points: `risk`, `approve_hint`, `compact`, `rank`, `salience`

Depends: T33.20 · Size: ~190 · Files: `crates/cox-core/src/turn.rs`, `crates/cox-core/src/compact.rs`, `crates/cox-tools/src/tool_search.rs`
Goal: the monotone rules from PL§4:
- risk only raises, and (amended 2026-09-26, R§4.3.6 J§5.1) the core asks only when raising the call to `Destructive` would change the engine's outcome from `Allow` to `Ask` or `Deny` — this filter belongs here so every `risk`-capable plugin gets it, not only Jev;
- `approve_hint` is warning-only (amended 2026-09-26, `docs/design/plugins.md` §14 decision 10): a plugin may add a caution note, never say a call "looks safe" — monotone the same direction as `risk`, never used to grant quiet approval;
- compaction only happens earlier, never skipped when mandatory;
- rank reorders or filters cox's own candidates.
`salience` is wired only if it fits the size; otherwise it is a follow-up card noted here.
Check: `risk_advice_cannot_lower_risk`, `risk_not_asked_when_outcome_would_not_change`, `approve_hint_cannot_say_looks_safe`, `compact_advice_cannot_skip_mandatory_compaction`, `rank_advice_cannot_add_tools`.

#### T33.23 TUI status segments

Depends: T33.10, T33.22 · Size: ~160 · Files: `crates/cox-tui/src/status.rs`, `crates/cox-tui/src/state.rs`, `crates/cox/src/session.rs`
Goal: `Msg::Plugin(PluginUiMsg)` and `Cmd::Plugin(PluginRequest)`. `status.left`/`status.right` segments are cached in `State`. `cox_render` runs only on redraw requests, resize or visibility, never from `view`. Plugin segments drop first on a narrow terminal. A render has 20 ms, and three misses show "⚠ <id> slow".
Check: insta snapshots (wide and narrow); `view_never_calls_plugin` (a counting fake bus); `slow_render_keeps_last_good_segment`.

#### T33.24 TUI panel and overlay

Depends: T33.23 · Size: ~170 · Files: `crates/cox-tui/src/state.rs`, `crates/cox-tui/src/view.rs`, `crates/cox-tui/src/modal.rs`
Goal: a bottom `panel` (≤ 8 rows, above the composer, toggled by the plugin's command or key) and `Modal::Plugin { id }` as a full-screen overlay that Esc closes. Sizes are sent through `Cmd::Plugin`.
Check: insta snapshots of the panel open and closed and of the overlay; `esc_closes_plugin_overlay`.

#### T33.25 Plugin commands and keys

Depends: T33.23 · Size: ~180 · Files: `crates/cox-tui/src/state.rs`, `crates/cox-tui/src/keymap.rs`, `crates/cox-tui/src/commands.rs`
Goal: `/<id>:<name>` in the palette after the built-ins, and a `CommandOut` limited to PL§4's closed set. Keys work only as `<leader> <key>`; `plugin.leader` can be rebound in `keybindings.toml`. Built-ins and user bindings win. A clash between plugins goes to the lower id and is reported by `Keymap::conflicts()`.
Check: `builtin_command_wins_over_plugin`, `plugin_command_prompt_submits_user_turn`, `plugin_key_only_under_leader`, `plugin_key_conflict_is_reported`.

#### T33.26 Custom rendering of tool results and messages

Depends: T33.23 · Size: ~160 · Files: `crates/cox-tui/src/cells.rs`, `crates/cox-tui/src/state.rs`
Goal: `cox_render_item` for `tool:<name>` and `item:assistant_message`, asked once when the cell completes and cached in the cell. `None`, a timeout or an error uses the built-in rendering. A target outside the plugin's own tools needs its explicit grant.
Check: insta snapshots (plugin-rendered, and fallback after a timeout); `renderer_output_never_reaches_rollout_or_model` (the rollout and the next `Request` are byte-identical with and without the renderer).

#### T33.27 Guest workspace and the Rust SDK

Depends: T33.2 · Size: ~190 · Files: `plugins/Cargo.toml`, `plugins/sdk/src/lib.rs`, `mise.toml` (+ `.github/workflows/ci.yml` `targets:`; `plugins/mise.toml` created empty for later languages)
Goal: `cox-plugin-sdk` over extism-pdk 1.4.1 and `cox-plugin-api`: typed wrappers for every export and host function in PL§4, plus a `register!` macro. Add `rust = { version = "1.97.1", targets = ["wasm32-unknown-unknown"] }`; the target is not a version bump. `docs/plugins.md` gets the author guide.
Check: `cargo build --manifest-path plugins/Cargo.toml -p cox-plugin-sdk --target wasm32-unknown-unknown`; the main-workspace `cargo nextest run --workspace` still never builds guest code; the extism-pdk row is in §1.1 and `toolchain.md`.

#### T33.28 Rust reference example and e2e

Depends: T33.27, T33.11, T33.23, T33.25 · Size: ~190 · Files: `plugins/examples/rust/src/lib.rs`, `crates/cox-plugin-fixtures/build.rs`, `tests/plugins.rs`
Goal: the shared example from PL§13 (a turn-count status segment, a `PostToolUse` failure counter, `/<id>:reset`, a `turn_started` subscription). `cox-plugin-fixtures` (`publish = false`) builds it to `OUT_DIR` for `wasm32-unknown-unknown`. With the target missing, the build fails and names `mise install`.
Check: e2e with the real binary in a scratch `COX_HOME` and the Scripted provider: install → enable `--yes` → a two-turn `run -p` → the rollout shows the hook's effect and `cox plugin list --json` shows the contributions; `just bench` records the PL§11 timings in R§4.7.

#### T33.29 `cox plugin new` and the Rust template

Depends: T33.28 · Size: ~200 · Files: `crates/cox/src/plugin_new.rs`, `crates/cox/src/cli.rs`, `plugins/templates/rust/*.tmpl` (templates do not count)
Goal: `cox plugin new <name> [--lang] [--dir] [--with …]` from PL§13:
- one pure module maps (name, lang, with) to a list of files;
- `plugin.toml` capabilities match `--with`;
- only the chosen stub exports are written, plus a `justfile`, a README and a smoke test;
- the name is validated and an existing directory is never overwritten;
- headless defaults to `rust`.
Check: e2e `cox plugin new demo --lang rust --with status,hook` in a scratch `COX_HOME` asserts the file tree and that the manifest validates against `docs/plugin.schema.json`; it then builds `--offline` with the SDK patched to the in-repo path and runs the smoke test. `new_refuses_existing_dir`, `new_rejects_invalid_name`.

#### T33.30 `/plugin new` in the TUI

Depends: T33.29 · Size: ~120 · Files: `crates/cox-tui/src/commands.rs`, `crates/cox-tui/src/state.rs`, `crates/cox/src/session.rs`
Goal: the palette's `/plugin new <name>` asks for the language with `Modal::Picker` when it is not given, then sends a `Cmd` to the same `plugin_new` function. There is no second implementation.
Check: insta snapshot of the picker; `tui_plugin_new_calls_shared_scaffold` (a fake executor records one call with the chosen language).

#### T33.31 `cox plugin update` and rollback

Depends: T33.7 · Size: ~190 · Files: `crates/cox/src/plugin_cmd.rs`, `crates/cox-plugin/src/install.rs`
Goal: PL§1b: re-read the recorded path source, validate the schema, `api` and digest, print a capability diff, and support `--check`. Staging uses temp and rename; `current` is swapped only after approval; one `previous` is kept. `--rollback` reuses the stored grant for that digest. Headless and ACP never approve a widening.
Check: e2e offline against a local path: install → rebuild with changed bytes → `update --check` shows the diff → `update` requires a re-grant → `--rollback` restores the old digest without asking; `update_in_headless_keeps_current_and_warns`.

#### T33.32 `cox plugin remove`

Depends: T33.31 · Size: ~150 · Files: `crates/cox/src/plugin_cmd.rs`, `crates/cox-plugin/src/install.rs`
Goal: PL§1c: confirm (`--yes` skips), disable, delete the plugin's own directory (resolved and checked to be under `~/.cox/plugins/`, no symlinks followed out), delete every grant row, and delete kv unless `--keep-data`. Report config references and edit none of them. A project plugin keeps its files.
Check: e2e: remove → files, grants, kv and contributions are gone and a sibling plugin is untouched; `--keep-data` keeps kv; `remove_refuses_path_outside_plugins_dir` (a symlinked version dir).

#### T33.33 `/plugin update | remove | list | reload` in the TUI

Depends: T33.30, T33.32 · Size: ~140 · Files: `crates/cox-tui/src/commands.rs`, `crates/cox-tui/src/state.rs`, `crates/cox/src/session.rs`
Goal: the palette entries reach the same `plugin_cmd` functions through `Cmd`. Remove asks through a modal. `/plugin reload` means `/clear` with a notice that the cache prefix restarts. In-session remove stops the instance; its frozen tools answer `Denied { why: "plugin removed" }`.
Check: insta snapshot of the remove confirmation; `removed_plugin_tool_is_denied_until_next_session`.

#### T33.34 Go: SDK wrapper, template, example

Depends: T33.29 · Size: ~200 · Files: `plugins/sdk-go/cox.go`, `plugins/examples/go/main.go`, `plugins/templates/go/*.tmpl`; `plugins/mise.toml` gets go and tinygo; CI job `plugin-examples`
Goal: a thin Go package over `github.com/extism/go-pdk` (v1.1.3) for the PL§4 exports and host functions (`//go:wasmimport` in `cox:host/v1`). The same example is built with TinyGo `-target wasip1 -buildmode=c-shared` (`wasi = true`). `cox plugin new --lang go`.
Check: `plugin_example_go` is `#[ignore = "needs go and tinygo: run just plugin-examples go"]` locally and runs in the `plugin-examples` CI job, where a missing toolchain fails the job; it asserts the same rollout effect as T33.28.

#### T33.35 Kotlin: feasibility spike

Depends: T33.28 · Size: ~80 (spike; a throwaway branch of files under `plugins/spikes/kotlin`, not merged) · Files: `research.md` §4.3.5, `docs/design/plugins.md` §13
Goal: prove or refute that a Kotlin/Wasm `wasmWasi` module (the latest Kotlin, R§4.3.5 P32) with `@WasmImport("extism:host/env", …)` and `@WasmExport` loads in extism 1.30.0 and round-trips `cox_init`, with and without extism's `wasmtime-exceptions` feature (P33–P34).
Falsifier: the module fails to instantiate under the wasmtime 43 extism pins, or needs a feature cox will not enable → Kotlin stays out of `--lang` and PL§13 records why. If it passes only with `wasmtime-exceptions`, ask the creator before enabling it.
Check: R§4.3.5 gains the spike's facts with versions; the PL§13 row says "proven" or "refuted".

#### T33.36 Kotlin: thin PDK, template, example (only if T33.35 passes)

Depends: T33.35 · Size: ~200 · Files: `plugins/sdk-kotlin/src/…/Cox.kt`, `plugins/examples/kotlin/src/…/Main.kt`, `plugins/templates/kotlin/*.tmpl`; `plugins/mise.toml` gets java, gradle and kotlin
Goal: a cox-owned minimal Kotlin PDK over the raw extism imports (there is no maintained one, R§4.3.5 P30), the same example, and `--lang kotlin`.
Check: `plugin_example_kotlin` is ignored with its reason locally and runs in the CI job.

#### T33.37 Dart: WASI re-check spike

Depends: T33.28 · Size: ~60 · Files: `research.md` §4.3.5, `docs/design/plugins.md` §13
Goal: re-check whether the latest Dart can emit a module that runs outside JS (dart-lang/sdk#56366, R§4.3.5 P35) and load it in extism.
Falsifier: `dart compile wasm` output still needs a JS bootstrap → Dart stays an MCP-server-only exception (T33.38) and the spike is repeated when #56366 closes.
Check: R§4.3.5 records the Dart version tried and the result.

#### T33.38 Dart: MCP-server plugin template and example

Depends: T33.19, T33.29, T33.37 · Size: ~170 · Files: `plugins/examples/dart/bin/server.dart`, `plugins/templates/dart/*.tmpl`, `crates/cox/src/plugin_new.rs`; `plugins/mise.toml` gets dart
Goal: `--lang dart` scaffolds a package whose only capability is an `[[mcp]]` stdio server (`dart compile exe`, `dart_mcp` 0.5.2), with no `plugin.wasm`. `--with` accepts only `tool` and `mcp` for Dart and says why for anything else. The example serves one `count` tool.
Check: `plugin_example_dart` is ignored with its reason locally and runs in the CI job (the tool is callable through `mcp__<id>-count__count` and runs under the sandbox); `new_dart_rejects_status_with_reason`.

#### T33.39 `cox doctor` and `cox ext` plugin reporting

Depends: T33.28 · Size: ~150 · Files: `crates/cox/src/doctor.rs`, `crates/cox/src/ext_cmd.rs`
Goal: a doctor plugins row listing, for each plugin:
- loaded, skipped (with reason), not granted, or dev;
- exports disabled by the three-failure breaker;
- catalog price conflicts (T33.16);
- the wasmtime cache directory size.
`cox ext list` shows the same state.
Check: doctor snapshots in a scratch `COX_HOME` with one healthy, one broken and one ungranted plugin; `disabled_export_is_visible_in_doctor`.

#### T33.40 Jev as the first plugin

Rationale: A25/P21, evidence `research.md` §4.3.6 (cited below as J§n/Jn). These seventeen cards test the ABI-form provider, the models, and the decision-point capabilities end to end against a real (wiremocked) use case, prove or refute PL§12 falsifier 3, and carry out `docs/design/plugins.md` §14 decision 8 (C1): the built-in `typesafe` client leaves the core once the plugin reaches parity. T33.18, T33.20 and T33.21 above already carry the amendments this use case needed.

#### T33.40.1 ABI: two-phase decide, own-provider call-out, batched questions — blocker

Depends: T33.2, T33.14, T33.15, T33.18, T33.20 · Size: ~180 · Files: `crates/cox-plugin-api/src/abi.rs`, `crates/cox-plugin/src/advisor.rs`, `crates/cox-plugin/src/net.rs`
Goal: close the gap that PL§12 falsifier 3 predicts (J§4.1–4.3). Today a decision plugin can reach its own provider only through a deadlock (`cox_model_call` into its own `cox_provider_stream`) or a ledger bypass (`cox_http` from `cox_decide`). This card adds a path with neither.
Plan:
1. ABI changes:
   - `DecideOut = Advice(Option<Advice>) | Call(ModelCall)`;
   - the new optional export `cox_decide_resume(DecideResume { question, events }) -> Option<Advice>`;
   - `ModelCall.target = Tier(t) | OwnProvider { name, model }`;
   - `Question.items: Vec<QuestionItem>` (one answer per item).
   
   These are minor additions: `api` stays 1. Regenerate `docs/plugin-abi.schema.json`.
2. `PluginAdvisor`: on `Call`, check that the target is a `[[provider]]` of the same plugin. Then run it through the budget gate → the provider registry (`PluginProvider`) → `Priced`, which writes one `usage` row with `job = plugin:<id>`. Then call `cox_decide_resume` with the events. The point's latency budget covers all three steps. Any failure along the way is `None`, and the local default applies.
3. `net.rs`: `cox_http` to a provider section's host is allowed only inside `cox_provider_stream`. Everywhere else it returns `NotInThisContext`.
4. PL§4: add the exports and state the context rule.
Check:
- `decide_call_out_writes_one_usage_row`: a WAT guest returns `Call`, and its own `cox_provider_stream` returns fixed events;
- `decide_call_out_over_budget_falls_back`;
- `decide_cannot_target_another_plugins_provider`;
- `http_to_provider_host_outside_provider_stream_is_refused`;
- `batched_question_answers_each_item`;
- `abi_schema_matches_committed_file`;
- invariant 8 `every_request_has_a_usage_row` still green.

#### T33.40.2 Jev guest crate: the System One wire

Depends: T33.27 · Size: ~190 · Files: `plugins/jev/src/lib.rs`, `plugins/jev/src/wire.rs`, `plugins/Cargo.toml` (member; `plugins/jev/Cargo.toml` and `plugins/jev/plugin.toml` are manifests)
Goal: the typed wire, pure and tested on the host target:
- `SystemOneRequest { state: Value, model, questions: BTreeMap<String, Question> }`, where `Question` is `Choice { instructions, criteria }`, `Score { instructions, levels }` or `Noul { instructions, criteria? }`;
- `Answer`, and `parse` returning a typed error with no default guessed.

It is ported from `crates/cox-provider/src/jev.rs` with two fixes:
- a Noul's certainty is `|2p − 1|`, not `p`, because the API returns no Noul confidence (J11);
- state plus the longest question is capped at 32k estimated tokens (J6).

The manifest declares `id = "jev"` and `[[provider]] name = "typesafe", api = "plugin"`. `decide`, `context` and `[[models]]` (`jev-1.13.0`, `jev-latest`, `context_window = 64000`, price 0.042/0.0) are added by later cards.
Plan: the extism-pdk glue sits behind `cfg(target_arch = "wasm32")`, so the pure modules test on the host. Port the six `jev.rs` tests with the documented bodies (J1, J2). A `just plugin-test` recipe runs the crate's tests, and the `plugins` CI job runs it.
Check: `cargo test --manifest-path plugins/Cargo.toml -p cox-plugin-jev`, with these tests:
- `choice_answer_parses_with_probabilities`;
- `score_answer_parses_with_legend`;
- `noul_certainty_is_distance_from_half`;
- `missing_answers_is_an_error_not_a_guess`;
- `unknown_answer_kind_is_an_error`;
- `state_over_32k_tokens_is_truncated_with_marker`;
- `manifest_validates` (through `cox-plugin-api`'s parser).

`cargo build … --target wasm32-unknown-unknown` succeeds.

#### T33.40.3 Jev provider export

Depends: T33.40.2, T33.16, T33.18 · Size: ~170 · Files: `plugins/jev/src/provider.rs`, `plugins/jev/src/lib.rs`, `crates/cox-plugin-fixtures/build.rs`
Goal: `cox_provider_stream` speaks System One over `cox_http` (PL§7a ABI form). The host injects the key.
- A decision call (`Job::Plugin("jev")` with the JSON body in its one user message) is sent verbatim.
- A request from a tier that names `typesafe` gets the old lossy mapping and one `Notice(Warn)`: "typesafe is a decision model; no tier should route to it" (J13).
- Map the status codes from J4 to the ABI error kinds, so the host's retry treats 429 and 529 as transient.
- Pass usage through unchanged; the host applies the estimate floor (T33.18).
- Add the `[[models]]` rows.
- `cox-plugin-fixtures` also builds `jev.wasm`.
Check:
- guest tests `decision_call_body_is_sent_verbatim`, `tier_request_uses_lossy_mapping_and_warns` and `status_529_maps_to_transient`;
- `cargo build -p cox-plugin-fixtures` produces both fixtures, and with the wasm target missing it fails naming `mise install`.

#### T33.40.4 Jev plugin as a provider, end to end and offline

Depends: T33.40.3, T33.7 · Size: ~180 · Files: `tests/plugins_jev.rs` (+ `tests/fixtures/jev/*.json`, fixtures)
Goal: prove the provider path through the real binary with no network and no keychain (J§8).
Plan:
1. A scratch `COX_HOME`. Install and enable the built fixture package from its local folder (C3) with `--yes`.
2. Main turns come from `COX_PROVIDER=scripted`.
3. Jev is wiremock on 127.0.0.1, reached through `[providers.typesafe] base_url` in the scratch config, with `TYPESAFE_API_KEY=test-key` in the env.
4. A test-only scripted command makes one decision call. It reuses T33.40.1's WAT harness pattern.
Check:
- `jev_request_carries_host_injected_bearer` and `jev_key_never_reaches_guest`;
- `jev_call_writes_usage_row_with_plugin_job` (provider `typesafe`, model `jev-1.13.0`, cost from the plugin catalog row);
- `jev_call_blocked_by_budget_falls_back`;
- `jev_401_is_one_notice_and_fail_open`;
- `jev_529_retries_max_retries_times`;
- `three_failures_disable_decide_export`;
- `cox plugin list --json` shows the provider and model contributions.

#### T33.40.5 Parity: the `typesafe` table configures the plugin's section

Depends: T33.40.4 · Size: ~120 · Files: `crates/cox/src/session.rs`, `crates/cox-plugin/src/provider.rs`, `crates/cox/src/doctor.rs`
Goal: while both exist, one name serves one client.
- When the `jev` plugin is loaded, the provider `typesafe` is its `PluginProvider`. The `[providers.typesafe]` table (the built-in default, or the user's) supplies `base_url`, `api_key_env`, `timeout_s`, `max_retries` and `model`, and the user wins, as for declarative sections.
- Without the plugin, the built-in `JevProvider` runs as today.
- `cox doctor` says which client serves `typesafe`.
Check: `typesafe_table_overrides_plugin_transport`, `builtin_jev_used_when_plugin_absent`, and a doctor snapshot for each case (in a scratch `COX_HOME`).

#### T33.40.6 `risk` advisor: raise only

Depends: T33.21 (as amended: ask only when the outcome could change), T33.40.1, T33.40.4 · Size: ~190 · Files: `plugins/jev/src/risk.rs`, `plugins/jev/src/lib.rs`, `tests/plugins_jev.rs`
Goal: J§5.1. There is one request per tool batch. The state is the task, the cwd, the sandbox mode and, for each call, `tool`, `subject`, `input` and `classifier_risk`, never tool output. The questions are the `severity` Score and the `irreversible`, `external_effect` and `exfiltration` Nouls. The thresholds sit in `[plugins.jev.risk]` with the J§5.1 defaults. The advice is "raise to Destructive" or none, and the core keeps `max(builtin, advised)`. `capabilities.decide` gains `risk`.
Check:
- guest tests:
  - `risk_state_never_contains_tool_output`;
  - `risk_thresholds_raise_on_any_hazard`;
  - `risk_low_answers_give_no_advice`;
  - `risk_config_overrides_thresholds`;
- e2e in `Auto` mode with a confining sandbox, where a scripted `git push --force origin main` is auto-allowed without the plugin:
  - a high fixture gives `Ask`, headless denies, and `Advised { point: risk, applied: true }` is in the rollout;
  - a low fixture leaves the call allowed;
  - a wiremock delay past 200 ms leaves it allowed with `Advised { applied: false }`;
  - `one_jev_request_per_tool_batch` (the wiremock count for a batch of 3);
- `risk_advice_cannot_lower_risk` still green.

#### T33.40.7 Eval E1: risk escalation against the classifier alone

Depends: T33.40.6 · Size: ~200 · Files: `evals/src/cox_evals/jev_risk.py`, `evals/tests/test_jev_risk.py`, `evals/risk/commands.yaml` (data)
Goal: measure J§5.1 against the no-Jev baseline at a hard budget cap.
- The corpus has about 300 labelled commands: must-ask (force push, publish, deploy, remote delete, pipe-to-shell, credential exfiltration) and benign (build, test, grep, formatting, local git). The labels and a one-line reason are in the file.
- Each command is one scripted bash call in `Auto` mode with the sandbox on. The scripted provider costs $0, and Jev is live.
- The run is repeated without the plugin as the baseline.
Plan:
1. A `cox_evals` module with a registry entry, not a script (eval-tooling rule).
2. Read the ledger total for `job = plugin:jev` after each batch, and stop at `--max-usd 0.10`.
3. Metrics:
   - the recall gain on must-ask commands the baseline auto-allows;
   - the false-raise rate on benign commands;
   - the late-fallback rate at 200 ms and at 500 ms;
   - p50 and p95 latency;
   - $.
4. The live run needs `TYPESAFE_API_KEY` from the creator. `--dry-run` runs the baseline only.
5. Record the table in R§5 with the Jev model version (`jev-1.13.0`, pinned), the date and the reproduce command.
Falsifier: if the false-raise rate exceeds 10 %, or the p95 late-fallback rate exceeds 20 %, `risk` is not recommended by default, and the user guide says so.
Check: `just test-evals` is green offline (corpus schema, metric maths, budget stop, command line). R§5 has the E1 table.

#### T33.40.8 `route` in the core: cache-aware downgrade offer, turn-local thinking strip

Depends: T33.20 · Size: ~170 · Files: `crates/cox-core/src/router.rs`, `crates/cox-core/src/context.rs`, `crates/cox-core/src/session.rs`
Goal: J§5.2, core side (C2).
- The `route` point is offered only for `Job::Main`, once per `UserTurn`, sticky for that turn's calls.
- It is never offered for `Plan`, for subagents, or after `/model`.
- `cheap` is offered only when its predicted turn cost is ≤ `(1 − route_margin)` × the `code` cost. The prediction uses catalog prices, the last request's prefix size and the cache-read vs cache-write formula in J§5.2. `route_margin` goes in `[plugins.decide]` (default 0.15).
- A downgraded turn strips thinking in its own `Request` only. `inner.history` is unchanged, and there is no `ModelSwitched`.
Check:
- `downgrade_not_offered_when_cache_loss_exceeds_saving`;
- `downgrade_offered_on_first_turn`;
- `routed_down_turn_keeps_history_thinking`: the next `code` request's prefix is byte-identical, and invariant 1 stays green;
- `route_never_offered_for_plan_job`;
- `model_override_disables_route_point`;
- `route_advice_never_routes_up` still green;
- `docs/config.md` drift test green.

#### T33.40.9 `route` advisor: downgrade only

Depends: T33.40.8, T33.40.1, T33.40.4 · Size: ~170 · Files: `plugins/jev/src/route.rs`, `plugins/jev/src/lib.rs`, `tests/plugins_jev.rs`
Goal: J§5.2, plugin side.
- The state is the prompt, the todo list, a summary of the last turn and the number of files touched.
- The questions are the `tier` Choice over the offered tiers (with the plugin's descriptions and `other` → `code`) and the `wants_depth` Noul.
- The advice is `cheap` only when:
  - the choice is `cheap`;
  - confidence ≥ 0.8;
  - `wants_depth` < 0.3;
  - the last turn did not error.
- `capabilities.decide` gains `route`.
Check:
- guest tests `route_needs_high_confidence`, `route_keeps_code_after_error_turn` and `route_other_means_code`;
- e2e over two scripted turns:
  - a fixture choosing `cheap` at 0.9 gives a main-turn `usage` row on tier `cheap` and `Advised { applied: true }`;
  - at 0.7 the turn runs on `code`;
  - a fixture naming `think` is ignored, and the static pick runs.

#### T33.40.10 Eval E2: route downgrade against the static pick

Depends: T33.40.9 · Size: ~180 · Files: `evals/src/cox_evals/harness.py` (the `jev-route` preset), `evals/tests/test_harness.py`, `evals/tasks/11-…14-*.yaml` (data: four tasks that need the code tier, such as a multi-file rename with a failing test)
Goal: measure J§5.2 with money on the line and a hard cap. Every task runs twice, as the baseline and with `[plugins.decide] route = "jev"`, on real Anthropic tiers with live Jev.
Plan:
1. The preset is a registry entry.
2. Caps: `--budget` per run, and a total cap of `--max-usd 3.00` read from the ledger, after which the run stops.
3. Metrics:
   - pass rate;
   - $ per task by (tier, job);
   - the downgrade rate;
   - cache read/write tokens;
   - Jev p95 latency.
4. Keys (`ANTHROPIC_API_KEY`, `TYPESAFE_API_KEY`) come from the creator's env. They are never read from the keychain.
5. R§5 gets the table with the model versions and the date.
Falsifier: if any task that passes in the baseline fails with the plugin, or the median saving per task is below 15 %, `route` stays off by default, and the guide says so.
Check: `just test-evals` is green offline (preset expansion, cap stop, the paired table). R§5 has the E2 table.

#### T33.40.11 User guide: the Jev plugin

Depends: T33.40.6, T33.40.9 · Size: ~150 · Files: `docs/plugins/jev.md`, `docs/plugins.md` (link), `crates/cox/tests/doc_examples.rs`
Goal: one page for users. It covers:
- what Jev is and is not (J13);
- building it from `plugins/jev` (`just plugin jev`) and `cox plugin install <dir>` (C3);
- the key through `TYPESAFE_API_KEY` or the keyring entry `cox/typesafe`;
- enabling points in `[plugins.decide]`;
- per point, the exact state sent to TypeSafe (J17);
- costs in `cox stats` as `plugin:jev`;
- fail-open behaviour and `cox doctor` rows;
- the E1/E2 results and the defaults they justify;
- the migration from `[providers.typesafe]` (J§7).
Check: `doc_examples` parses every `toml` block on the page against `Config` or the plugin manifest schema and fails on drift.

#### T33.40.12 Remove the built-in Jev client

Depends: T33.40.5, T33.40.6 (parity: the provider and one advisor are served by the plugin) · Size: ~120 edited (the deleted `jev.rs` does not count) · Files: `crates/cox-provider/src/jev.rs` (deleted), `crates/cox-provider/src/lib.rs`, `crates/cox/src/session.rs`
Goal: C1, step 1.
- The `typesafe` arm of `backend_for_with` returns the plugin's `PluginProvider` when the plugin is loaded.
- Otherwise it returns a typed "provided by plugin jev, not loaded" error, which the router step (T33.40.13) turns into fail-open.
- `jev.rs`'s tests now live in `plugins/jev` (T33.40.2).
Check: `rg -n 'jev' crates/cox-provider/src` is empty; `typesafe_backend_without_plugin_is_typed_error`; the three standard commands green.

#### T33.40.13 Router and `ProviderId` without Jev

Depends: T33.40.12 · Size: ~120 · Files: `crates/cox-protocol/src/types.rs`, `crates/cox-core/src/router.rs`, `crates/cox-core/src/session.rs`
Goal: C1, step 2.
- `ProviderId::Jev` goes, replaced by T33.18's plugin bucket; it is never serialized in events (J§7).
- The `typesafe` pin in `Router::pick` goes; plugin sections resolve generically.
- A tier naming a legacy plugin provider (the table `("typesafe", "jev")`) with the plugin absent fails open (D14): one `Notice(Warn)` "Jev moved to a plugin: build `plugins/jev` and run `cox plugin install <dir>`", and that tier uses its `default.toml` provider and model for the session.
Check: `legacy_typesafe_tier_without_plugin_uses_default_tier_with_notice`, `typesafe_tier_with_plugin_routes_to_plugin_section`, `unknown_provider_still_errors`; `docs/protocol.jsonschema` regenerated.

#### T33.40.14 Config tombstone, `default.toml`, schema and doctor

Depends: T33.40.13 · Size: ~150 · Files: `crates/cox-protocol/src/config.rs`, `crates/cox/src/doctor.rs`, `crates/cox-protocol/default.toml` (config data; `docs/config.md` and the config schema are generated)
Goal: C1, step 3. An old config loads.
- `JevProviderConfig` becomes `LegacyTypesafe`: the same keys, still `deny_unknown_fields`, an `Option` with no default section.
- It stays a named field, so the table can never fall into the flattened `custom` map as a chat-shaped `CompatibleProviderConfig` (J§7).
- Its knobs feed the plugin's `typesafe` section. Its `models` rows join the config catalog layer.
- The `[providers.typesafe]` block leaves `default.toml`.
- The doctor's key check covers plugin provider sections generically. A leftover table without the plugin is a doctor warning with the install pointer.
Check:
- `old_typesafe_table_loads_with_notice` (a config file from before this change);
- `typesafe_table_never_becomes_compatible_section`;
- `unknown_key_in_typesafe_table_still_rejected`;
- a doctor snapshot;
- the config drift test and `every_flag_has_a_config_key` green.

#### T33.40.15 Catalog, prices and vendor script without Jev

Depends: T33.40.14, T33.16 · Size: ~110 · Files: `crates/cox-models/src/catalog.rs`, `crates/cox-models/src/price.rs`, `scripts/vendor/src/cox_vendor/models.py` (+ its tests; `prices.toml` is regenerated by the script, A48)
Goal: C1, step 4.
- The `typesafe` special cases in `Catalog::load` and `price.rs` go.
- `cox-vendor models` stops keeping `jev-latest` as a known exception, and its re-run drops the row from `prices.toml`.
- Jev's price and window now come only from the plugin row. `cox doctor` shows `source = plugin:jev`, and T30.27's price-sync row stays green.
Check: `jev_price_comes_from_plugin_row`, `catalog_without_plugin_has_no_jev_row`, the vendor pytest suite, `just vendor models --check` clean.

#### T33.40.16 Docs and plan sweep after the removal

Depends: T33.40.15 · Size: ~80 · Files: `docs/design/providers.md`, `docs/design/crates.md`, `docs/design/v0.2-jev.md` (+ `plan.md`)
Goal: no doc describes a built-in Jev.
- `providers.md` loses the Jev family.
- `crates.md` loses the `cox-provider-jev` row.
- `v0.2-jev.md` gets a closing note that the integration shipped as the `jev` plugin (A52, T33.40), with the eval verdicts.
- In `plan.md`, T32.15 is dropped with a reason, and the PL§2 example uses `name = "typesafe"`.
Check: `rg -n -i 'jev|typesafe' crates docs` matches only the plugin, the tombstone, the migration notice and the history. All docs drift tests are green.

#### T33.40.17 Optional: record live fixtures (needs the creator's key)

Depends: T33.40.6, T33.40.9 · Size: ~150 · Files: `scripts/vendor/src/cox_vendor/jev_fixtures.py`, `scripts/vendor/tests/test_jev_fixtures.py`, `tests/fixtures/jev/*.json` (data)
Goal: replace the hand-built fixtures, which follow the documented shapes, with recorded ones.
- The script sends the plugin's own `risk` and `route` question sets for a handful of fixed states.
- It redacts with the rollout scrubber, writes the bodies and records the model version and the date.
- It runs only with `TYPESAFE_API_KEY` set by the creator. It never reads the keychain.
Check: the offline pytest (body construction, redaction, no key means a clear exit) is green. With recorded fixtures, T33.40.4 and T33.40.6 stay green unchanged.

#### T33.41 Optional: `cox plugin link` (dev loop)

Depends: T33.7 · Size: ~110 · Files: `crates/cox/src/plugin_cmd.rs`, `crates/cox-plugin/src/grant.rs`
Goal: use a built plugin in place without installing it. A linked plugin asks again only when its capabilities widen, never on byte changes. It is marked `dev` in `list`, `doctor` and the TUI grant dialog. `cox plugin build` and `cox plugin dev` are not planned (PL§13).
Check: `linked_plugin_rebuild_does_not_reask`, `linked_plugin_widening_reasks`, `linked_plugin_marked_dev_everywhere`.

#### T33.42 Sandbox every MCP stdio server, with a per-server opt-out

Depends: T33.19 · Size: ~140 · Files: `crates/cox-mcp/src/client.rs`, `crates/cox/src/session.rs`, `crates/cox-protocol/src/config.rs`
Goal: extend the sandbox wrap from T33.19 to every MCP stdio server, not only plugin-shipped ones (`docs/design/plugins.md` §7c, resolved 2026-09-26, §14 decision 4). `crates/cox` wraps every stdio command with `sandbox::Policy` before `cox-mcp` spawns it, including today's user-configured `.mcp.json`/config servers. A per-server `sandbox = false` key opts a named server out, for setups that need it, and `cox doctor` gains a row naming any server that opted out.
Check: `every_stdio_server_runs_under_sandbox_by_default`, `sandbox_false_opts_a_named_server_out`, `doctor_lists_unsandboxed_servers`; existing `.mcp.json`/config MCP e2e tests still pass with the wrap applied.

#### T33.43 Bump extism to a release on wasmtime ≥ 48 and drop the advisory ignores

Depends: an extism release after v1.30.0 that pins wasmtime ≥ 48 (extism `main` already pins 48; checked 2026-09-26) · Size: ~30 · Files: `Cargo.toml`, `Cargo.lock`, `deny.toml`
Goal: move the workspace `extism` and the direct `wasmtime` (declared only for its `anyhow` feature) to that release, then remove the `RUSTSEC-2026-0222` and `RUSTSEC-2026-0269` entries from `deny.toml` `ignore` (A55, research.md P39). Also check whether the direct `wasmtime` declaration is still needed. The bump was approved in advance by the creator (A55), but only onto a published crates.io release, never a git dependency. It unblocks the WASI preopens in T33.14. If no such release exists by 2026-12-31, bring it back to the creator.
Check: `cargo deny check advisories` passes with no wasmtime ignores; the `cox-plugin` tests and `slim_build_has_no_wasm_runtime` pass; `scripts/footprint.sh` stays within the 20 MiB budget (PL§12).

**Order.** T33.1 → T33.2 → T33.3 → T33.4 → T33.5 → T33.6 is the critical path. After it these can run in parallel:

- T33.7–T33.8;
- T33.9 → (T33.10, T33.11, T33.12, T33.14, T33.15);
- T33.16 → T33.17 → T33.18;
- T33.19 (after T32.3) → T33.42;
- T33.20 → T33.21.

TUI: T33.22 → T33.23 → (T33.24, T33.25, T33.26). SDK and languages: T33.27 → T33.28 → T33.29 → T33.30, then T33.31 → T33.32 → T33.33; T33.34; T33.35 → T33.36; T33.37 → T33.38. Then T33.39; T33.40.1–T33.40.17 (own order below); T33.41 and T33.42 whenever they are wanted. The top table gets rows T33.1–T33.39, T33.41–T33.42 and T33.40.1–T33.40.17; P2 by default, P1 for the blockers T33.1–T33.6 and T33.40.1, P3 for the optional cards (T33.41, T33.40.17), the paid eval E2 (T33.40.10) and the Kotlin/Dart feasibility spikes (T33.35, T33.37).

**T33.40 order.**

- Main line: T33.40.1 and T33.40.2 (in parallel) → T33.40.3 → T33.40.4 → T33.40.5.
- Risk: T33.40.6 → T33.40.7.
- Route: T33.40.8, which can start after T33.20 → T33.40.9 → T33.40.10.
- Docs: T33.40.11 after T33.40.6 and T33.40.9. Its results section is filled by T33.40.7 and T33.40.10.
- Removal (C1): T33.40.12 → T33.40.13 → T33.40.14 → T33.40.15 → T33.40.16, starting once T33.40.5 and T33.40.6 are done. It does not wait for the evals.
- T33.40.17 runs whenever the creator has a key.

The paid runs are T33.40.7 (≤ $0.10, approved) and T33.40.10 (≤ $3, needs the creator's go-ahead each time).

### P34 — Subagents (goal: a subagent can be a custom named definition, capped in number, able to ask the user, and able to exchange follow-up messages with its parent and its siblings — all through the parent's own `Submission`/`Event` stream)

Rationale in §6 A53. The design doc for the messaging cards is `docs/design/subagent-messaging.md` (T34.0, cited below as SM§n).

Every card in this phase:

- stays within 200 LOC and 3 source files (generated schemas and fixtures do not count);
- leaves a test that fails without it;
- documents what it adds (`docs/design/subagent-messaging.md` if the design moves, `docs/protocol.jsonschema`/`docs/config.jsonschema` through their drift tests for new variants or keys);
- runs the three standard commands.

**Blockers** (everything after them depends on them): T34.0 (blocks T34.4–T34.9).

#### T34.6 The `send_message` tool

Depends: T34.5, T34.2 · Size: ~170 · Files: `crates/cox-tools/src/send_message.rs` (new), `crates/cox-core/src/subagent.rs`
Goal: `send_message { to, text }` for a subagent (`to: "parent"` or a sibling's name/`TaskId`, relayed through the parent per T34.0 §4) and for the parent (`to: <child name/id>`). T34.5 already routes and hop-limits (`MAX_HOPS`); this card adds the tool, `Session::self_task`, and the `MAX_MESSAGES_PER_TASK = 16` received-message cap (SM§5: a named constant, not a config key), so a flood is denied instead of spinning.
Plan:
1. `cox-tools/src/send_message.rs`: the `Tool` impl. It parses `{to, text}` and emits the child's `Event::TaskMessage` (or, in the parent, a `Submission::TaskMessage`) through a `ToolCx`/trait hook. It never touches the registry itself.
2. `cox-core/src/subagent.rs`: `self_task` is set in `spawn`. The parent resolves `to` by name or `TaskId`. `MAX_MESSAGES_PER_TASK` counts deliveries per `TaskId`, and the 17th is `ToolError::Denied`, shaped like T34.2's denial.
3. A message that wakes a *dormant* child (`wake`, from `deliver`/`park_child`) must hold a T34.2 slot for that run, reserved with `try_reserve_agent_slot`. At the cap, the sender gets `Denied` and the message is not queued. T34.2 left this gap: a woken child today runs outside the cap.
4. No preset grants `send_message` yet, the same as `ask_user` (SM§4).
Check: `sibling_message_is_relayed_through_the_parent_session`, `message_cap_denies_the_nth_plus_one_follow_up`, `waking_a_dormant_child_at_the_cap_is_denied` (T34.5 already has `hop_limit_stops_a_ping_pong`).

#### T34.8 ACP rendering, and the dropped task-lifecycle events

Depends: T34.6 · Size: ~120 · Files: `crates/cox-acp/src/server.rs`
Goal: `drive_prompt`'s event loop today falls into `Ok(_) => {}` for everything except `TurnDone` and `ApprovalRequired` (server.rs:344), so an ACP client (Zed, JetBrains) never sees `TaskCreated`/`TaskCompleted`/a delivered `TaskMessage`. This card gives those three their own arms: `TaskCreated`/`TaskCompleted` become a plan/task update the same way `ask_permission` already labels a relayed approval, and a `TaskMessage` renders with the same label.
Check: `acp_reports_task_created_and_completed`, `acp_reports_a_delivered_task_message`.
Plan: starts before T34.6 lands, because the three events already exist and the tests drive `drive_prompt` with injected events. Add three arms in `server.rs`: `TaskCreated`/`TaskCompleted` become a plan/tool-call update, and `TaskMessage` becomes an agent-message chunk with the task label. All text goes through `cox_sanitize::sanitize`.

#### T34.9 e2e: two subagents messaging through the parent

Depends: T34.6 · Size: ~150 · Files: `tests/subagent_messaging.rs` (new)
Goal: with the Scripted provider, a parent spawns two children; child A sends `send_message` to child B by name; the parent relays it; B replies; the parent's history carries both pointer lines; a scripted ping-pong hits T34.6's hop limit and stops instead of looping forever.
Check: `parent_relays_a_message_between_two_children`, `hop_limit_stops_a_scripted_ping_pong` — both against the real event stream, no network, no API key (D12).

### P35 — External agents from plugins (Cursor first) (goal: a plugin can declare an external CLI agent that appears to the model as a subagent preset, driven over its own official headless protocol, sandboxed and grant-gated like every other plugin capability)

Rationale in §6 A54; the design is `docs/design/external-agents.md` (T35.0, cited below as EA§n). Evidence `research.md` §4.3.8 (Cursor, checked 2026-09-26).

Every card in this phase:

- stays within 200 LOC and 3 source files (manifests, generated schemas, snapshots and fixtures do not count);
- leaves a test that fails without it;
- documents what it adds (`docs/design/external-agents.md` if the design moves, `docs/plugin.schema.json` through its drift test for the new manifest capability, `docs/plugins.md` user docs from T35.9);
- runs the three standard commands.

**Blockers** (everything after them depends on them): T35.0, T35.1, T35.2, and the P33/P34 work this phase builds on — T33.6 (grants and granted-only loading, which implies T33.1–T33.5), T33.19 and T33.42 (sandboxed stdio spawn for a plugin-brought process), T34.1 (custom preset dispatch) and T34.5 (parent-routed follow-up messages).

#### T35.2 Host spawner, sandbox and grant — blocker

Depends: T35.1, T33.6, T33.19, T33.42 · Size: ~190 · Files: `crates/cox-plugin/src/external_agent.rs` (new), `crates/cox/src/session.rs`
Goal: resolve a granted `[[external_agents]]` entry to a `std::process::Command` (in-package path or PATH program), the same resolution shape T33.19 gives `[[mcp]]`; `crates/cox` wraps it with `sandbox::Policy` before spawning, exactly as it already does for a plugin's MCP stdio server (PL§7c) — no second sandbox path. The capability is one more line the grant dialog lists in words (PL§2's "the capability list is the unit of approval"); `grant::check` needs no change, since it already treats the manifest's capability set generically.
Check: `external_agent_command_is_wrapped_by_sandbox_before_spawn`, `path_program_is_shown_verbatim_at_approval`, `ungranted_external_agent_is_not_spawned` (matches `headless_never_loads_ungranted_plugin`, T33.6).

#### T35.3 ACP client adapter

Depends: T35.2 · Size: ~190 · Files: `crates/cox-acp/src/client.rs` (new), `crates/cox-acp/src/lib.rs`
Goal: cox as an ACP client over the spawned process's stdio, reusing the `agent-client-protocol` crate `crates/cox-acp` already depends on as a server (no new dependency, per EA§4). `session/request_permission` from the agent is decided by `cox_permission::Engine`, the same single guard every other tool call goes through; an `fs/*` or `terminal/*` request is served only through `path::confine` and the sandbox policy already governing the spawned process, or refused with the reason named.
Check: `acp_client_relays_request_permission_through_the_engine`, `acp_client_fs_request_is_confined_to_the_workspace`, `acp_client_terminal_request_without_sandbox_grant_is_refused`.

#### T35.4 stream-json adapter

Depends: T35.2 · Size: ~170 · Files: `crates/cox-core/src/external_agent.rs` (new), `crates/cox-core/src/subagent.rs`
Goal: a pure, host-side line mapper (EA§5) from Cursor CLI's `stream-json` event shapes (research.md §4.3.8: `system`/`user`/`assistant`/`tool_call{started,completed}`/`result`) onto `cox_protocol::Event`/`Item`; an unrecognised line becomes a sanitized `Notice`, never a hard error (D14), matching `broken_hook_is_skipped_not_fatal`'s fail-open shape.
Check: `stream_json_assistant_line_maps_to_cox_event`, `stream_json_tool_call_started_and_completed_pair_map_to_one_item`, `unrecognised_stream_json_line_becomes_a_sanitized_notice`.

#### T35.5 Wiring as a subagent preset

Depends: T35.3, T35.4, T34.1, T34.5 · Size: ~190 · Files: `crates/cox-core/src/subagent.rs`, `crates/cox-core/src/session.rs`, `crates/cox-core/src/tasks.rs`
Goal: a granted `[[external_agents]]` entry registers as one more name `AgentTool::preset()` resolves (T34.1's path), so `agent(preset: "cursor")` dispatches it exactly like a discovered `.cox/agents/*.md` definition, picking its ACP (T35.3) or stream-json (T35.4) driver from the manifest's `mode`; a follow-up to a running or finished external-agent task, and a sibling message addressed to it, are routed the same way T34.5 already routes to any other child task — no second messaging path. Usage is recorded per EA§6 (a `$0`/`billed_externally: true` row unless the driver ever reports tokens).
Check: `agent_dispatches_a_granted_external_agent_preset_by_name`, `task_message_reaches_a_running_external_agent_task` (reusing T34.5's fixture shape), `external_agent_turn_writes_a_billed_externally_usage_row`.

#### T35.6 The Cursor plugin package

Depends: T35.1, T35.5 · Size: ~120 · Files: `plugins/cursor/plugin.toml`, `plugins/cursor/src/lib.rs`, `plugins/Cargo.toml` (member)
Goal: the first real user of `[[external_agents]]`, the same role Jev played for the ABI provider form (T33.40): `plugin.toml` declares one entry (`name = "cursor"`, `command = "agent"` resolved on `PATH`, `mode = "acp"` by default with `stream-json` as the manifest's documented alternative, `key_env = "CURSOR_API_KEY"`); the guest exports only `cox_init` (no other capability), since spawning and driving the process is entirely the host's job (EA§2) — the smallest possible plugin, unlike Jev's `api = "plugin"` provider guest.
Check: `cox plugin list` (e2e, scratch `COX_HOME`) reports the `cursor` plugin's `external_agents` capability; `cursor_plugin_toml_matches_the_manifest_schema`.

#### T35.7 e2e: a fake `agent` binary replaying recorded fixtures

Depends: T35.5, T35.6 · Size: ~190 · Files: `tests/external_agents_cursor.rs` (new), `tests/fixtures/cursor/*.json` (data), `scripts/vendor/src/cox_vendor/cursor_fixtures.py` (+ its tests)
Goal: fixtures are the documented `stream-json` and ACP event shapes from research.md §4.3.8, recorded by a saved, tested script under `scripts/vendor` (AGENTS.md: a file no package manager fetches comes only from such a script, never hand-pasted) — no live Cursor call, no key. A test-only fake `agent` binary replays a fixture's lines over stdio in both modes; the e2e drives it through the real `cox-plugin`/`cox-acp`/`cox-core` path (D12: no network, no API key).
Check: `fake_agent_stream_json_reaches_a_cox_event_stream_unchanged`, `fake_agent_acp_permission_request_is_decided_by_the_engine` — both against the real code path, no scripted-provider shortcut for this one (it is not a model call).

#### T35.8 `cox doctor` reporting

Depends: T35.2 · Size: ~120 · Files: `crates/cox/src/doctor.rs`, `crates/cox-plugin/src/external_agent.rs`
Goal: a doctor row per granted `[[external_agents]]` entry: CLI binary found on `PATH` (and its `--version`, best-effort), `key_env` set or missing, sandboxed or opted out (T33.42's per-server opt-out shape). A missing CLI or key is the fail-open warning EA§7 specifies, with the preset left out of `agent`'s names, not a hard failure.
Check: `doctor_reports_missing_cli_as_a_warning_not_a_failure`, `doctor_reports_key_env_set_and_cli_version`.

#### T35.9 User guide: the Cursor plugin

Depends: T35.7 · Size: ~130 · Files: `docs/plugins/cursor.md`, `docs/plugins.md` (link), `crates/cox/tests/doc_examples.rs`
Goal: install and grant the plugin, set `CURSOR_API_KEY`, dispatch it with `agent(preset: "cursor")`, read `cox doctor`'s row when something is missing — the same shape `docs/plugins/jev.md` (T33.40.11) already gives Jev.
Check: the doc's commands are checked against the real binary the way `doc_examples.rs` already checks other pages.

#### T35.10 Optional: live check against a real Cursor account (needs the creator's key)

Depends: T35.7 · Size: ~130 · Files: `scripts/vendor/src/cox_vendor/cursor_live_fixtures.py` (+ its tests), `tests/fixtures/cursor/*.json` (data, recorded from one real run)
Goal: with the creator's own `CURSOR_API_KEY` and the installed CLI, one real `agent -p --output-format stream-json` and one real `agent acp` run against a scratch repo, recorded into the same fixture shape T35.7 already consumes — confirms the documented event shapes still match a real CLI release; never runs in CI, matches T33.40.17's shape.
Check: the recorded fixture round-trips through T35.7's mapper unchanged; the script's own test asserts it never touches a real key by default (opt-in env var required, same guard as T30.13's local-model comparison).

**Order.** T35.0 → T35.1 → T35.2 is the critical path (it also waits on T33.6, T33.19, T33.42, T34.1, T34.5, whichever lands last). After T35.2: T35.3 and T35.4 run in parallel → T35.5 → (T35.6, T35.8 in parallel) → T35.7 → T35.9; T35.10 runs whenever the creator has a key. The top table gets rows T35.0–T35.10; P1 for the design doc and the critical path through the host spawner and the preset wiring (T35.0–T35.2, T35.5), P2 for the two drivers, the plugin package, the fixture e2e, doctor reporting and the user guide (T35.3, T35.4, T35.6–T35.9), P3 for the optional live check (T35.10).

### P31 — Beta readiness (goal: the v0.1 definition of done in §4 holds for everything cox can prove without a paid key)

Rationale in §6 A50. T31.1–T31.5 are in `done.md`; T31.2 landed as a no-op (see A50 and its done.md card — T30.23 had already made Jev construction fallible). Still open against §4, all outside the code: the paid eval run and the cache-read ratio (T30.3, a funded `ANTHROPIC_API_KEY`), and a signed macOS release (the `MACOS_CERTIFICATE` / `MACOS_CERTIFICATE_PWD` repository secrets).

## 4. Definition of done for v0.1

1. `cox` runs a multi-turn coding session against Anthropic, OpenAI Responses and a local Ollama model with the same tool set, with the sandbox on, on macOS and Linux.
2. `cargo test --workspace` passes offline with no API key in under 90 s on CI; every widget, transcript cell and loop scenario has a snapshot.
3. A user with `.claude/settings.json`, `CLAUDE.md`, `.claude/commands`, `.claude/agents`, `.mcp.json` and rtok hooks gets identical behaviour without editing them.
4. `cox stats` shows cost by tier and job; the `just bench` table in `research.md` §4.6 shows measured savings for each D6 mechanism; cache-read ratio on turn ≥ 3 of a typical session is ≥ 80 %.
5. `cox run -p` and `cox acp` pass their conformance tests; `cox mcp` serves `read`/`grep`/`glob` to Claude Code.
6. No `unwrap`/`panic!` outside tests; `cargo deny` clean; fuzz jobs green.
7. The seventeen invariants in §1.15 each have a passing, named test.

## 5. Roadmap

| Milestone | Phases | What a user can do | Tasks |
|-----------|--------|--------------------|-------|
| M1 "talks" | P0, P1, P2, T5.1–T5.3 | chat with tools in a scripted or real provider, resume a session | 26 |
| M2 "edits safely" | P3, P4, rest of P5 | daily-driver TUI: diff-shaped edits, sandboxed shell, approvals, diff view | 21 |
| M3 "fits in" | P6, P7 | headless/CI, MCP both ways, Claude Code/Codex config compatibility, hooks, skills | 13 |
| M4 "cheap" | P8, P9 | compaction, archive/expand, dedup, deferred tools, tiered routing, budgets, measured savings | 9 |
| M5 "everywhere" | P10, P11, P12 | memory, Zed/JetBrains via ACP, evals, release | 9 |
| M6 "competitive TUI" | P22–P30 | nothing documented is a no-op; themes, tool cards, queue, `Shift+Tab`, `/rewind` incl. shell changes, visible agents, `--plain`, published footprint | 48 |

Order of value if time is short: M1 → M2 → P8 (T8.1–T8.3) → P6 → P7 → the rest. M4 before M3 if cost is the pain; M3 before M4 if adoption is.

## 6. Plan amendments

- A1 D9, §1.1, §1.7, T0.4 — Diesel 2.2 (sqlite, bundled libsqlite3-sys with FTS5, diesel_migrations) replaces rusqlite as the store layer, matching rtok D13. Why: user request; typed models for ledger joins; sync so hooks and tests need no runtime. Effect on other tasks: T1.7, T8.3, T8.4, T10.1, T10.3 write Diesel queries in `cox-store`, never SQL elsewhere.
- A2 §2, `AGENTS.md` — agents work only on `main`; no `cox/<task-id>` branches. Why: user request. Effect: claim, commit and finish tasks on `main`.
- A3 §2, `AGENTS.md` — don't duplicate code or logic; reuse an existing helper or extract one shared helper at the responsible layer. Why: user request.
- A4 §2, `AGENTS.md` — every implemented task is marked done and moved to `done.md` with its Check output. Why: user request.
- A5 §1.2, `cox-protocol::Tool`, T3.5 — added `Tool::risk(&self, input) -> Risk`, defaulting to `spec().risk`; `cox-core::turn::run_tools` now asks the tool instead of reading `spec().risk`. Why: T3.5 step 4 and the §4 tool table require `apply_patch` to be `Destructive` only when a patch deletes > 5 files, and a static `ToolSpec` cannot express a per-call risk. Effect on other tasks: none — every other tool inherits the default; T2.2's permission engine keeps reading `ToolCall.risk`.
- A6 D13, §1.1, §1.6, T0.7 — `dotenvy` 0.15 on `cox` loads `.env` then `.env.local` from cwd (walk up) into unset process env before figment. Why: user request (local API keys / `COX_*` without a second config file). Effect: not a figment layer and does not override set variables, so D12 tests and CI keep winning; provenance stays `env`. `cox-core` does not take this dep.
- A7 `website/` — added a standalone Hugo documentation site using Tailwind CSS (modern home page plus architecture and configuration references). Why: user request. Effect: no runtime crate or release behaviour changes; publish with `hugo --source website`.
- A8 `website/`, `.github/workflows/deploy-pages.yml` — deploy the Hugo site to GitHub Pages from `main`, building within `website/` and publishing `website/public`. Why: user request. Effect: the deploy workflow installs the pinned Tailwind dependencies and runs only when site/workflow files change.
- A9 §1.1, §1.6, D3 — provider registry in two types (`docs/design/providers.md`). Type-1 (native `Provider` impl per *wire protocol*): `AnthropicProvider`, new `OpenAiResponsesProvider` (`POST /responses`, bearer-optional), `OpenAiChatProvider` (+`models` list, `from_parts`). Type-2 (compatible, zero code): any `[providers.<name>]` table (`CompatibleProviderConfig`: `base_url`, `api_key_env`, `api`, `model`, `context_window`, `models`) served by the shared Chat/Responses clients; seed `deepseek`, `openrouter` (curated), `moonshot`, `z-ai` with per-model `{id, context_window, efforts}` from models.dev and matching `prices.toml` rows (20 rows; `qwen3-coder` costed 0). Why: user request — adding DeepSeek/OpenRouter must be config lines, never a `DeepseekProvider` struct duplicating the Chat client. Effect: `ProvidersConfig` loses `deny_unknown_fields` (flattened `custom` map, Hooks/Mcp precedent — a typo'd table parses but fails closed in router/session); `Router::pick` accepts custom names (Local family id, section-model pin, per-model effort clamp to greatest-supported-≤-request); `provider_for` builds Responses-or-Chat per `api` (unknown `api` bails at startup); `--provider <name>` propagates to all tiers for any non-first-party name; `cox-provider::http` unifies key resolve/error mapping (5xx is now `Overloaded` on every backend); `Effort` gains `Ord`; `figment` becomes a `cox-protocol` dev-dep for the default.toml shape test. Deferred: per-model effort *enforcement* beyond the clamp (gateway models pass through), keyring fallback for custom keys (env-only), shared retry policy for OpenAI-shaped clients, `cox doctor` prices-age check.
- A10 D16, P13 — implement vendor-neutral OpenTelemetry observability as three bounded tasks: OTLP/HTTP traces+logs exporter, GenAI semantic instrumentation, then backend documentation/smoke stack. Why: user requested full AI-agent telemetry visible in Maple, SigNoz, Jaeger and Grafana. Effect: standard OTEL environment variables remain the portability contract; raw prompt/completion/tool content is opt-in only because it can contain source code and secrets; operational metadata, usage, costs and errors are always exported when telemetry is enabled.
- A11 §1.6, P14 — the TUI must render on any terminal font, glyph set, colour depth and language: one glyph table with an ASCII fallback and `[tui.icons]` overrides (T14.1), colour-depth downgrade plus `NO_COLOR` (T14.2), and syntect highlighting extended from markdown fences to file-shaped tool output and diff hunks with a configurable theme (T14.3). Why: user request. Effect: adds `tui.glyphs`, `[tui.icons]`, `tui.color`, `tui.syntax_theme` to §1.6; no new dependency (syntect and unicode-width are already in the tree); a font is the terminal's to choose, so cox's contract is width-correct, degradable output rather than font selection. Already covered and not redone: fenced-code highlighting (T5.3), wide/combining width handling (T5.3, T5.6), `tui.theme` (T5.1).
- A12 §1.11, T3.7 — `bash` gains an optional `shell` input (`sh` default, plus `bash`, `zsh`, `fish`, `dash`, `ksh`, `tcsh`, `nu`, `pwsh`); `cox_tools::sandbox::command` takes the shell path instead of hardcoding `/bin/sh`. Why: user request — command lines written for a specific shell (fish substitution, zsh globs) failed under `sh`. Effect: the schema enum *is* the allowlist, so a name the model invents is a deserialisation error and never reaches a spawn; the binary is resolved in `/bin`, `/usr/bin`, `/usr/local/bin`, `/opt/homebrew/bin` and never on `PATH`; a missing shell is `ToolError::Denied`. Risk classification stays tree-sitter-bash, which rates an unparseable (e.g. fish-only) line `Exec` — fails closed, never lower.
- A13 §1.1, §1.6, §1.13, P15 — git belongs to the *surfaces*, not to the tool catalogue: a `cox_tools::git` module (branch, worktree `+n −m`, worktree diff, local branch names) feeds a status-line segment (T15.2), a `Ctrl+G` Diff view that renders the worktree diff through the existing `cox_tui::diff` (T15.3), and git-aware completion of a shell line in the composer (T15.4). Why: user request. Effect: **no `git` tool** — `bash` (A12) already runs git, and a second exec path would need its own risk classification, permission rules and sandbox story to say what `Bash(git:*)` already says; `cox-tui` still never spawns a process, so `crates/cox` polls `cox_tools::git` and pushes the result in exactly as it already fills the `@` picker's file list; adds `tui.git = true|false` to §1.6 and `Ctrl+G` to the §1.13 keymap; no new dependency — git is shelled to, not linked. Untracked files are outside the counts and the diff, because including them means writing to the index and a status line is a reader (`GIT_OPTIONAL_LOCKS=0` for the same reason). A branch name and a diff body are repository input, so both reach the terminal through `text::sanitize` like any other untrusted string.

- A13 §1.7, §1.9, §1.12, T1.7, T10.3 — the ledger records the routed `effort` (migration `00000000000002_usage_effort`, `usage.effort TEXT` nullable, `UsageRow.effort: Option<Effort>`), and `cox sessions <ID>` prints one session's stored record: start, end, duration, turns, cost, then tokens and cost grouped by `(provider, model, effort)`. Why: user request — the database held every other dimension of a call but not the effort it ran at, and nothing joined the `sessions` row to its ledger rows in one view. Effect: the column is nullable rather than defaulted so pre-migration rows stay honest (printed `-`, never guessed `high`); `cox stats` gains an `effort` column in its table and CSV (the CSV header changed); `ledger_row` takes an effort argument; `cox sessions` gains a positional id that conflicts with `--grep`. Documented in `docs/observability.md` ("What the database records about a session").
- A14 §1.7, §1.9, §1.13, T7.4, P16 — concurrent sessions on one workspace see each other. Every session keeps a presence record `COX_HOME/presence/<session>.json` (pid, cwd, project root, `active|waiting|idle|stopped`, turn, last-edited paths, heartbeat) written by a built-in `Hook` (`cox_ext::presence::PresenceHook`, wrapping `ShellHooks`) — the seam the surface already installs, so the core still spawns nothing and opens no file. On `UserPromptSubmit` the hook reads the other records of the same project and returns them as `additional_context`; the core appends that as a second text block on the user message (never shown as the user's words, never in `system[0..=2]`, so the cache prefix is untouched), and `ShellHooks` maps Claude Code's `additionalContext` the same way, so Claude Code hooks that add context now work too. `PermissionRequest` fires when the engine escalates, so "waiting for approval" is observable. The TUI gets a `Msg` feed channel from the binary (`app::run(session, state, feed)`), which polls presence every 2 s; `/agents` lists live agents with status and the status line counts them; `/effort` is a session-wide override the router clamps like a tier effort; `/sessions` lists this project's recent sessions preloaded by the binary. Why: user request — several agents share this worktree and none knows the others' files are mid-edit. Effect: no new dependency; presence is a directory of small files rather than a table because it is process liveness, not history (a crashed process leaves a record whose stale heartbeat *is* the signal; `SessionEnd`/`Drop` remove it); the feed channel is the receiver T15.2 planned, so T15.2 sends `Msg::Git` on it instead of adding its own `select!` arm; in-place `/resume` stays out (T16.5 prints the command).
- A15 `Cargo.toml`, `justfile`, `mise.toml` — build profiles: `profile.dev` keeps line tables only and builds dependencies at `opt-level = 1` without debug info (the bulk of a debug target tree was dependency DWARF nobody steps through); `profile.dist` moves from thin to fat LTO with one codegen unit and stripped symbols, and deliberately keeps `panic = "unwind"` because `cox-mcp` turns a panicking task into a `JoinError` and keeps going (fail open on extensions). `just release` builds the dist profile and prints the binary size; `just cache` / `just cache-autoclean` wrap `cargo-cache` 0.8.3 (a mise tool, not a crate dependency) because the shared cargo home fills the disk. Why: build time and binary size; no crate, feature or runtime behaviour changes. Effect: CI builds dependencies at `opt-level = 1`, which is slower to compile once and faster to test; the release workflow is unchanged.
- A16 §1.13, T5.1 — screenshots and whole-screen tests for the TUI. `cox_tui::svg::buffer_to_svg` turns a rendered `Buffer` (plus the cursor `view` returns) into an SVG — one `<rect>` per background run, one `<text>` per styled run with `textLength` pinning the columns, ANSI/256/24-bit colours mapped to CSS — so a picture of the screen is derived from the same buffer the snapshot tests compare as text. `crates/cox-tui/tests/screenshots.rs` composes the terminal the way `app::run` does (scrollback rows for every finished cell above the 15-row inline viewport) for eleven states (fresh session, streaming reply after a read, finished turn with thinking collapsed, bash approval modal, slash palette, `@` picker, diff view, todo panel, running bash tool with background tasks, security banner with a warning and an error, light theme), snapshots each with insta and, when `COX_SCREENSHOTS=<dir>` is set, writes `<dir>/<name>.svg`; `just screenshots` regenerates `docs/screenshots/`. `tests/keys.rs` covers what the frame tests left out: `Ctrl+D`, paste, the `Ctrl+R` history picker, `ApprovalDecided` and `ModelSwitched` arriving from the runtime, `TaskCompleted`, the tick, and the pinned security banner. Why: user request — a way to see the TUI without running it, and tests for the keys and runtime messages nothing exercised. Effect: no new dependency (`unicode-width` was already a `cox-tui` dependency); nothing at runtime calls the SVG renderer; the SVGs are committed artifacts regenerated by the recipe, not built in CI.
- A17 `release-plz.toml`, `.github/workflows/release-plz.yml`, `CHANGELOG.md`, `config/`, T12.2, T12.3 — release-plz owns the version and the changelog; cargo-dist keeps building. Every push to `main` opens or refreshes the release PR (bump of `[workspace.package].version`, a `## [x.y.z]` section in `CHANGELOG.md` from the commits since the last `v*` tag); merging it creates the `v<version>` tag through the GitHub API, and that tag triggers `release.yml` exactly as a hand-pushed tag did. Config: git-only mode because nothing goes to crates.io (`git_only`, `publish = false`, `semver_check = false`); one tag `v{{ version }}` made by the `cox` package only, since release-plz tags package by package and checks only the local clone for an existing tag, so ten crates naming one tag would fail on the second; no GitHub Release from release-plz (cargo-dist creates it and takes the notes from the changelog section); `release_always = false`; one changelog written by `cox` with `changelog_include` of the nine libraries. The grep test's gitignored fixtures (`fixtures/grep/ignored.txt`, `build.log`) are written by the test instead of committed: a file both tracked and ignored is a dirty tree to release-plz, and the first release PR committed their deletion. `cliff.toml` is gone: release-plz embeds git-cliff, `changelog_config` is deprecated, and the `[changelog]` table carries the same verbatim-commit body (on `raw_message`, because git-cliff reads `T0.1: title` as a conventional type and `message` would drop the id); the flat pre-release list in `CHANGELOG.md` is replaced by the header, and the first release PR regenerates `0.1.0` from history. Why: user request. Effect: git-only mode diffs each crate against its last tag with `cargo package --workspace`, whose verify build needs every crate self-contained, so the files the crates `include_str!` move into them (`crates/cox-protocol/default.toml`, `crates/cox-provider/prices.toml`, `crates/cox-ext/agents/`) and `config/` keeps the documented paths as symlinks; the workflow needs a `RELEASE_PLZ_TOKEN` repository secret (fine-grained PAT: contents and pull requests, read and write) because a PR or tag made with the default `GITHUB_TOKEN` triggers no other workflow, so neither CI on the release PR nor cargo-dist on the tag would run; task commits are not conventional, so every release is a patch bump — a minor or major release sets the workspace version by hand on `main` first, and release-plz never lowers a version.
- A18 §2, `AGENTS.md` — the human is the only author: no agent adds a `Co-Authored-By` trailer, a "Generated with …" line or itself as author to a commit, merge or PR, whatever its harness defaults to. Why: user request. Effect: a rule that never bends in `AGENTS.md`; existing history is left as it is.
- A19 `.github/dependabot.yml` — Dependabot: cargo for the workspace and `fuzz/` (its own workspace and lockfile), GitHub Actions, npm for `website/`; minor and patch bumps grouped into one PR per ecosystem, a major bump on its own. Why: user request. Effect: `dependencies_update` stays off in `release-plz.toml` (Dependabot owns bumps, release-plz owns releases); a merged bump touches only root files, so it never appears in the changelog and never forces a release on its own; `deny.toml` in CI still gates every bump.

- A20 §1.12, T2.4, T2.6, T0.5, T12.3 — P17 closes leftovers that sat in `done.md` as "Not done": `Session::resume` so `--resume`/`--continue` reuse the rollout id; TUI positional `PROMPT`; OpenAI Chat/Responses `stream_with_retry`; `cox doctor` prices-age; website copy matches the shipped binary. Why: user request to finish remaining work after every §3 task was moved to `done.md`. Effect: no new crates; OpenAI constructors keep their signatures (default `retry::Policy`).

- A21 §1.12, T12.3, T17.3 — P18: TUI `--resume`/`--continue`, `/clear`, and Hugo pages for tools/compat/ide/how-it-works. Why: user request to finish remaining work after P17. Effect: `--resume` on `Cli` is not `global`, so `cox run --resume` stays on `RunArgs`.
- A22 `.github/workflows/ci.yml`, `release-plz.yml` — the `dtolnay/rust-toolchain@<version>` pin names the *toolchain*, and `1.120.0` does not exist (CI failed downloading it), so both workflows pin `@1.97.1`, the version `mise.toml` already pins and `mise exec -- rustc --version` reports. Why: red CI on every push. Effect: no floating toolchain; bump the five pins together with `mise.toml` when Rust moves. The `revert-on-failure` job skips pushes touching `.github/` (least privilege instead of granting `workflows: write`).
- A23 §2, §3 P19 — per-task branches and one draft PR for the v0.2 scoping slice. Why: user request `work through the plan on a separate branch, one commit per task, and open a draft PR` (translated), which overrides A2 (`main`-only) for this slice only. Effect: work happens on branch `plan/v0.2-scoping`, one commit per task (`T19.1`–`T19.7`: each commit touches ≤ 3 files, ≤ 200 LOC, message `<task-id>: <title>`), pushed as a single draft PR into `main` (PR #24); A2 stays in force for everything outside P19. Each T19 task writes its phase-gate design doc (`docs/design/v0.2-<slug>.md`, Problem / The field / cox / Falsifiers / Review) and moves its `roadmap.md` v0.2 line into the P19 card; no runtime crate changes in this slice.
- A24 §3 P20 — ketch-model release for cox (T19.8, T20.1–T20.6). Why: user request `prepare a release in ketch and on GitHub like listrepo/ketch and listepo/rtok` (translated). Effect: work happens on branch `release/ketch-model`, one commit per task, PR #26 into `main`; release-plz only proposes (no tags), `release.yml` builds `cox-<target>.tar.xz` via `scripts/package.sh` and creates `v<version>` by publishing (tag iff release completed), `scripts/cask.sh` generates the Homebrew cask, `ketch.toml` + registry entry `cox/` make `ketch install cox` work. T20.6 repins the CI toolchain to `1.97.1` after Dependabot #16 broke it with nonexistent `1.120.0` (same class as A22).
- A25 §3 P21 — TypeSafe Jev as a decision model (T21.0 scope gate). Why: user request to restore and improve the Jev note that was lost in an uncommitted working-copy overwrite of `plan.md`. Effect: new phase P21 with one `open` scope-gate task T21.0 (`docs/design/v0.2-jev.md`, Problem / The field / cox / Falsifiers / Review), mirroring the P19 gate shape: design doc first, no `crates/` changes, no new §1.1 dependency until the doc fixes the boundary (Jev answers never bypass the permission engine; fail open like hooks/skills/MCP). Restores the lost facts in their correct form — Jev is TypeSafe's System One decision model (state + Choice/Score/Noul questions in, probabilities + confidence out, `POST /v1/systemone` in its own JSON format, Python/JS SDKs, no OpenAPI; LangChain middleware and Vercel AI Gateway integrations; keys via waitlist at `console.typesafe.ai`; docs index at `docs.typesafe.ai/llms.txt`) — and maps the candidate call sites (router pick, permission classification, compaction/memory salience, skill suggestion) to the cookbook patterns (intent routing, confidence-gated routing, skill suggestion, LLM guardrails).
- A26 `research.md` §8, `docs/design/improvement-plan-2026.md`, `ideas.md` — field survey of terminal coding agents (2026-09-22) and a proposed improvement plan. Why: user request to research what agent CLIs/TUIs ship in 2026, compare with cox and plan how to be more convenient and better-looking than the field. Effect: research §8 records the survey (four research agents, author-verified cox column and crate facts, ledger #29–36); the design doc holds nine proposed phases P22–P30 (trust fixes for dead config keys and the fixed-answer `ask_user`; terminal capabilities; themes and tool cards; message queue and `Shift+Tab`; checkpoints and `/rewind`; visible agents; context and cost visibility; `--plain`; lean profile and footprint) as task cards in the §2 format, with priorities, dependencies needing approval (§7 of the doc) and falsifiers; `ideas.md` lists the phases. No task is added to the §3 table or `todo.md`; no decision in §0 changes; nothing moves until the creator approves a phase.
- A27 §3 P22–P30, top table, `todo.md`, `ideas.md`, §3.0, §5 M6 — the improvement plan approved and moved into the plan (2026-09-22). Why: the creator approved the A26 proposal and asked for every task in `plan.md` with concrete step-by-step instructions and a complexity rating. Effect: 48 tasks total: 44 open and 4 done (T22.5, T26.1, T26.2, T27.3); the cards use the §2 format (Model, Depends, Size, Priority, Complexity, Goal, Files, numbered Steps, bash Check, Done when, Out of scope), and the same ids appear in the top table and `todo.md`; `ideas.md` keeps only the unapproved later gates; `docs/design/improvement-plan-2026.md` keeps the survey, principles, pitch, non-goals and falsifiers and points to §3 for the cards. Cards were corrected against the code before the move: `ask_user` already has `Answers::Surface` (T22.1 wires it), background agents are already concurrent (T9.2) so T27.1 is about `bash` tasks and `Ctrl+B`, `SessionStart`/`Notification` already exist in `HookEvent` (T22.3 fires them), `similar` is already a workspace dependency (T24.5). Four new dependencies still need approval before their task starts: ratatui `scrolling-regions` feature (T23.2), crossterm `osc52` feature (T23.4), `terminal-colorsaurus` (T22.6), `two-face` (T24.3); each card names it. No decision in §0 changes; §1.13 keymap rows and §1.2 protocol variants that a card adds (`Submission::UserShell`, `Rewind`, `Background`; `Event::Checkpoint`, `Rewound`) are amended in that task's commit.
- A28 §3 P22, T22.8 — `cox-mcp` `oauth_refresh_failure_is_a_warning` failed twice in loaded `cargo nextest run --workspace` runs (~5.5 s). Its assertion is about how an error is classified, but the whole connect ran under the bare 5 s handshake budget. Why: user request to find the real cause and make the test deterministic without weakening it. Effect: a test-only change. The test gets a connect budget a stall cannot reach; `connect_all`, the production budget and the sibling OAuth test are unchanged.
- A29 §3 P27, T27.2, T27.5 — the creator's answer to T27.2's open question: the `/agents` card is the narrow one (name, preset, tier, cost, elapsed, state from `TaskCreated`/`TaskCompleted` and the T16.1 presence records), not a new `Event::AgentProgress`. Why: user request (today). Effect: T27.2 closes without a protocol change — per-subagent model, tokens and last tool stay undone until that event exists; `Enter` on a card opening its rollout read-only is split into the new T27.5, since it needs `/agents` to become a navigable list instead of a static `Notice`.
- A30 `crates/cox-protocol/default.toml`, T22.4 — `tui.mouse` defaults to `false`. Why: the creator's decision after T22.4 made the key live. Mouse capture in the inline viewport takes the wheel from the terminal's own scrollback and plain text selection, and the key had been `true` only because nothing read it. Effect: `default.toml`, `TuiConfig::default`, `State::new` and `docs/config.md` say `false`; `tui.mouse = true` turns on the T22.4 wheel scrolling.
- A31 §3 P27, T27.4, T27.6 — T27.4's card asked for `/loop` (TUI) and `cox run --loop` (headless) in one ≤3-file task (`crates/cox-tui/src/commands.rs`, `crates/cox-tui/src/state.rs`, `crates/cox/src/run.rs`), but the headless half also needs `crates/cox/src/cli.rs` for its new `RunArgs` flags (`--loop`, `--max-iterations`) — a fourth source file, over the cap. Why: plan.md §2 ("if the Check cannot pass without exceeding the size limit, split the task"). Effect: T27.4 lands only the TUI `/loop` (`commands.rs` + `state.rs`, `docs/getting-started.md`); the headless counterpart is the new T27.6 (`cli.rs` + `run.rs`), depending on T27.4 for the shared interval grammar. No design change — same goal, same budget-cap idea (T27.6 reuses the core's existing `StopReason::Budget` rather than inventing a second cap), split only on file count.
- A32 `crates/cox-protocol/default.toml`, T22.4 — `tui.mouse` defaults to `true` again, which reverses A30. Why: the creator's later decision. Effect: `default.toml`, `TuiConfig::default`, `State::new` and `docs/config.md` say `true`; the terminal's own selection needs Shift/Option while cox runs, and `tui.mouse = false` gives it back.
- A33 §3 P22, P27, T22.9, T27.7 — the two parts of approved cards that did not fit their size limits become cards of their own: T22.9 (T22.4's click on a folded tool card unfolds it) and T27.7 (T27.4's `↻ <time>` status-line segment for an active `/loop`). Why: the creator asked for every remaining task that needs no creator input; both halves were already approved as part of T22.4 and T27.4. Effect: two rows in the top table and `todo.md`; no new dependency.
- A34 §3 P27, T27.5 — T27.5's card listed `crates/cox-tui/src/state.rs`, `crates/cox-tui/src/view.rs`, `crates/cox/src/resume.rs`, but the actual touch is `state.rs` + `view.rs` + `crates/cox-tui/tests/agents.rs` (the T27.2 snapshot test reads `/agents`'s old `Notice` cell and has to change now that it opens a modal) + `crates/cox/src/session.rs` (not `resume.rs`, which builds a turn-oriented `History` this overlay does not need — the poll loop wants the raw `Vec<Event>` `Store::rollout_read` already returns). A 3-files-only split was drafted (§6's earlier text) to land the `cox-tui` half and leave `session.rs` to a follow-up, but `session.rs`'s `match ask { Some(Ask::GitDiff) => …, None => break }` is exhaustive over `Option<Ask>`, so the compiler requires a `session.rs` edit the moment `Ask` grows `Rollout` — a stub costs the same one match arm as the real `Store::rollout_read` call, so the split would not have saved a file. Why: discovered mid-implementation, not planned; plan.md §2's split guidance assumed avoiding the file cost was possible, and it was not. Effect: T27.5 lands whole, 4 files instead of the usual 3 (state.rs, view.rs, tests/agents.rs, session.rs); no follow-up card.
- A35 §3 P30, T30.4, T30.3 — new card T30.4 (send `anthropic-workspace-id` from `ANTHROPIC_WORKSPACE_ID`) ahead of T30.3. Why: the creator's key is not scoped to a workspace, so every Anthropic call 400s; the creator chose teaching cox the header over issuing a workspace-scoped key. Effect: T30.3 step (3) runs after T30.4.
- A36 §3 P30, T30.5, T30.3 — new card T30.5 (price every provider call through a `Priced` decorator) ahead of T30.3. Why: T1.7's `ledger_row` was never wired into a production path, so every ledger row costs $0 and budgets never fire; found during T30.4's live check; the creator chose fixing it before the paid eval run. Effect: T30.3 step (3) runs after T30.5; costs recorded before this fix are $0 and stay so (history is append-only).
- A37 §3 P30, T30.6, T30.3 — new card T30.6 (the Anthropic stream emits `ToolUseEnd` on a tool block's `content_block_stop`) ahead of T30.3. Why: without it every Anthropic tool call is dropped; found by T30.3's first live task; the creator chose fixing it first. Effect: T30.3 step (3) runs after T30.6. `openai/chat.rs` never emits `ToolUseEnd` either; that is a separate, larger fix (interleaved calls by index) proposed to the creator, not part of T30.6.
- A38 §3 P30, T30.7–T30.9 — the eval scripts become a uv-managed Python package with tests (T30.7, T30.8), and the Terminal-Bench part of T30.3 becomes T30.9 (Harbor agent, colima, $1 budget). Why: the creator asked for the scripts to be a proper package with tests before TB; the old adapter could not run for real. Effect: T30.3 closes after T30.9; `just eval` runs through uv.
- A39 §3 P30, T30.10 — the Anthropic stream wire types are generated with typify from a curated JSON Schema subset of Anthropic's OpenAPI spec; the SSE → `ProviderEvent` mapping stays hand-written. Why: the creator chose typify-generated types over a hand-written `Value` walk or a full generated SDK (none exists for Rust that handles SSE). Effect: one build-time proc-macro dependency; D3 unchanged.
- A40 §0 D3, §3 P30, T30.11–T30.12 — D3 gains an order for where provider wire types come from: a maintained SDK's types, else typify over the vendor's vendored spec, else hand-written; transport, SSE mapping and the ledger stay ours; SDK code is a `wire` module inside the provider. Login stays API-key only: Anthropic forbids third-party Claude subscription login in writing, OpenAI documents nothing for ChatGPT login (R§4.3.1). Why: the creator asked providers to check for an SDK or a generatable spec before hand-writing, starting with the Claude Code and Codex replacements. Effect: `async-openai` (types only) and a vendored Anthropic spec enter the provider crate.
- A41 §3 P30, T30.13 — new card: cox, Harbor's `claude-code` and `terminus-2` on the same 12 Terminal-Bench 2.0 tasks with the same local model (`bonsai-27b` in LM Studio), as a baseline before the optimization and refactoring pass, repeated after it (roadmap). Why: T30.9's 3/3 covered 3 of the 4 `easy` tasks and cannot be compared with anything; the creator chose same-model agents on a local model (no API spend), 12 tasks × 1 attempt.
- A42 §3 P30, T30.14 — new card ahead of T30.13: the comparison runner becomes a tested `evals` module with registries of agents, providers and models (`cox-bench`). Why: the creator asked for a package/module supporting different providers, models and agents rather than a one-off script. Effect: T30.13 depends on T30.14 and runs through it.
- A43 §3 P30, T30.15–T30.16 — new cards: a built-in `lmstudio` provider whose chat loop runs over LM Studio's Anthropic-compatible `/v1/messages` through the existing Anthropic provider (T30.15), and LM Studio's native `/api/v1/models` and `models/load` for the loaded context length, capabilities and load on demand (T30.16), with hand-written types (D3/A40 step 3). Why: the creator asked for LM Studio's own API as a local provider; R§4.3.2 shows the native chat endpoint takes no custom tool schemas, so the native API serves model state and the chat stays on Messages.
- A44 §3 P30, T30.17 — new card: one model of providers, models, prices and effort; design first (D15), code only through cards the creator approves. Why: the creator's rule that provider, model, price and effort handling be as unified as possible, and the LM Studio provider (T30.15) should land in that shape.
- A45 §0 D1, §3 P30, T30.18 — new card: design a finer crate split (D1's ten crates are a floor, not a target). Why: the creator's rule that the project be split into crates as far as possible. Effect: D1 changes only through the amendment T30.18 proposes.
- A46 §3 P30, T30.15, T30.16 — approved by the creator: T30.17's result. Seven implementation cards U1–U7 (table in `docs/design/providers.md` § Target shape; evidence R§4.3.3): one key resolver, one `Transport` descriptor in every provider section, constructors over it, a pure `cox-models` catalog (context, max output, efforts, capabilities, price) replacing the `Caps` literals and `ADAPTIVE_THINKING_PREFIXES`, one per-wire effort map with `Effort::Medium`, and a `cox doctor` catalog/price row. Why: the creator's rule that provider, model, price and effort handling be as unified as possible. Effect: U1–U7 are cards T30.21–T30.27; T30.15 depends on T30.21–T30.23, T30.16 on T30.24–T30.25, T32.13–T32.15 on T30.21–T30.26; T30.13 is unaffected.
- A47 §0 D1, §3 P32 — approved by the creator ("create the tasks for crates.md"): T30.18's result. D1 becomes: "One Cargo workspace, one static binary. A module is its own crate when it alone uses a heavy or platform-gated dependency, is a trust guard, is a ≥ 500-LOC leaf, or is needed by another crate without the rest of its own (`docs/design/crates.md`); `crates/cox/tests/deps.rs` holds the graph. No WASM or dylib plugin host in v0.1." Seventeen new crates (27 in total), extracted by cards C1–C16 in the order in `docs/design/crates.md` (`cox-models` comes from T30.24, A46 U4); every card is a `git mv` plus a re-export at the old path, a `deps.rs` rule and the AGENTS.md layout row, with no logic change; moved lines do not count toward the 200-LOC limit. Why: the creator's rule that the project be split into crates as far as possible; evidence R§4.3.4. Effect: D1 reworded as above; the phase is P32, not P31, because the unmerged branch `t31-beta-mvp` already uses P31/T31.1–T31.5; C1–C16 are cards T32.1–T32.16 in the new phase P32; the provider wires (T32.13–T32.15) move after T30.21–T30.26 (A46 U1–U6).
- A48 §3 P30, AGENTS.md — new cards T30.19–T30.20 and a convention: a file no package manager fetches (a vendored API spec, a price or model table, any JSON/YAML data) is produced only by a saved, tested Python script that is re-run to update it; no hand download, no pasted rows. Why: the creator's rule. Effect: the Anthropic spec (T30.19) and the models.dev-derived `prices.toml` rows and `default.toml` model lists (T30.20) get their scripts; A46 U4's embedded catalog rows come from T30.20's script.
- A49 §3 P30, AGENTS.md — new card T30.28 and a convention, by the creator: tests never read the real OS keychain; they inject the lookup. Why: test runs prompted for the macOS login password and read the developer's real key. Effect: T30.28 runs after T30.23; the T30.13 keychain prompt stays with T30.13.
- A50 (renumbered from `t31-beta-mvp`'s own A35 — that branch's numbering was against a different, older `main`) §3 P31, §4 — beta readiness: an audit of the v0.1 definition of done found five gaps the code can close, and each became a task (T31.1–T31.5). Criterion 6 had eight `expect` calls on production paths (the three request-body builders, the Jev client, the CLI override tree, `cox config show`); criterion 5 had no test of the shipped `cox mcp` binary, and writing one showed the default selection advertised an `outline` tool that does not exist (an outline is `read` with `mode = "outline"`); the README quick start used a top-level `cox -p` that clap rejects; the `Command` doc still said most subcommands print `not implemented`. Why: user request — determine what the beta needs and do it in one branch. Effect: `cox mcp` serves `read`, `grep`, `glob` by default; the README and `Command` doc no longer lie about the CLI shape; the work landed on branch `t31-beta-mvp` rather than `main` because the user asked for one branch, and was cherry-picked onto `main` on 2026-09-26 after `main` had moved through T30.19–T30.28 in the meantime. Landing found two of the five tasks already overtaken: T30.23 (A46 U1) had independently made `JevProvider::with_key`/`new` take `&Transport` and return `Result<Self, ProviderError>`, with `session::provider_for` already propagating it — T31.2 is dropped as fully superseded, no code changed; T30.11–T30.12's typed `wire::CreateMessageParams`/`wire::CreateResponse` rewrite had already removed the `expect` this branch targeted from the Anthropic and OpenAI Responses builders, so T31.1 keeps only its `openai/chat.rs` hunk. T31.3–T31.5 landed unchanged. Not done here: the paid eval and cache-ratio measurement (T30.3) and the macOS signing secrets — the release workflow refuses an unsigned build by design, and that stays the creator's call.
- A51 §3 P30, AGENTS.md — new card T30.29, by the creator ("fix every keychain place so fakes are used"): `COX_KEYRING=off` switches the OS keyring off in the binary, and `.cargo/config.toml` sets it for every cargo-run process. Why: the T30.28 seams covered tests, but smoke runs of the rebuilt binary (`cargo run -- doctor`) still raised keychain prompts. Effect: A49's rule now covers dev runs too; T30.13's real runs set the key's env var. (A50 is taken by the T31 landing.)

- A52 §0 D1 and the "Deferred to v0.2+" line, §1.1, §1.15, §3 P33, `roadmap.md` — WASM plugin host, approved by the creator. It implements `docs/design/plugins.md`.
- A53 §0 "Deferred to v0.2+" line, §3 new P34 — Subagents, `todo.md`, `ideas.md` — subagent support, researched at the creator's request ("add subagent support: research it, add to the plan if it is not there"), and inter-agent communication, approved by the creator ("add support for communication between subagents, and between subagents and the main agent"). That second request is explicit approval for messaging including sibling ↔ sibling, so P34 is not gated behind `ideas.md`'s "agent teams / orchestration DSL" line the way a fuller orchestration feature would be. Why: `crates/cox-core/src/subagent.rs`/`tasks.rs`, `crates/cox-ext/src/agents.rs` and the `/agents` TUI overlay already implement a one-shot, structurally depth-1 `agent` tool with two hardcoded presets and a one-shot approval relay (`relay_approval`), but §1.11's own `agent` row already documents a named custom preset (`preset: "<name>"`) and a `tier` override that the code never got, no subagent-specific concurrency cap exists (only the generic `core.parallel_tools`), `ask_user` from a subagent carries no `Source` label, and nothing lets a parent follow up with a running or finished child or lets siblings exchange messages. Separately, the "Deferred to v0.2+" line still named "git worktree isolation for subagents" as undelivered even though it shipped as T27.3 (`subagent.rs`'s `isolation: "worktree"`, tested) — fixed in this same edit, no card for it. Effect: eleven new cards (T34.0–T34.10): a ≤ 1-page design doc for parent↔child and sibling messaging (T34.0, D15), reviewed by `think`, followed by its narrow implementation split across protocol types, core routing, the `send_message` tool and three surfaces (T34.4–T34.9, each ≤ 200 LOC / 3 files); three cards independent of the messaging design (T34.1 the custom-preset/`tier` wiring, T34.2 the concurrency cap, T34.3 the `Source`-labelled `ask_user` channel); and one optional visibility gate (T34.10). Every message is routed through the parent session as a `Submission`/`Event` (D2 pure state machine) — no side channel, no direct sibling socket. `ideas.md`'s "agent teams / orchestration DSL" line is removed as its own, still-unapproved idea: P34 is deliberately narrower than it — no `SendMessage`-as-a-tool with an injected sibling roster, no teammates, no split-pane processes, no plugin-provided agent definitions, no per-`AgentDef` permission-mode override, no `@mention` invocation; the last three are added to `ideas.md` instead, one line each. No decision in §0 changes beyond dropping the stale deferred-list line.
  - **The host.** An extism 1.30.0 host in the new crate `cox-plugin`, a pure ABI and manifest crate `cox-plugin-api` (re-exported as `cox_protocol::plugin`), and a separate guest cargo workspace `plugins/` (`cox-plugin-sdk` over extism-pdk 1.4.1, examples and templates).
  - **What a plugin can contribute**, each as a manifest capability the user approves per package digest: hooks, called methods, a context snapshot, event subscription, TUI status segments, a bottom panel or overlay, slash commands and keys under a leader, custom rendering of tool results and messages, model providers (declarative `chat`/`responses` sections, or the ABI `Provider`), catalog rows (a fill-only layer between built-in and config), MCP server declarations (stdio servers run under `sandbox::Policy`), and answers at the core's decision points (the `Advisor` trait; Jev is the first user).
  - **Why.** The creator decided it: runtime extism, the full contribution set above, capabilities approved on install and enable and re-asked on changed bytes or wider capabilities, and design and plan before code. This overrides the evidence gate in `extensions.md` and `v0.2-wasm.md` (falsifier 1: three requests MCP cannot serve), which is recorded as superseded, not refuted.
  - **Effect on §0.**
    - D1's last sentence "No WASM or dylib plugin host in v0.1." becomes "No dylib plugin host. One WASM plugin host (extism) from v0.2: `docs/design/plugins.md`; it reaches the core only through traits in `cox-protocol`."
    - "WASM plugin host (extism 1.30)" leaves the Deferred-to-v0.2+ line.
    - D2, D6(e), D9 and D14 are unchanged; the design keeps each (plugins.md §§4–7, 10).
  - **Effect on §1.1.** Two crate rows (`cox-plugin-api`, `cox-plugin`) and the dependency-direction line.
  - **Effect on §1.15.** Three invariants: 15 `plugin_tool_specs_frozen_within_session`, 16 `plugin_grant_reasked_on_digest_or_widening`, 17 `plugin_failure_is_skipped_not_fatal`; §4's definition of done now names seventeen invariants, not fourteen.
  - **Effect on `roadmap.md`.** The v0.2 line "WASM plugins (extism)" moves into P33 and is deleted from the roadmap; two new roadmap lines take its place (publishing the SDK once the ABI is stable, and installing from git/URL).
  - **Effect on AGENTS.md.** Layout rows for the two crates and `plugins/`. The trust list says a plugin host function never replaces one of the four guards.
  - **Effect on other tasks.**
    - T33.19 wraps plugin-shipped MCP stdio servers with the sandbox after T32.3 (`cox-sandbox`).
    - T33.16 extends `Catalog::load` from T30.24.
    - T33.18 reuses `resolve_key` from T30.21.
  - **Creator decisions, 2026-09-26** (resolving this amendment's open questions and the ones the Jev use case raised, T33.40; recorded in full in `docs/design/plugins.md` §14):
    1. SDK/API publishing (`cox-plugin-api`, `cox-plugin-sdk`) waits for a stable ABI; it is a `roadmap.md` item, not a P33 card.
    2. Dart stays the documented MCP-stdio-server exception (PL§13); the re-check spike (T33.37) stays in the plan.
    3. Kotlin: T33.35 spikes first; if it passes, cox keeps its own thin PDK (T33.36) and turns on extism's `wasmtime-exceptions` feature only if the spike needs it.
    4. Every MCP stdio server, not only a plugin's, runs under `sandbox::Policy`, with a per-server opt-out in config — its own card, T33.42, not folded into T33.19.
    5. `route`: a plugin may only downgrade the tier (D5 holds); the core offers a downgrade only when its own cost estimate predicts a saving (T33.40.8, from the Jev research R§4.3.6 J§5.2).
    6. Install sources in v1 stay local-folder-only (PL§1); git/URL sources move to `roadmap.md`.
    7. CI gets a separate `plugin-examples` job (go, tinygo, java, gradle, kotlin, dart), as PL§13 already specified.
    8. The built-in `[providers.typesafe]` client (`crates/cox-provider/src/jev.rs`) leaves the core once the Jev plugin reaches parity — the tombstone config type, fail-open notice and removal cards are T33.40.12–T33.40.16.
    9. Jev evals: only E1 (`risk`, Jev only, capped at $0.10, T33.40.7) is approved to run now. E2 (`route`, real Anthropic plus Jev, capped at $3, T33.40.10) stays in the plan but needs the creator's explicit go-ahead before each run.
    10. `approve_hint` becomes warning-only: a plugin may add a caution note, never say a call "looks safe" (monotone like `risk`, T33.21).
    11. The `risk` advisor is enabled only by an explicit line in `[plugins.decide]`, never automatically on install; the grant dialog states exactly what data leaves the machine.
    12. T32.15 (`cox-provider-jev`) is dropped: its table row and card move to `done.md` as "Status: dropped 2026-09-26" with the reason, and it leaves `todo.md`. After parity, `jev.rs` is deleted outright (T33.40.12), not extracted into a crate.
    13. The Jev research's ABI fix (T33.40.1): `cox_decide` returns either an `Advice` or a `ModelCall`, which the host runs against the plugin's own provider through the budget gate and ledger before calling `cox_decide_resume`; `cox_http` to a provider host is allowed only inside `cox_provider_stream`; `Question` is batched. `docs/design/plugins.md` §4 and its manifest example (§2) are updated, and the example provider is named `typesafe`, not `jev` (the plugin id stays `jev`).
    14. The release ships no prebuilt Jev plugin archive; users build it from `plugins/jev` (`just plugin jev`) and `cox plugin install <dir>`.
  - **Not decided further:** anything not listed above and not in `docs/design/plugins.md` §14 stays open for a later amendment.

- A54 §3 new P35 — External agents from plugins (Cursor first), `todo.md`, `ideas.md`, `docs/design/plugins.md` §10 — Cursor as a plugin, researched at the creator's request (`research.md` §4.3.8, inserted after P34's §4.3.7). The research found Cursor has no chat/completions endpoint (the Cloud Agents API only creates and drives durable, autonomous "Cloud Agent" runs, R§4.3.8), so it cannot be a `Provider` the way T30.15 wired LM Studio; the creator resolved the resulting question — "is Cursor still wanted as a provider?" — by deciding it is not: **"Cursor has no chat or completions API, so it is not a model provider. Add Cursor as a plugin that drives the Cursor CLI `agent` in its two official headless modes: `agent -p --output-format stream-json` and `agent acp` (ACP server over stdio, JSON-RPC 2.0)."** The creator further ruled, as a hard requirement rather than a preference: **"Only official paths: the dashboard-issued API key (env var such as `CURSOR_API_KEY`, resolved like other keys, never read from tests' real keychain) and the official CLI/ACP. Never the desktop session, and never the reverse-engineered proxies."** Why: an unauthenticated survey of Cursor's eight documented programmatic surfaces (`cursor.com/docs/api`) found the CLI's `agent -p --output-format stream-json` and `agent acp` are the only ones that are (a) officially documented, (b) driven by an issued API key rather than the desktop session, and (c) shaped like something cox already knows how to consume — an external agent process, the same relationship D4 already gives Claude Code and Codex, not a model completions wire. Effect: eleven new cards (T35.0–T35.10) in a new phase P35, gated on P33's plugin-loading/grant/sandbox path (T33.6, T33.19, T33.42) and P34's custom-preset and messaging path (T34.1, T34.5) — a new plugin manifest capability `[[external_agents]]` (T35.1), a host-only spawner under the same sandbox and grant machinery as a plugin's MCP stdio server (T35.2), an ACP client adapter reusing `crates/cox-acp`'s existing `agent-client-protocol` dependency (T35.3), a host-side `stream-json` line mapper (T35.4, chosen over a WASM guest export — EA§5), wiring the granted entry into the `agent` tool's preset resolution and P34's message routing (T35.5), the Cursor plugin package itself (T35.6), an offline e2e against a fake `agent` binary replaying fixtures recorded by a `scripts/vendor` script from the documented event shapes (T35.7, no live Cursor, no key), `cox doctor` reporting (T35.8), a user guide (T35.9), and an optional live check gated on the creator's own key (T35.10, same shape as T33.40.17). `docs/design/plugins.md` §10's "No bypass" line is updated to name external-agent CLI processes alongside MCP stdio servers as the only two kinds of process a plugin brings, both sandboxed the same way, with a forward pointer to `docs/design/external-agents.md` (T35.0). No `§0` decision changes; D1's plugin sentence already covers "an in-process WASM host … reaches the core only through traits" and this phase adds no exception to it, since the process itself is always host-spawned, never guest-spawned. `ideas.md` gains one new, still-unapproved line: the Cloud Agents API (`api.cursor.com`) as a possible background-task backend, kept separate from this phase because it would be a different shape entirely (durable server-side runs, not a local subprocess) and was not part of the creator's decision above.
- A55 §1.1 `cox-plugin` row, §3 P33 (new T33.43; T33.14 depends on it), `docs/design/plugins.md` PL§11–12, `deny.toml` — T33.3 fired two PL§12 falsifiers; the creator decided both on 2026-09-26. (1) Size: linking extism grows `cox` by 16.8 MiB (51.2 → 68.0 MiB), over the 10 MiB budget, and almost all of it is cranelift and wasmtime. The budget becomes 20 MiB, and a `plugins` cargo feature on `crates/cox` (on by default) gives a slim build with no WASM runtime, enforced by `slim_build_has_no_wasm_runtime`. (2) Advisories: extism 1.30.0 pins wasmtime 43, which has RUSTSEC-2026-0222 and RUSTSEC-2026-0269 with no fix on the 43 line. The creator chose "ignore with a deadline, WASI off". Both are in `deny.toml` with reasons and a 2026-12-31 review date; WASI stays off, so the preopens in T33.14 wait for T33.43. extism cannot share one engine across plugins (`CompiledPlugin::new` builds its own), and 0222 needs the embedder to move objects between engines, which cox never does (research.md P39). Also: extism 1.30 does not build with `default-features = false` alone, which is why `wasmtime` is declared directly (research.md P38).

## 7. Risk register

| # | Risk | Signal | Mitigation | Task |
|---|------|--------|------------|------|
| R1 | Anthropic wire format changes (beta headers, `fallbacks`, effort names) break T1.1 | contract tests fail after re-recording | own provider layer isolates it to one file; cassettes re-recorded with `cox record`; prices/features carry `verified_on` | T1.1, T1.5, T1.7 |
| R2 | Cache hit rate stays low because instruction files or tool lists change mid-session | `cox stats --cache` shows repeated misses | breakpoint layout §1.9; discovered tools appended not reordered; diagnostics name the byte | T2.3, T8.3 |
| R3 | Sandbox blocks legitimate builds (network for `cargo fetch`, writes to `~/.cargo`) | users switch to `danger-full-access` | `writable` extras and `network` per project; `on-failure` policy asks instead of failing; doctor explains | T4.1–T4.3 |
| R4 | tree-sitter grammar/version churn (0.25 vs 0.27) | build breaks on update | pin grammars to a tested set; outline has a regex fallback | T3.2, T3.7 |
| R5 | ratatui inline viewport glitches on some terminals (tmux, Windows Terminal) | scrollback corruption reports | `tui.inline = false` falls back to alternate screen; PTY e2e covers both | T5.1, T5.8 |
| R6 | Hook or MCP server hangs the turn | turns stall | timeouts, process-group kill, fail open | T7.4, T7.6 |
| R7 | Bash classifier misses a destructive command | a destructive command runs without asking | classifier is an allowlist for `ReadOnly` (unknown → `Exec` → ask); sandbox is the second guard; fuzz the parser | T3.7, T12.4 |
| R8 | Task size limits force half-finished features | many §6 amendments | split by design at planning time; a phase gate reviews before the next phase starts | §2 |
| R9 | Third-party prices and thresholds in research were unverifiable | ledger cost wrong | `prices.toml` verified from official pages before the ledger goes live; doctor warns when stale | T1.7 |


---

## Note 2026-09-17 — testing library candidates

Shared catalog: [`listepo/rust.md`](../../rust.md) → *Testing candidates*.
Do not auto-add to workspace `Cargo.toml`.

Fits for cox (1–3):

1. `mockall` — Provider / Tool / MCP trait unit mocks (HTTP stays on `wiremock`).
2. `tokio-test` — async helpers for core loop / provider stream unit tests.
3. `serial_test` — only if P16 concurrent-session or global-env tests cannot
   isolate with temp roots (prefer isolation first).

Already covered: `assert_cmd`, `assert_fs`, `insta`, `predicates`, `pretty_assertions`,
`proptest`, `rstest`, `tempfile`, `wiremock`, `libfuzzer-sys` (`fuzz/`). Skip
`bolero`/`honggfuzz` unless fuzz gaps beyond libfuzzer; `vfs` optional for
tools FS unit tests (compare with rtok T56 pattern); `testcontainers` YAGNI
unless Docker e2e is required.
