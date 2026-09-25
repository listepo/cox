# cox — research: how the terminal coding agents are built, and what cox takes from each

Date: 2026-09-02. Method: six parallel low-cost research agents (Haiku) with web access, one adversarial fact-check agent, plus direct verification of crate versions (crates.io API) and Codex's workspace manifest by the author. Every claim carries a confidence tag; §6 is the fact-check ledger; §7 lists what is still unverified. Cross-references from `plan.md` decisions (D1–D16) point here as R§n.

## 1. OpenAI Codex CLI (`codex-rs`) — the closest existing Rust TUI agent

### 1.1 Shape
Rust workspace with 160+ crates, edition 2024, release `rust-v0.152.0` (2026-09-01). Domains: core/protocol/app-server, TUI, exec (headless), MCP client + server, sandbox (`linux-sandbox`, Seatbelt), providers (OpenAI, Bedrock, Ollama, LM Studio), extensions (`ext/{agent,mcp,skills,memories,web-search}`), 20+ `utils/*`. [high — repo tree]

### 1.2 Protocol: Submission Queue / Event Queue
The core is driven by a submission queue and emits an event queue; the TUI, `codex exec`, the app-server (JSON-RPC 2.0 over stdio/WebSocket/Unix socket for IDE extensions) and the MCP server are all consumers. Hierarchy `Thread → Turn → Item`, item deltas streamed, bounded queues with an overload error (`-32001`), queued submissions with stable ids that auto-start when the thread is idle. [high — app-server README] → cox D2, D11.

Design response: `docs/design/protocol.md`.

### 1.3 Dependencies (read from `codex-rs/Cargo.toml` on 2026-09-02, not from memory)
| Concern | Codex uses | cox verdict |
|---|---|---|
| TUI | ratatui 0.30.2 (default-features off), crossterm 0.29 (OpenAI fork), ratatui-macros 0.7, pulldown-cmark 0.10, syntect 5, image 0.25, arboard 3 | same, unforked crossterm |
| async / net | tokio 1, tokio-util, tokio-stream, reqwest 0.12, **eventsource-stream 0.2.3**, tokio-tungstenite 0.28, axum 0.8, tonic 0.14 | same minus axum/tonic (no app-server in v0.1) |
| MCP | rmcp =3.1.3 | rmcp 3.2 |
| storage | sqlx 0.9 (SQLite), JSONL rollouts in `~/.codex/sessions` | Diesel 2.2 on bundled SQLite (sync ORM; hooks and tests need no runtime; plan A1) + JSONL |
| diff / patch | diffy 0.4.2, similar 2.7 | diffy 0.5, similar 3.2 |
| code | tree-sitter 0.25.10, tree-sitter-bash 0.25, tree-sitter-powershell (command classification), nucleo (git), ignore 0.4.23 | same idea; tree-sitter-bash for the permission engine |
| sandbox | landlock 0.4.4, seccompiler 0.5, bubblewrap wrapper, Seatbelt via `sandbox-exec` | same |
| pty | portable-pty 0.9, vt100 0.16 (tests) | same |
| config | toml 0.9.5, toml_edit 0.24, dirs 6, keyring 3.6, schemars 0.8 | toml 1.1, toml_edit 0.25, figment, directories 6, keyring 4, schemars 1 |
| observability | tracing 0.1.44, tracing-subscriber 0.3.22, opentelemetry 0.31 (+otlp, semconv) | tracing; OTel behind a feature |
| tests | insta 1.46, pretty_assertions 1.4, wiremock 0.6, assert_cmd 2, predicates 3, tempfile 3.23, tokio-test | same |

Takeaway: the "eventsource-stream is stale" verdict from the crate survey (§4.5) is wrong in practice — the crate is small, finished, and shipped by Codex; cox uses it (D3).

### 1.4 Sandbox
Linux: bubblewrap if on `PATH` (`--unshare-user --unshare-pid`, read-only root, network namespace when restricted), `PR_SET_NO_NEW_PRIVS`, seccomp; fallback Landlock + mounts (`features.use_legacy_landlock`). `.git`, resolved `gitdir:` and `.codex` are re-applied read-only inside writable roots. macOS: `sandbox-exec` with generated Seatbelt profiles, `SandboxPolicy.network_access`. Sandbox modes read-only / workspace-write / danger-full-access; approval policies untrusted / on-request / on-failure / never. [high — linux-sandbox README, issues #11210, #6828] → cox D7.

### 1.5 Storage, config, context
`~/.codex/config.toml` with profiles, `[features]` gates (hooks, memories), `[sandbox]`, `[hooks]` (PreToolUse, PostToolUse, PreCompact, SessionStart, UserPromptSubmit, Stop), `model_context_window` / `model_max_output_tokens` (a known bug: ignored on some models, issue #19185). Sessions as JSONL rollouts plus SQLx index. Compaction and prompt-caching strategy are not documented. [med] → cox D9, D13; cox documents its compaction (D6f).

### 1.6 TUI and tests
Ratatui history cells, streaming markdown (pulldown-cmark → spans, syntect), diff renderer, composer with `@` file mentions and slash commands, transcript overlay, vim search (`/`, `?`, `n`/`N`) added in 0.152. Tests: `insta` snapshots through `TestBackend`; PTY tests with `portable-pty` + `vt100`; `wiremock` for the API. [high for crates; med for structure] → cox D10, D12.

### 1.7 Known weaknesses
`apply_patch` ENOENT on Windows (#17240); context-window config ignored (#19185); Desktop SIGKILL on update (#30359); "hangs and ignores instructions" (#38124); compaction opaque; `config.toml` silent failures. [high — issue tracker]

## 2. Competitors

### 2.1 Claude Code and GitHub Copilot CLI
| | Claude Code | Copilot CLI |
|---|---|---|
| loop | classic `while tool_use`, parallel tool calls, foreground/background subagents, `/loop`, task chips | turn-based with preview-before-execute approval gates [med] |
| tools | Read/Edit/Write, Bash, Grep/Glob, WebFetch/WebSearch, Agent, Todo, ToolSearch (deferred tool schemas) | repo browse, shell, LSP hover/goto-def, GitHub API; 128-tool cap per request |
| permissions | modes default/auto/plan/bypass; `allow`/`ask`/`deny` rules `Tool(pattern)` in `settings.json`, deny wins; sandbox = Seatbelt (macOS), bubblewrap + socat (Linux/WSL2) [high, ledger #11] | org-admin policies; no per-tool modes documented |
| extensibility | hooks (31 events, ledger #12), MCP (stdio, HTTP, OAuth), slash commands, skills (`SKILL.md`), plugins/marketplaces, subagent files, output styles | `.agent.md` custom agents, MCP (stdio/HTTP), `/lsp`, `/experimental` |
| memory / context | `CLAUDE.md` hierarchy, auto-memory dir per project, `--continue`/`--resume`, `/compact [focus]`, instructions reloaded after compaction | `copilot-instructions.md`; memory undocumented |
| UI | React + Ink; vim mode; `keybindings.json`; transcript viewer `Ctrl+O`; status line | new TUI GA June 2026: themes, narrow-terminal layout |
| headless | `-p`, `--output-format text/json/stream-json`; Agent SDK spawns the CLI | `-p`; `--headless --port` server + TypeScript SDK |
| routing / cost | delegates to Haiku for cheap jobs (the most-cited complaint: silent, only visible in verbose logs); `/cost`; `/model` | auto model selection by task/health/cost, 10 % credit discount for auto — praised because explicit |
| top complaints | April-2026 quality regression (three overlapping bugs); silent Haiku delegation; co-author trailer; over-engineered output | Node OOM after ~37 min (leaked libuv handles); auth/SSO failures; PowerShell constrained mode; wrong model id sent to custom endpoints |

Sources: code.claude.com docs (permissions, sandboxing, hooks, memory, interactive-mode), github.com/github/copilot-cli, GitHub changelog 2026-01-14 / 2026-02-25 / 2026-06-23 / 2026-07-01. Report A's "default model Claude 3.5 Sonnet" and "MCP 1.0" lines were stale and are dropped.

### 2.2 Pi, OpenCode, Crush, Goose, Gemini CLI, aider, and the rest
| Agent | Stack | What is distinctive | Weak spot |
|---|---|---|---|
| Pi (badlogic/pi-mono) | TypeScript monorepo: `pi-coding-agent`, `pi-agent-core`, `pi-ai`, custom `pi-tui` (differential rendering) | deliberately minimal: read/write/edit/bash only, no MCP, extensions as TypeScript; unified multi-provider API; sessions shareable to Hugging Face; isolation by container (Docker, micro-VM) rather than a permission model | no built-in access control; OpenRouter cost tracking, image rendering, Windows install issues [high — repo] |
| OpenCode (sst → anomalyco) | TypeScript client/server, TUI + desktop + web | Build (read-write) and Plan (read-only) agents switched with Tab; general subagent; LSP integration; MCP servers; share links; 200 k+ stars | 4 k+ open issues: desktop GPU crashes, timeout config ignored, per-subdirectory project sprawl, subagent progress invisible to integrations [high — issues; MCP/LSP from docs] |
| Crush (charmbracelet) | Go, Charm libs | MCP with stdio/http/sse, per-server timeouts and disabled tools, dynamic OAuth client registration; LSP; multi-session per project; XDG config | provider config gotchas, hard-coded timeouts, silent model fallback in headless [high — README/issues] |
| Goose (block) | Rust core, CLI + desktop + API | 15+ providers, 70+ MCP "extensions", recipes, ACP client; Linux Foundation (AAIF); evals under `evals/harbor`, `deny.toml` | session-state bugs after editing history, UI freezes; desktop is Electron [high — repo] |
| Gemini CLI (google) | TypeScript, React/Ink | checkpointing, policy engine, hooks, extensions, `GEMINI.md`, Google Search grounding, `-p` with json/ndjson output | model picker gaps, auth hangs from subdirectories, 590+ open issues [high — repo] |
| aider | Python | repo map (tree-sitter tags + PageRank, token-budgeted), edit formats (whole/diff/udiff), architect/editor two-model split, `--weak-model` for commits and summaries, auto-commit, polyglot benchmark | credential exposure in child commands, markdown fence parsing bugs, maintenance-status questions [high — repo/issues] |
| Cline | TS | Kanban task board running agents in parallel with auto-commit | — |
| Warp | Rust (closed) | "Oz" agents triage → spec → implement → review | — |
| Qwen Code / Kimi Code / Mistral Vibe | TS / TS / Python | multi-protocol provider switching at runtime; open-model first | — |
| Kilo / Roo | TS | "team of agents" modes; Kilo #1 on OpenRouter by volume | — |
| Amp, Cursor CLI, Factory Droid | closed | minimal terminal agent; IDE-bound CLI; enterprise droids | no public source |

### 2.3 Best-of-breed, and the gaps
| Feature | Who does it best | Why | cox |
|---|---|---|---|
| decoupled core / many surfaces | Codex | SQ/EQ protocol, one core → TUI, exec, app-server, MCP | D2, D11 |
| permission rules + sandbox | Claude Code + Codex | rule syntax that is readable (`Bash(npm run test:*)`) plus a real kernel sandbox with a small mode vocabulary | D7, T2.2 |
| extensibility without a plugin ABI | Claude Code | hooks (31 events), skills, commands, subagent files, MCP, plugins as bundles of those | D1, D4, P7 |
| explicit model routing | Copilot CLI (auto), aider (`weak_model`) | routing is visible and priced; Claude Code's hidden Haiku delegation is the counter-example | D5 |
| token-frugal tools | Claude Code (ToolSearch, line-range Read), aider (repo map) | deferred schemas; outlines instead of files | D6c–d |
| diff-shaped edits | Codex (V4A `apply_patch`), Claude Code (`str_replace`) | both trained-in formats | D8 |
| minimalism / hackability | Pi | four tools, extensions in the host language, session sharing | keeps the core eight tools small |
| plan vs build modes | OpenCode | one key toggles a read-only agent | permission mode `plan` (T2.2) |
| MCP client depth | Crush, Goose | OAuth registration, per-server timeouts/disabled tools; 70+ extensions | T7.6 |
| testing an agent | Codex, Goose | insta + TestBackend + vt100 PTY; `evals/` directory in-repo | D12, P12 |
| headless / SDK | Claude Code (`stream-json`), Copilot (`--headless --port`) | scriptable event stream | T6.1 |
| editor integration | ACP (Zed, JetBrains, neovim), Goose as ACP client | one protocol instead of one extension per IDE | T11.1 |

Nobody does well: (1) showing cache hit/miss and *why* a cache broke (D6, T8.3); (2) lossless truncation with a retrieval handle instead of silent cuts (D6a, T2.5); (3) refusing identical re-reads (D6b, T2.6); (4) a documented, append-only compaction that keeps the cache (D6f, T8.1); (5) a budget that stops the session (T2.7); (6) an offline, model-free regression suite that replays the agent's own event log (D12). The competitor-survey agent (C, 15 tools, ~75 lookups) adds three gaps cox should also close: no agent shows *why* it chose a tool and what context it passed (cox: the rollout JSONL plus `cox stats --turn`); none falls back to a local model when offline (cox: `cheap` tier may be Ollama, T9.1); none offers undo without git (cox: the archive keeps pre-edit file contents, T3.4). Its aggregate figures ("83 % MCP adoption", "30–50 % savings from architect/editor") are unsourced and not used.

## 3. Specifications cox implements

| Spec | Version / date | What it costs to implement | Verdict |
|---|---|---|---|
| MCP | revision 2026-07-28 (HTTP+SSE deprecated since 2025-03-26, reclassified Deprecated in 2026-07-28; Streamable HTTP + stdio current; OAuth with Client ID Metadata Documents; registry at registry.modelcontextprotocol.io) [high, ledger #3–4] | client via rmcp 3.2 (`client`, `server`, `auth` features); `.mcp.json` discovery; tool namespacing | must (T7.6, T6.2) |
| ACP (Agent Client Protocol) | JSON-RPC over stdio; Zed since Aug 2025, JetBrains since 2025-10-06 (ledger #14), public agent registry; crate `agent-client-protocol` 2.0.0 (2026-07-23) [high] | ~300 LOC over the event stream | must for IDE reach (T11.1) |
| AGENTS.md | plain markdown at repo root and above; no frontmatter; read by 20+ agents; AAIF/Linux Foundation [high] | trivial | must (T7.1) |
| Agent Skills | `SKILL.md` frontmatter `name`, `description`, optional `license`, `allowed-tools`, `metadata`, `compatibility`; progressive disclosure; opened Dec 2025 (agentskills.io) [high] | frontmatter parser + lazy body | must (T7.2) |
| Claude Code surfaces | `settings.json` permission rules, hooks JSON protocol (stdin JSON, stdout JSON, exit 2 blocks), `.claude/commands/*.md`, `.claude/agents/*.md`, `stream-json` [high — code.claude.com] | import layer | must (T7.3–T7.5, T6.1) |
| Codex surfaces | `~/.codex/config.toml`; V4A patch grammar (`*** Begin Patch`, Add/Update/Delete/Move, `@@` context, progressive matching) [med — community write-up + repo] | V4A parser only; config not imported | V4A yes (T3.5), config no |
| Anthropic Messages API | streaming, tool use, `cache_control` (min cacheable prefix 512 tokens on Fable 5.1/Opus 5/Sonnet 5, 4 096 on Haiku 4.5; ledger #21), adaptive thinking, `output_config.effort`, `fallbacks`, `count_tokens`, server tools `web_search_20260209`/`web_fetch_20260209`, mid-conversation `system` messages (Opus 5/Fable) [high — Claude API reference] | own client (D3) | must (T1.1–T1.2) |
| OpenAI Responses + Chat Completions | Responses for OpenAI models; Chat Completions is the common subset for Ollama, vLLM, LM Studio, llama.cpp, OpenRouter, DeepSeek [high] | own client | must (T1.3–T1.4) |
| OpenTelemetry GenAI semconv | `gen_ai.operation.name`, `gen_ai.provider.name`, `gen_ai.request.model`, `gen_ai.usage.input_tokens/output_tokens`; experimental [high] | tracing layer + feature flag | should (D16) |
| A2A | v1.0.1 (May 2026), agent cards | — | later |
| Benchmarks | SWE-bench Verified; Terminal-Bench (site now shows 4.0; 2.0 task count unverified, ledger #16); aider polyglot (225 exercises) | adapter | T12.1 |

### ACP stdio smoke (T11.2, 2026-09-04)

Recorded against the debug binary with a scratch `COX_HOME` (no network, dummy key only for `session/new`):

```
→ {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}
← {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{"loadSession":true,…},"authMethods":[]}}
→ {"jsonrpc":"2.0","id":2,"method":"authenticate","params":{"methodId":"none"}}
← {"jsonrpc":"2.0","id":2,"result":{}}
→ {"jsonrpc":"2.0","id":3,"method":"session/new","params":{"cwd":"/tmp","mcpServers":[]}}
← {"jsonrpc":"2.0","id":3,"result":{"sessionId":"01M1MJRKRG6GPHGPWYRX5QAT4H"}}
→ {"jsonrpc":"2.0","id":4,"method":"session/load","params":{"cwd":"/tmp","sessionId":"01ARZ3NDEKTSV4RRFFQ69G5FAA"}}
← {"jsonrpc":"2.0","id":4,"error":{"code":-32602,"message":"Invalid params",…}}
```

Notes: `session/new` without `mcpServers` is rejected by schema validation
(`DefaultOnError` does not apply to that field); unknown `session/load`
ids are explicit errors, never empty sessions. Full prompt/permission
round-trips run in-process in `crates/cox-acp/tests/conformance.rs`.

## 4. Token economy, routing, provider layer, crates

### 4.1 Prompt caching (the largest lever)
Prefix match over tools → system → messages; any byte change invalidates everything after it. Anthropic: up to 4 breakpoints, 5-minute default TTL, 1-hour TTL option, cache write 1.25× input, cache read 0.1× (Fable 5.1 cache read $0.25/MTok); model-scoped, so a routing cascade forfeits reuse across models. OpenAI: automatic prefix caching. Anthropic's own guidance: measure the capable model at lower `effort` before building a cascade. [high — Claude API reference + prompt-caching docs; ledger #21] → D5, D6e.

### 4.2 Compaction and truncation
Claude Code auto-compacts near the window and reloads instruction files after; `/compact [focus]`; older tool results are "microcompacted". Codex compaction undocumented. Third-party numbers ("Claude Code auto at 250–300 k", "500–2 000 tokens per tool result", "335 k → 169 k") were **not verifiable** (ledger #9–10) and are not used as design inputs; cox measures its own (T8.5). Techniques that are verifiable by construction: head/tail truncation with the full output on disk (cox D6a), replacing old tool results with pointers (D6f microcompact), search-before-read and line-range reads, structural outlines (tree-sitter), deferred tool schemas (Claude Code ToolSearch; Anthropic `tool_search_tool_*` server tools), diff-only edits. [high for mechanisms, low for third-party effect sizes]

### 4.3 Why an own provider layer (D3)
Candidate crates: rig-core 0.42 (multi-provider, opinionated), genai 0.7-beta, async-openai 0.41, community `anthropic` 0.0.8 (2024, unofficial, stale — ledger #20). What decides cost and correctness in 2026 is wire-level: `cache_control` placement, thinking blocks replayed unchanged on the same model, `effort`, `fallbacks`, `stop_details`, server tools, per-message system blocks. None of the frameworks track all of these, and each provider is ~500 LOC. Codex hand-rolls its client and uses `eventsource-stream` for SSE (§1.3). Verdict: own layer, `eventsource-stream` for SSE, `wiremock` + recorded `.sse` fixtures for tests.

### 4.3.1 SDKs, specs and logins for the Claude Code and Codex providers (checked 2026-09-25)
Question: before hand-writing a provider, is there (1) a maintained Rust SDK, else (2) a machine-readable spec to generate from? And can cox log in the way Claude Code and Codex do (subscription OAuth)? Every row was checked against the primary source on 2026-09-25 unless marked.

| Item | Finding | Source |
|---|---|---|
| Official Rust SDK, Anthropic | none; official SDKs are Python, TypeScript, Go, Java, Ruby, C#, PHP | https://github.com/anthropics (org repo list) |
| Official Rust SDK, OpenAI | none | https://github.com/openai (org repo list) |
| `async-openai` | 0.42.0, released 2026-09-09; hand-written "based on OpenAI OpenAPI spec", struct and field names copied from it; typed Responses API incl. streaming events (`responses` feature → `response-types`); custom base URL; raw JSON escape hatches (`extra_body`, BYOT, `serde_json::Value`) | https://crates.io/api/v1/crates/async-openai ; https://github.com/64bit/async-openai (README, CONTRIBUTING.md) |
| Community Anthropic crates | none maintained with cache_control, thinking replay and server tools; `anthropic` 0.0.8 is stale (ledger #20) | crates.io search; §4.3 |
| Codex model client (`codex-rs`) | crates `codex-api` (Responses request + SSE), `codex-client` (transport, retry), `codex-protocol` (wire types, Rust → TS via `ts-rs`), `codex-model-provider-info`, `codex-login`; hand-written; none published on crates.io; Apache-2.0; 180+ workspace crates, no stability contract | https://github.com/openai/codex `codex-rs/Cargo.toml`, `codex-rs/codex-api/src/endpoint/responses.rs`; https://crates.io/api/v1/crates?q=codex |
| Codex endpoints | `https://api.openai.com/v1/responses` (API key) and `https://chatgpt.com/backend-api/codex/responses` (ChatGPT login), same request code, SSE only | `codex-rs/codex-api/src/provider.rs`; `codex-rs/model-provider-info/src/lib.rs:77` |
| Codex ChatGPT login | OAuth PKCE (S256), token endpoint `https://auth.openai.com/oauth/token`, Codex's own client id, local callback on port 1455 (fallback 1457), tokens in `$CODEX_HOME/auth.json`, requests carry `Authorization: Bearer` + `ChatGPT-Account-ID` | `codex-rs/login/src/auth/manager.rs`, `login/src/oauth/authorization.rs`, `login/src/server.rs`, `login/src/auth/storage.rs`, `model-provider/src/bearer_auth_provider.rs` |
| Third-party use of the ChatGPT login | no OpenAI document permits or forbids it: **not found** | OpenAI docs and terms searched; nothing primary |
| Claude Code auth modes | Console API key; Claude Pro/Max/Team/Enterprise login; Bedrock; Vertex; Microsoft Foundry; gateway | https://code.claude.com/docs/en/authentication |
| Third-party use of a Claude subscription | **forbidden**: "Anthropic does not permit third-party developers to offer Claude.ai login into their own applications" | https://code.claude.com/docs/en/legal-and-compliance |
| Claude Code OAuth endpoints, client id, beta header | not published by Anthropic; only community reverse-engineering: **unverified** | https://code.claude.com/docs/en/authentication (absent) |
| Claude Code SDK dependency | the npm package ships a native binary with no declared `dependencies`; which SDK it bundles is not visible | https://registry.npmjs.org/@anthropic-ai/claude-code/latest (2.1.282) |
| Anthropic OpenAPI spec | `openapi_spec_url` removed from `anthropic-sdk-python/.stats.yml` in commit `f9b0cf28` (2026-09-03); the last linked Stainless URL still answers: OpenAPI 3.1.0, contains `MessageStreamEvent` and `content_block_delta`. A snapshot, not a maintained pointer | https://github.com/anthropics/anthropic-sdk-python/commit/f9b0cf28 ; https://storage.googleapis.com/stainless-sdk-openapi-specs/anthropic/anthropic-465bff21a179090915396565d1ae8f705cf8596e2ec920eb121072f25b8a7d68.yml |
| OpenAI OpenAPI spec | `openai/openai-openapi`, `main`, `openapi.yaml`/`openapi.json`, OpenAPI 3.1.0, MIT, ~3.7 MB, contains `ResponseStreamEvent` | https://github.com/openai/openai-openapi |
| progenitor | 0.15.0 (2026-09-10); OpenAPI 3.0.x only (via `openapiv3` 2.2); no SSE support documented | https://crates.io/api/v1/crates/progenitor ; README |
| typify | 0.8.0 (2026-09-09); JSON Schema → serde types; used in cox since T30.10 | https://crates.io/api/v1/crates/typify ; https://github.com/oxidecomputer/typify |
| OpenAPI 3.1 + SSE Rust generator | none maintained found: **not found** (README-level search only) | progenitor, openapi-generator READMEs |

Reading: no vendor ships Rust. For OpenAI a maintained typed crate exists (`async-openai`); for Anthropic only the spec exists, and only as an unlinked snapshot, so generation is from a vendored copy (T30.10 does this for the stream types). Subscription login: Anthropic forbids it in writing; OpenAI is silent.

### 4.3.2 LM Studio's native API as a cox provider (checked 2026-09-25)

LM Studio serves three API families on one port (default 1234). Facts are
from LM Studio's docs and from the running server (LM Studio CLI commit
`07b7252`, MLX runtime `mlx-llm-mac-arm64-apple-metal-advsimd` 1.11.0,
model `prism-ml/bonsai-27b`).

| Fact | Source |
|---|---|
| Native REST API v1 at `/api/v1/*` is an "official release" in LM Studio 0.4.0; the legacy `/api/v0/*` stays | https://lmstudio.ai/docs/developer/api-changelog |
| Anthropic-compatible `POST /v1/messages` arrived in 0.4.1; OpenAI-compatible `/v1/chat/completions` tool calling since 0.3.6 | https://lmstudio.ai/docs/developer/api-changelog |
| v1 endpoints: `GET /api/v1/models`, `POST /api/v1/models/load` (`context_length`, `eval_batch_size`, `flash_attention`, `num_experts`, `offload_kv_cache_to_gpu`), `POST /api/v1/models/unload`, `POST /api/v1/models/download`, `POST /api/v1/chat` | https://lmstudio.ai/docs/developer/rest, https://lmstudio.ai/docs/developer/rest/load |
| `GET /api/v1/models` returns per model `max_context_length`, `loaded_instances[].config.context_length`, `capabilities.trained_for_tool_use`, `capabilities.reasoning.allowed_options`, `capabilities.vision`, quantization and format | live `curl localhost:1234/api/v1/models` |
| `POST /api/v1/chat` rejects a `tools` array (`"Unrecognized key(s) in object: 'tools'"`); its only tool mechanism is MCP servers passed as `integrations` | live call; https://lmstudio.ai/docs/developer/core/mcp |
| v1 chat streams named SSE events (`chat.start`, `prompt_processing.*`, `reasoning.start/delta/end`, message deltas, `chat.end` with the full result) and reports `stats` (`input_tokens`, `total_output_tokens`, `reasoning_output_tokens`, `tokens_per_second`, `time_to_first_token_seconds`); `previous_response_id` chains stateful chats | live streaming call; https://lmstudio.ai/docs/developer/rest |
| One auth scheme for all three families when "Require Authentication" is on: `Authorization: Bearer <token>` (Anthropic path also `x-api-key`) | https://lmstudio.ai/docs/developer/core/authentication |
| No Rust SDK: crates.io has no `lmstudio`, `lm-studio`, `lmstudio-rs`, `lmstudio_rs` (`lms` is an unrelated rsync tool). Official SDKs are TypeScript and Python only | https://crates.io/api/v1/crates/lmstudio (404), https://github.com/lmstudio-ai/lmstudio-js, https://github.com/lmstudio-ai/lmstudio-python |
| No published OpenAPI or JSON Schema for the native API found; lmstudio-js keeps TS/zod types in `packages/lms-shared-types/src` | https://github.com/lmstudio-ai/lmstudio-js (tree checked; deeper listing **unverified**) |
| The SDKs talk to LM Studio over a WebSocket RPC protocol | **unverified** (secondary summaries only; no LM Studio protocol doc) |
| cox's Anthropic Messages path works against LM Studio as is: one-tool task finished, `cost_usd` 0 | live run, T30.14 |
| `lms load prism-ml/bonsai-27b --context-length 65536 -y` left the model loaded with `context_length` 251648 and `parallel` 4 (`lms ps`, `/api/v1/models`): the loaded context must be read back, not assumed | live, 2026-09-25 |

What follows for cox. Wire types fall to step 3 of D3/A40 (no Rust SDK, no
spec), and the native surface cox needs is small: `models` and
`models/load`. The chat loop cannot move to `/api/v1/chat`, because it takes
no custom tool schemas, so it stays on `/v1/messages` through the existing
Anthropic provider. OpenAI Chat is not an option either while `chat.rs` drops
tool calls (ideas.md). The native API earns its place for what the
compatibility endpoints lack: the loaded context length (the context window
cox needs for compaction, today a hand-set `context_window`), tool-use and
reasoning capabilities, load state, and loading a model with an explicit
context length before a session starts.

### 4.4 Routing evidence (D5)
Copilot's auto model selection is praised because it is explicit, priced (10 % discount) and switchable; Claude Code's Haiku delegation is complained about because it is silent. aider's `--weak-model` (commits, summaries) and OpenCode's small model for titles are the same pattern. Jobs that tolerate a small model, by consensus of the surveyed tools: titles, summaries, commit messages, compaction, search/explore, tool-result summarisation, classification. Effect-size numbers from the survey ("4.2× savings", "Codex 3–4× fewer tokens than Claude Code") are unsourced and dropped. [med]

### 4.5 Crate survey (versions verified against crates.io on 2026-09-02)
| Area | Recommended | Alternatives considered | Note |
|---|---|---|---|
| TUI | ratatui 0.30.2 (2026-06-19), crossterm 0.29.0, ratatui-macros 0.7 | cursive 0.21 (retained, 2024), iocraft/r3bl (small) | Codex choice; TestBackend for tests |
| composer / text | tui-textarea 0.7.0 (2024-10; stable, low churn) | own widget (Codex) | start with tui-textarea, replace if it blocks |
| markdown | pulldown-cmark 0.10 → own spans | tui-markdown 0.3.9 (2026-07), termimad 0.35 | Codex renders its own; tui-markdown as fallback |
| highlighting | syntect 5.3 | tree-sitter-highlight 0.27 | syntect for display; tree-sitter for structure |
| images (v0.2) | ratatui-image 11.0.6 | — | — |
| async | tokio 1.53, tokio-util, tokio-stream, futures 0.3 | async-channel 2.5 | — |
| HTTP / SSE | reqwest 0.12 (rustls), eventsource-stream 0.2.3 | reqwest-eventsource 0.6 (2024) | see §1.3 |
| MCP / ACP | rmcp 3.2.0 (2026-08-31; stdio, Streamable HTTP, OAuth via `auth` — ledger #13), agent-client-protocol 2.0.0 | mcp-sdk, mcpr (unofficial) | — |
| search | ignore 0.4.33, grep-searcher 0.1.17, grep-regex, globset, nucleo 0.5 | — | ripgrep's own libs |
| diff / patch | similar 3.2.0, diffy 0.5.2 | imara-diff | diffy for unified apply, similar for display |
| code structure | tree-sitter 0.25 (Codex) / 0.27 (latest, MSRV 1.90) + grammars | — | pin to grammar compatibility |
| git | shell out to `git` | gix 0.87.1, git2 0.21 | gix is v0.2 |
| tokens | tiktoken-rs 0.12.0 | tokenizers 0.23 | Anthropic: `count_tokens` endpoint |
| store / config | diesel 2.2 (`sqlite`) + libsqlite3-sys 0.30 (`bundled`, FTS5) + diesel_migrations, toml 1.1, toml_edit 0.25, figment, directories 6, keyring 4.2 | rusqlite 0.40.2 (plan v1), sqlx 0.9 (Codex), SeaORM (async) | sync ORM as in rtok D13: typed models, no runtime in hooks/tests; FTS5 via `sql_query` |
| plugins (v0.2) | extism 1.30.0 | wasmtime 48 (component model), rhai 1.26, mlua 0.12, dylib (`abi_stable`: rejected, ABI fragility) | — |
| process / sandbox | portable-pty 0.9.0, shlex 2, landlock 0.4.7, seccompiler 0.5.0, nix; `sandbox-exec` via `Command` | birdcage | — |
| observability / CLI / errors | tracing 0.1.44, tracing-subscriber 0.3.23, tracing-appender, opentelemetry 0.31 (feature), clap 4.6, thiserror 2.0, anyhow 1 | miette, color-eyre | — |
| tests | insta 1.48.0, proptest 1.11, wiremock 0.6.5, rstest 0.26, mockall 0.15, assert_cmd 2.2, predicates 3, assert_fs, tempfile 3.27, pretty_assertions, vt100 0.16.2, cargo-nextest, cargo-mutants (optional), cargo-llvm-cov | httpmock 0.8, expectrl | — |
| misc / release | uuid 1.26, jiff or chrono 0.4.45, notify 8.2, which 8, semver 1, cargo-dist, cargo-deny 0.20, git-cliff | indicatif (headless progress) | — |

### 4.6 Measured savings (filled by T8.5, 2026-09-03, `just bench`)
| Mechanism | Sessions | Context-token-turns before | after | Δ |
|---|---|---|---|---|
| archive (D6a truncation) | 5 | 393337 | 148662 | 62.2 % |
| dedup (D6b re-read) | 5 | 157588 | 148662 | 5.7 % |
| outline (D6c) | 5 | 158291 | 148662 | 6.1 % |
| deferred tools (D6d) | 5 | 189854 | 148662 | 21.7 % |
| compaction (D6f) | 5 | 161736 | 148662 | 8.1 % |
| prefix stability (D6e, emulated cache-write) | 5 | 244790 | 148662 | 39.3 % |

Method: 5 hand-written transcripts (`evals/token/sessions/*.jsonl`, 6 turns
each over the `evals/token/workspace` fixture) replayed through the real
`Session` loop with a `Scripted` provider and real `read`/`grep`/`glob`
tools; toggling is the real config flag per mechanism (see
`evals/token/README.md`). Totals are sums of `Usage::context_tokens` from
the ledger rows the loop wrote. Caveats: transcripts are built to exercise
each mechanism (big-file reads, repeated reads, outlines), so the shares
are ceiling-shaped, not field averages; `prefix` counts emulated
cache-write bytes, not billed tokens. No mechanism measured 0, so none is
flagged for removal.

### 4.7 Footprint (filled by T30.2, 2026-09-23, `just footprint`)

| Metric | Darwin-arm64 | How |
|---|---|---|
| cold start (`cox --version`, median of 5) | 11.0 ms | `date +%s%N` around the process |
| first frame (scripted `stream-json`, median of 3) | 40.0 ms | spawn to first event on stdout |
| replay RSS peak (30 turns, 60 provider calls) | 26.4 MiB | `/usr/bin/time -l`, max over turns |
| binary size (`target/release/cox`) | 43.9 MiB | `cargo build --release -p cox` |

Method: `scripts/footprint.sh`; baseline `scripts/footprint.json` (keyed by
OS-arch); CI runs `footprint.sh --check` and fails on a >20% regression of
any metric. Replay: every `evals/token/sessions/*.jsonl` line becomes a
`Scripted` scenario per user turn, run back to back through `--resume` with
real `read`/`grep`/`glob` over a workspace copy — the corpus is 30 user
turns / 60 provider calls, not 50 (the card's number predates the corpus;
context still grows across turns, which is what RSS measures). Caveats:
timings are machine- and load-dependent (CI compares per-runner, not against
this table); no network, no key, scripted provider only. Comparative numbers
for other agents are out of scope here — their footprint threads move weekly:
[Claude Code performance degradation #19452](https://github.com/anthropics/claude-code/issues/19452),
[Claude Code high memory usage #8836](https://github.com/anthropics/claude-code/issues/8836),
[Codex CLI memory leak #9345](https://github.com/openai/codex/issues/9345),
[Codex 12GB on startup (forum)](https://community.openai.com/t/codex-consuming-12gb-memory-for-5-minutes-on-startup-macos/1376282).

## 5. Testability patterns adopted
1. `Provider` trait with `Scripted` and `Replay` (cassette) implementations; cassettes re-recorded on demand and redacted. Temperature 0 and seeds do not give bit-exact replay across providers; replaying the event log does. [high]
2. Golden `Event` JSONL for loop scenarios (`insta`); the rollout file and the fixture are the same format. [design]
3. ratatui `TestBackend` + `insta` per widget and per frame; `portable-pty` + `vt100` for the real binary (Codex practice). [high]
4. Tools in `tempfile` trees; `proptest` on `str_replace` and V4A (`parse(print(p)) == p`, edit-then-reverse identity); fuzz targets for SSE/V4A/frontmatter parsers. [design]
5. Evals separate from tests: Terminal-Bench 2.0 agent (Harbor) + 10 in-repo tasks, run on demand with the real provider, cost recorded in the ledger. §5.3 holds the recorded runs.

### 5.3 First eval run (T12.1, 2026-09-04)

Harness `evals/run.py` (`just eval`), 10 tasks in `evals/tasks/`. (The
terminal-bench 0.2.x adapter written here was replaced in T30.9 by a
Harbor agent, `evals/src/cox_evals/tbench.py`; see the Terminal-Bench
subsection below.)

Dry-run (`COX_PROVIDER=scripted just eval --dry-run`): **10/10 passed**,
$0.0000, no network, no key.

Live run: **blocked, $0 spent.** No Anthropic key in env; `OPENAI_API_KEY`
is set but the account is exhausted (`429 credit_balance_exhausted` on
`POST /v1/responses` with `gpt-4o-mini`, verified by direct curl the same
day — cox surfaces it as a fast turn error, exit 1). Reproduce when funded:

```bash
python3 evals/run.py --provider openai --model gpt-4o-mini
```

Related precise bug (not fixed here, provider owner's scope): neither
OpenAI module wraps its stream in `retry::stream_with_retry` (only the
Anthropic one does), so a retryable 429 fails on the first attempt instead
of backing off per §1.14. Worth a wiremock contract test (429, 429, 200)
when touched.

Two drive-by findings from building the harness (both fixed in T12.1):
empty `workspace_roots` reached the tools verbatim so every confined
tool failed outside `--cwd` (plan §1.6 says empty means git-root-else-cwd;
now resolved in `session::open`); eval runs pass `--no-hooks --no-mcp`
because ambient repo servers add startup noise to every task.

#### Live run with and without the verify preset (T30.3, 2026-09-25)

`claude-sonnet-5`, the 10 in-repo tasks, `--approve never
--permission-mode auto`. Cost and tokens are the per-task ledger rows
(`usage`, what `cox stats` reads), summed; the harness's own
per-task rounding gives $0.0516 / $0.0628.

| Configuration | Pass | Provider calls | Input | Output | Cache read | Cache write | Cost |
|---|---|---|---|---|---|---|---|
| baseline (`--no-hooks`) | 9/10 | 21 | 42 | 951 | 173 260 | 2 932 | $0.0515 |
| `--preset verify` | 9/10 | 25 | 50 | 1 273 | 208 382 | 3 300 | $0.0626 |

The one failure is the same task in both: `append-line` exits 2 because
the model first tried a writing `bash` command, which `--approve never`
denies ("Exec calls require approval"), then finished with `edit`; the
file is correct but the harness scores any denial as a failure. The
preset costs +22 % here and changes no outcome: nine of the ten tasks are
one tool call and one answer, so there is nothing for a test hook to
catch, and on `append-line` it doubled the calls (4 → 8). These tasks are
too small to show a verification benefit; Terminal-Bench is where it
would.

Getting here took three fixes the offline suite could not see: an
org-level key needs `anthropic-workspace-id` (T30.4); no production path
priced a call, so every ledger row was $0 (T30.5); the Anthropic stream
never emitted `ToolUseEnd`, so every tool call was dropped (T30.6). The
harness overrides `HOME`, which hides the macOS keychain, so the key has
to come from the environment:

```bash
ANTHROPIC_API_KEY="$(security find-generic-password -s cox -a anthropic -w)" \
  uv run --project evals cox-evals --provider anthropic --model claude-sonnet-5 [--preset verify]
```

#### Terminal-Bench 2.0 subset (T30.9, 2026-09-25)

Terminal-Bench 2.0 runs through Harbor, not the old `tb` CLI
(https://github.com/laude-institute/harbor, PyPI `harbor` 0.23.0,
https://pypi.org/project/harbor/0.23.0/, checked 2026-09-25). The agent is
`cox_evals.tbench:CoxAgent`, a `BaseInstalledAgent`: `install` uploads a
Linux cox build to `/installed-agent/cox`, `run` executes one
`cox run --output-format json` in the task container and copies the payload's
tokens and cost into Harbor's `AgentContext`. Tasks come from
https://github.com/laude-institute/terminal-bench-2 at commit
`69671fbaac6d67a7ef0dfec016cc38a64ef7a77c` (recorded by Harbor in each
trial's `config.json`).

Setup: cox from `b027a47`, cross-built with `cargo zigbuild --release
--target aarch64-unknown-linux-gnu.2.31` (cargo-zigbuild 0.23.4, zig
0.16.0); Docker in colima 0.10.3 (vz, arm64). `claude-sonnet-5`,
`--budget 0.25`, `--max-turns 40`, `--permission-mode bypass --sandbox
danger-full-access` (the container is the isolation boundary), two trials
at a time, three tasks picked for spread: git surgery, a COBOL-to-Python
port, a Coq proof.

| Task | Reward | Turns | Input | Output | Cache read | Cache write | Cost |
|---|---|---|---|---|---|---|---|
| `fix-git` | 1.0 | 9 | 17 | 2 691 | 102 496 | 7 533 | $0.0663 |
| `cobol-modernization` | 1.0 | 22 | 43 | 13 242 | 379 814 | 16 349 | $0.2493 |
| `prove-plus-comm` | 1.0 | 4 | 7 | 1 013 | 34 356 | 997 | $0.0195 |
| **total** | **3/3** | 35 | 67 | 16 946 | 516 666 | 24 879 | **$0.3351** |

Cost is cox's own ledger figure from the `cox run` payload; wall time
2 min 56 s. Three tasks out of 89 is a smoke test of the agent and the
pipeline, not a leaderboard score. `cobol-modernization` finished at
$0.249 of a $0.25 cap, so a larger subset needs a higher per-task budget.

A first attempt the same day spent about $0.51 on `cobol-modernization`
and `prove-plus-comm` (both stopped by the budget) plus an unrecorded
share of `fix-git` (at most $0.25, interrupted), and scored nothing, for
two reasons fixed in `db85e5f`: `--permission-mode auto --approve never`
denied 24-26 calls per task, which the model spent its budget retrying;
and the jobs dir sat in `/tmp`, which colima does not share with its VM
(only `$HOME`), so the verifier's reward file never reached the host.

Reproduce (Docker reachable, Linux cox built):

```bash
ANTHROPIC_API_KEY="$(security find-generic-password -s cox -a anthropic -w)" \
  uv run --project evals --extra tbench harbor run -d terminal-bench@2.0 \
  -a cox_evals.tbench:CoxAgent -m anthropic/claude-sonnet-5 --force-build -n 2 \
  -i fix-git -i cobol-modernization -i prove-plus-comm \
  --ak cox_bin=<linux cox> --ak budget_usd=0.25 -o ~/.cache/cox-evals/tb-jobs
```

The verify preset (T30.3) was not run on Terminal-Bench: the harness
system addendum and hook are wired for the in-repo harness only, and all
three tasks already passed without it.

## 6. Fact-check ledger
| # | Claim (report) | Verdict | Correction / source |
|---|---|---|---|
| 1 | ratatui 0.30.2 released 2026-06-19 (D) | confirmed | crates.io |
| 2 | rmcp 3.2.0 released 2026-08-31 (D) | confirmed | crates.io |
| 3 | MCP latest revision 2026-07-28 (E) | confirmed | modelcontextprotocol.io changelog |
| 4 | HTTP+SSE deprecated in 2026-07-28 (E) | confirmed, clarified | deprecated since 2025-03-26; reclassified 2026-07-28 |
| 5–7, 18 | Anthropic prices from finout.io (E) | unverifiable by the checker | plan uses the Claude API reference table cached 2026-06-24: Haiku 4.5 $1/$5, Sonnet 5 $2/$10, Opus 5 $5/$25, Fable 5.1 $10/$50; report E's "Sonnet promo ends Sept 1, then $3/$15" is unconfirmed → T1.7 re-verifies from the official pricing page |
| 28 | Anthropic prices re-verified 2026-09-02 (T1.7) | confirmed, Sonnet 5 promo extended | Haiku 4.5 $1/$5, Sonnet 5 $2/$10 (promo extended indefinitely), Opus 5 $5/$25, Fable 5.1 $10/$50; cache pricing verified; config/prices.toml carries verified_on dates — https://platform.claude.com/docs/pricing |
| 8 | eventsource-stream last release 2022-02-17 (D) | confirmed date, verdict rejected | Codex ships it (§1.3); small and finished |
| 9–10 | Claude Code compaction at 250–300 k; tool results 500–2 000 tokens (F) | unverifiable | dropped as design inputs |
| 11 | Claude Code sandbox = Seatbelt / bubblewrap (A) | confirmed | code.claude.com/docs/en/sandboxing |
| 12 | Claude Code hook events = 7 (A) | refuted | 31 events incl. SessionStart/End, UserPromptSubmit, Stop, StopFailure, PostToolUseFailure, PermissionRequest/Denied, Notification, PreCompact/PostCompact, PreModelSwitch/PostModelSwitch, Elicitation… (code.claude.com/docs/en/hooks) |
| 13 | rmcp has no OAuth (D) | refuted | OAuth 2.0 via `auth` feature (docs.rs/rmcp/3.2.0) |
| 14 | JetBrains adopted ACP Jan 2026 (E) | refuted | 2025-10-06 (zed.dev/acp) |
| 15 | tokio 1.53.1 on 2026-07-20 (D) | confirmed | crates.io |
| 16 | Terminal-Bench 2.0 has 89 tasks (E) | unverifiable | tbench.ai shows 4.0 |
| 17 | SWE-bench Verified is the de-facto benchmark (E) | unverifiable | site lists several variants |
| 19 | rig-core 0.42 multi-provider (D) | confirmed | crates.io |
| 20 | `anthropic` crate 0.0.8 unofficial, 2024 (D) | confirmed | crates.io |
| 21 | min cacheable prefix 1 024–4 096 tokens (F) | refuted | model-dependent: 512 (Fable 5.1, Opus 5, Sonnet 5), 4 096 (Haiku 4.5, Opus 4.5) — prompt-caching docs |
| 22 | Codex uses sqlx 0.9, ratatui 0.30.2, insta, wiremock (B) | confirmed by author | `codex-rs/Cargo.toml` read 2026-09-02 (§1.3) |
| 23 | Codex "tonic used for inter-service gRPC?" (B) | present in manifest, purpose unverified | tonic 0.14.3 listed |
| 24 | OpenCode has no MCP; aider supports "Claude 3.7" (C sub-agent) | refuted / stale | OpenCode docs list MCP + LSP; aider model list is generated from litellm and is current |
| 25 | Claude Code default model "3.5 Sonnet", "MCP 1.0" (A) | stale | dropped |
| 26 | OpenAI "GPT-5.6 Sol/Terra/Luna", Gemini "3.7 Flash", DeepSeek V3.2 prices (E) | unverified (third-party pricing sites only) | not used; T1.7 fills `config/prices.toml` from official pages |
| 27 | "RTK 60–90 %", "caveman 46 %", "engram +10.4 % at 8× fewer tokens" (F) | vendor claims, unverified | rtok's own measurements found 3–40 % for the hook stack; see `~/GitHub/rtok/research.md` |

## 7. Method and limits
- Agents: A (Claude Code/Copilot, 19 lookups), B (Codex, 22), C (competitors, 3 sub-agents, ~75 lookups), D (crates, 69), E (specs, 19), F (tokens/testing, **3 lookups** — largely written from the model's memory; treated as directional only), G (fact-check, 43). Total ≈ 550 k subagent tokens on Haiku, ≈ $1.
- Author verifications: `codex-rs/Cargo.toml` (deps), crates.io API (18 crates), crates.io name availability (`cox`, `coxswain`, `boatswain`, `mizzen`, `brigantine` free), the Claude API reference (models, prices, caching thresholds, thinking/effort rules).
- Still unverified: official prices for non-Anthropic providers; Claude Code's compaction thresholds and truncation limits; Terminal-Bench 2.x task counts; Copilot CLI internals (closed). Each has a task that replaces the guess with a measurement (T1.7, T8.5, T12.1).

## 8. Field survey 2026-09-22 — what terminal coding agents ship now, and where cox stands

Date: 2026-09-22. Method: four parallel research agents (Sonnet 5, web access, ~180 lookups, every bullet carries a URL in the agent transcripts) covering (A) Claude Code / Codex CLI / Gemini CLI→Antigravity / Copilot CLI, (B) OpenCode, Crush, Amp, Cursor CLI, Factory Droid, Goose, Pi, aider, Kilo, Mistral Vibe, Qwen Code, Kimi, Warp, (C) terminal capabilities and ratatui ecosystem, (D) capability trends and a matrix. The author verified the cox column against the code (not the plan), and the ratatui/crossterm claims against the vendored crate sources (ledger #29–31). Third-party claims about vendors keep the agents' confidence: [high] = official docs/changelog/repo, [med] = one secondary source, [unverified] = not confirmed. The improvement plan that follows from this section is `docs/design/improvement-plan-2026.md` (proposal, A26); nothing here changes §0 decisions.

### 8.1 What the field converged on (table stakes in 2026)

| Convention | Who ships it | Confidence | cox today |
|---|---|---|---|
| `Shift+Tab` cycles into a read-only *plan* mode | Claude Code, Codex (`/plan` too), Copilot CLI; OpenCode uses `Tab` | high | `Tab` cycles default→plan→auto (§1.13); no plan-specific view |
| Checkpoint before every edit, `/rewind` restores code, conversation or both | Claude Code (`Esc Esc`, ~30-day retention; bash-caused changes *not* tracked), Cursor CLI (`/rewind` timeline with per-turn diffs, branch-on-rewind), OpenCode (`/undo`, `/redo`, separate git object DB), Gemini CLI | high | implemented in T26.1–T26.2, including shell-caused changes |
| Queue messages while a turn runs; a *send-now* key interrupts and flushes | Claude Code (`Ctrl+Enter`, 2.1.275), Pi (queued messages pinned above the editor) | high | composer is blocked during a turn |
| Subagents run in the background by default, isolated in git worktrees on request | Claude Code (`isolation: "worktree"`, agents map with per-agent cards), Codex (up to 6, "Smart Approvals" label the source thread), Copilot (`/fleet`, live subagent timing) | high | `agent` tool with presets and worktree isolation (T27.3); `/agents` lists sessions; approvals are not labelled by source |
| `/fork` / `/branch` and `/handoff` | Codex (`/fork`, `/side`), Claude Code (`/branch`, `--fork-session`), Amp (`/handoff` seeds a new thread) | high | none (`parent_id` column exists) |
| `/context` token breakdown and a visible auto-compact threshold | Claude Code (`/context`, `/autocompact`), OpenCode v2 compacts *before* the call | high | `ctx %` and `cache %` in the status line; compaction after `TurnDone` only |
| Themes as files, `/theme` with live preview; syntax themes from `.tmTheme` | Codex (32 themes, `.tmTheme` drop-in), Crush (`Ctrl+P` palette, `Ctrl+E` live editor), OpenCode ("system" theme derived from the terminal background, `{dark,light}` per colour) | high | `tui.theme = auto|dark|light` picks one of two syntect base16 themes; `auto` is not detected |
| Custom keybindings file | Claude Code (`keybindings.json`), Codex (F13–F24) | high | none |
| Real vim: text objects, visual mode, undo/redo of drafts | Claude Code 2.1.118, Codex | high | vim-lite (`hjkl`, `i`, `x`, `dd`-less) |
| `!` drops into a shell line without leaving the session | Factory Droid, Claude Code | high | none |
| Desktop notification on turn end / approval (OSC 9/777 + BEL, hooks) | all four majors; Pi | high | none |
| OSC 8 hyperlinks on paths/URLs; clickable | Codex, Pi | high | none |
| Image paste (`Ctrl+V`), screenshots to the model | Codex (Windows/Linux too), Claude Code (macOS; Windows open issue), OpenCode (drag-drop), Pi (Kitty graphics inline) | high | refused with a hint (gate T19.4) |
| Screen-reader / plain mode, reduced motion, daltonized themes | Claude Code (`--ax-screen-reader`, `prefersReducedMotion`) | high | `NO_COLOR`, ASCII glyph fallback (T14.1/T14.2) |
| Voice input (`/voice`) | Claude Code, aider | high | none — out of scope (see plan §6) |
| Remote control from phone/web, session keeps running locally | Claude Code Remote Control, Codex Remote, Amp Orbs, Cursor background agents | high | none — out of scope for v0.2 |
| Scheduled agents / `/loop` | Claude Code Routines (cloud), Cursor CLI `/loop` (local) | high | none |
| MCP OAuth, elicitation, 2026-07-28 spec (MRTR, list TTLs, MCP Apps extension; sampling deprecated) | Claude Code CLI, Copilot, Codex | high | stdio + HTTP and OAuth implemented (T22.5); elicitation and MCP Apps remain open |
| ACP as an agent | OpenCode (Zed, JetBrains, Neovim), Goose, Amp via adapter; Claude Code/Codex/Cursor not listed as native ACP agents | med | `cox acp` (T11.1) — a real lead |
| `AGENTS.md` under the Agentic AI Foundation, 60 k+ repos; Claude Code reads it since 2.1.277 | everyone | high | yes (T7.1) |
| Native Windows sandbox | Codex only (restricted tokens/ACLs, "experimental") | high | none, loud warning (D7) |

### 8.2 What users complain about (the gaps a newcomer can win on)

- **Trust**: silent model downgrades and routing (Claude Code April 2026 incident, Gemini CLI Pro→Flash), expired credentials in the wider field, hallucinated tool results. cox's D5 ("never up, never silent"), keyring-backed MCP OAuth (T22.5), and the ledger are the answer; the remaining work is *showing* it (plan P28). [high]
- **Noise in multi-agent views**: Codex #12047 "raw scaffolding noise", approvals popping from unnamed threads. [high]
- **"It does not look good yet"** even for Codex (#2609, #21130: semantic colours beyond syntax, Plan/Build switcher). Crush is the reference for looks; OpenCode for "genuinely pleasant" clarity of tool calls and diffs. [high]
- **Footprint**: OpenCode ~1 GB RSS "for a TUI", Goose loads whole sessions into memory, Copilot CLI Node OOM. A Rust binary with a measured RSS is a marketing fact cox has not published. [high]
- **System-prompt tax**: Pi keeps the prompt under ~1 000 tokens by shipping nothing optional; `oh-my-pi` adds the rest as extensions. cox's deferred tools (D6d) are the same idea half-done: the skills index, memory index and instruction files still ride in every request. [high]
- **Copy fidelity**: Claude Code drops GFM features on copy (#26390); Codex falls back to key/value for cramped tables. [high]
- **Windows**: image paste, sandbox and MCP install remain the weakest area for everyone. [high]
- **Policy**: Anthropic forbids third-party apps from offering Claude.ai login or routing requests through Free/Pro/Max credentials (https://code.claude.com/docs/en/legal-and-compliance, checked 2026-09-25). cox does not implement Claude subscription OAuth; API keys and the keyring stay the path. OpenAI neither permits nor forbids third-party use of Codex's ChatGPT login in any document found (§4.3.1). [high for Anthropic, unverified for OpenAI]

### 8.3 Terminal capabilities (author-verified against vendored sources where marked ✔)

| Capability | Mechanism | Support | ratatui/crossterm 0.30.2/0.29 |
|---|---|---|---|
| Distinct `Shift+Enter`/`Ctrl+Enter` | Kitty keyboard protocol | Kitty, Ghostty, foot, Alacritty, iTerm2, WezTerm, Rio, Warp; not tmux | `PushKeyboardEnhancementFlags` ✔ (no-op where unsupported) |
| Flicker-free `insert_before` | scrolling regions | VT100-class terminals | ratatui feature `scrolling-regions` ✔ present, **enabled** in cox (T23.2, 2026-09-24): 40 → 0 full-viewport repaints for 40 inserted cells on the vt100 PTY fixture. Caveat: the `vt100` crate (0.16 `grid.rs` `scroll_up`) keeps no scrollback for lines scrolled off a DECSTBM region; the feature's premise (ratatui#1341) is that real terminals save lines leaving a region whose top is row 1, not re-verified per terminal here, so PTY tests that read scrollback must model that |
| Clipboard over SSH/tmux | OSC 52 | Alacritty, Ghostty, Kitty, WezTerm, tmux forwards | crossterm feature `osc52` → `CopyToClipboard` ✔ present, not enabled |
| Hyperlinks | OSC 8 | iTerm2, Terminal.app 13+, Ghostty, Kitty, WezTerm, Alacritty, VTE | no widget in ratatui; emit the sequence around a span (`hyperrat` exists) |
| Desktop notification | OSC 9 / OSC 777 / BEL | iTerm2, WezTerm, Ghostty, Kitty, Warp (OSC 9) | raw write |
| Tab/taskbar progress | OSC 9;4 | Windows Terminal, Konsole, foot, WezTerm, Kitty, Ghostty | raw write |
| Background colour → dark/light | OSC 11 | xterm, iTerm2, Kitty, Alacritty, WezTerm, foot, VTE, Windows Terminal ≥ 1.22 | `terminal-colorsaurus` / `termbg` crates (new dependency, needs approval) |
| Focus in/out (notify only when unfocused) | focus events | most | `EnableFocusChange` ✔ |
| Inline images | Kitty / iTerm2 / Sixel | Kitty ≥ 0.28, Ghostty, WezTerm, iTerm2; halfblock fallback | `ratatui-image` (v0.2 images gate) |
| Synchronised output | mode 2026 | Ghostty ≥ 1.0, Kitty, WezTerm | ratatui's crossterm backend already wraps frames |
| Languages beyond syntect's ~40 | `two-face` (bat's syntax set, ~250 languages, +0.6 MiB) | — | new dependency, needs approval |

Codex TUI structure worth copying (R§1.6 confirmed by two independent code readings): immutable committed `HistoryCell`s plus exactly one mutable active cell; a `BottomPane` stack of views (approval, pickers) that receives input first; `Ctrl+T` transcript overlay over the inline viewport. cox already has the first and third (T5.3, `Ctrl+O`); the modal stack is single-level.

### 8.4 cox versus the field — capability matrix (cox column verified in code on 2026-09-22)

`yes` shipped · `part` partial · `no` absent · `?` unverified for that vendor

| Capability | cox | Claude Code | Codex | OpenCode | Crush | Copilot | Cursor CLI | Pi | aider |
|---|---|---|---|---|---|---|---|---|---|
| Lossless tool-output archive + `expand` | **yes** | ? | ? | ? | ? | ? | ? | no | no |
| Explicit tiered routing, never up | **yes** | no (silent Haiku) | part (effort profiles) | part | part | part (auto, discounted) | no | no | part (architect/editor) |
| Per-request usage row with cache read/write | **yes** | part (`/cost`) | ? | ? | ? | ? | ? | ? | part |
| Deferred tool schemas + `tool_search` | **yes** | yes | ? | no | no | ? | ? | no | no |
| Dedup of repeated reads | **yes** | no | no | no | no | no | no | no | no |
| OS sandbox macOS + Linux | yes | yes | yes | ? | ? | yes | ? | no (containers) | no |
| Windows sandbox | no | no | yes (exp.) | no | no | part (proxies) | ? | no | no |
| Permission rules, deny wins | yes | yes | yes | ? | ? | yes | ? | no | no |
| Plan mode | part | yes | yes | yes | ? | yes | yes | no (`oh-my-pi`) | no |
| Checkpoints / rewind | **yes** | yes | ? | yes | ? | ? | yes | no | part (git commits) |
| Bash-caused changes in rewind | **yes** | no | ? | yes (worktree snapshots) | ? | ? | ? | no | yes (commits) |
| Queued messages + send-now | **no** | yes | ? | ? | ? | ? | ? | yes | no |
| Background subagents | part | yes | yes | yes | ? | yes | yes | no | no |
| Approval labelled by source agent | no | yes | yes | ? | ? | yes | ? | n/a | n/a |
| Worktree isolation | **yes** | yes | yes | part (community) | ? | yes | ? | no | no |
| `/fork`, `/handoff` | no | yes | yes | ? | ? | ? | yes (rewind branch) | no | no |
| `/loop` / scheduled | no | yes | part | ? | ? | ? | yes | no | no |
| Hooks (events) | yes (11 of 13 fire) | yes (30+) | part | yes (25+) | part | ? | ? | no | no |
| Skills (`SKILL.md`) | part (discovered, not in context) | yes | ? | ? | yes | ? | ? | yes | no |
| Custom slash commands from files | part (`cox ext list` only) | yes | yes | yes | ? | yes | yes | yes | no |
| MCP client | yes | yes | yes | yes | yes | yes | yes | no | part |
| MCP OAuth | **yes** | yes | yes | ? | yes | yes | ? | no | no |
| MCP elicitation | no | yes (CLI) | ? | ? | ? | ? | ? | no | no |
| ACP server | **yes** | ? | ? | yes | ? | ? | ? | ? | ? |
| `cox mcp` (tools as an MCP server) | **yes** | no | yes | no | no | no | no | no | no |
| Headless `stream-json` | yes | yes | yes | yes | yes | yes | yes | yes (RPC) | no |
| Multi-provider incl. local | **yes** | no | no | yes (75+) | yes | part | no | yes | yes |
| `ask_user` answered interactively | **no** (fixed answer) | yes | yes | yes | yes | yes | yes | no | n/a |
| Themes as files, `/theme` | no | part | yes | yes | yes | yes | ? | yes | no |
| Terminal background detection | no | ? | ? | yes | ? | ? | ? | ? | no |
| Word-level / side-by-side diff | no | no | part | yes (`diff_style`) | ? | ? | ? | ? | yes (best-rated) |
| Collapsible tool cards | part (fold + hint) | yes | yes | yes | yes | yes | ? | ? | no |
| Keybindings file | no | yes | part | ? | ? | ? | ? | ? | no |
| Vim mode | lite | full | full | ? | ? | ? | ? | ? | no |
| Mouse | no (config key is dead) | yes | ? | yes | yes | yes | ? | ? | no |
| Kitty keyboard protocol | no | ? | ? | ? | ? | ? | ? | yes | no |
| OSC 8 links | no | ? | yes | ? | ? | ? | yes (Jan 2026) | yes | no |
| OSC 52 clipboard | no | ? | ? | ? | ? | ? | ? | ? | no |
| Notifications (OSC 9 / bell) | no | yes | yes | ? | ? | yes | ? | ? | no |
| Image paste | no | yes | yes | yes | ? | ? | ? | yes | part |
| `/context` breakdown | no | yes | ? | ? | ? | ? | ? | ? | part (`/tokens`) |
| Pre-emptive compaction (before the call) | no (after turn) | yes | ? | yes | ? | ? | ? | ? | no |
| Auto-memory | part (opt-in extraction) | yes | yes | ? | ? | yes | ? | no | no |
| Screen-reader / plain mode | no | yes | ? | ? | ? | yes (a11y work) | ? | ? | part (plain) |
| Secret redaction of transcripts | part (`record --redact`) | part (community) | ? | ? | ? | yes | ? | ? | no |
| `doctor` | yes | yes | no | no (requested) | no | ? | ? | ? | no |
| Measured RSS / startup published | no | no | no | no (~1 GB reported) | no | no | no | no | no |
| Repo map | no (gate T19.6) | no | no | ? | ? | no | no | no | yes |
| LSP diagnostics after edit | no (gate T19.2) | part | part | yes | yes | ? | ? | part (ext) | no |

Reading: cox's core economics (archive, dedup, deferred tools, routing, ledger, `cox mcp`, ACP, multi-provider) are ahead of every vendor; checkpoints and rewind now close one major surface gap, while the remaining gaps include no queue, no themes, dead config keys, and a fixed-answer `ask_user`. The plan therefore spends P22–P26 on the surface and keeps the core decisions.

### 8.5 Fact-check ledger additions

| # | Claim | Verdict | Source |
|---|---|---|---|
| 29 | ratatui 0.30.2 has a `scrolling-regions` feature that makes `insert_before` scroll a region instead of repainting | confirmed by author | `~/.cargo/registry/.../ratatui-0.30.2/Cargo.toml` line 88; PR ratatui/ratatui#1341 |
| 30 | crossterm 0.29 ships `CopyToClipboard` behind an `osc52` feature | confirmed by author | `crossterm-0.29.0/Cargo.toml` line 66, `examples/copy-to-clipboard.rs` |
| 31 | crossterm 0.29 exposes `PushKeyboardEnhancementFlags` | confirmed by author | `crossterm-0.29.0/src/event.rs` |
| 32 | cox `tui.mouse`, `tui.theme = "auto"` are read but have no effect; `ask_user` in the TUI is `Answers::Fixed`; skills index and custom commands are only in `cox ext list`; `SessionStart` hook never fires | confirmed by author | `crates/cox/src/session.rs`, `crates/cox-tui/src/app.rs`, `crates/cox-core/src` grep on 2026-09-22 |
| 33 | Gemini CLI retired into a closed-source Antigravity CLI (June 2026) | [med] | developers.googleblog.com (agent A/D), not read by the author |
| 34 | Anthropic disabled Claude Pro/Max OAuth for third-party harnesses (2026-04-04) | confirmed in substance, date unverified | https://code.claude.com/docs/en/legal-and-compliance (checked 2026-09-25): third parties may not offer Claude.ai login or route requests through plan credentials; the enforcement date is from secondary press only |
| 35 | Vendor model names quoted by reviewers (e.g. "GPT-6 Astra") and star counts (OpenCode 140–172 k, Pi 104–140 k) | [unverified] | vary by source; directional only |
| 36 | Codex CLI checkpoints/rewind | [unverified] | no documentation found by agent D |
| 37 | Codex's ChatGPT-plan login is sanctioned for third-party clients | [unverified] | no OpenAI document found either way (2026-09-25); `codex-rs/login` shows the flow uses Codex's own OAuth client id |
