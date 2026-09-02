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
| storage | sqlx 0.9 (SQLite), JSONL rollouts in `~/.codex/sessions` | rusqlite (sync; hooks and tests need no runtime) + JSONL |
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

## 4. Token economy, routing, provider layer, crates

### 4.1 Prompt caching (the largest lever)
Prefix match over tools → system → messages; any byte change invalidates everything after it. Anthropic: up to 4 breakpoints, 5-minute default TTL, 1-hour TTL option, cache write 1.25× input, cache read 0.1× (Fable 5.1 cache read $0.25/MTok); model-scoped, so a routing cascade forfeits reuse across models. OpenAI: automatic prefix caching. Anthropic's own guidance: measure the capable model at lower `effort` before building a cascade. [high — Claude API reference + prompt-caching docs; ledger #21] → D5, D6e.

### 4.2 Compaction and truncation
Claude Code auto-compacts near the window and reloads instruction files after; `/compact [focus]`; older tool results are "microcompacted". Codex compaction undocumented. Third-party numbers ("Claude Code auto at 250–300 k", "500–2 000 tokens per tool result", "335 k → 169 k") were **not verifiable** (ledger #9–10) and are not used as design inputs; cox measures its own (T8.5). Techniques that are verifiable by construction: head/tail truncation with the full output on disk (cox D6a), replacing old tool results with pointers (D6f microcompact), search-before-read and line-range reads, structural outlines (tree-sitter), deferred tool schemas (Claude Code ToolSearch; Anthropic `tool_search_tool_*` server tools), diff-only edits. [high for mechanisms, low for third-party effect sizes]

### 4.3 Why an own provider layer (D3)
Candidate crates: rig-core 0.42 (multi-provider, opinionated), genai 0.7-beta, async-openai 0.41, community `anthropic` 0.0.8 (2024, unofficial, stale — ledger #20). What decides cost and correctness in 2026 is wire-level: `cache_control` placement, thinking blocks replayed unchanged on the same model, `effort`, `fallbacks`, `stop_details`, server tools, per-message system blocks. None of the frameworks track all of these, and each provider is ~500 LOC. Codex hand-rolls its client and uses `eventsource-stream` for SSE (§1.3). Verdict: own layer, `eventsource-stream` for SSE, `wiremock` + recorded `.sse` fixtures for tests.

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

### 4.6 Measured savings (filled by T8.5)
| Mechanism | Sessions | Context-token-turns before | after | Δ |
|---|---|---|---|---|
| (pending) | | | | |

## 5. Testability patterns adopted
1. `Provider` trait with `Scripted` and `Replay` (cassette) implementations; cassettes re-recorded on demand and redacted. Temperature 0 and seeds do not give bit-exact replay across providers; replaying the event log does. [high]
2. Golden `Event` JSONL for loop scenarios (`insta`); the rollout file and the fixture are the same format. [design]
3. ratatui `TestBackend` + `insta` per widget and per frame; `portable-pty` + `vt100` for the real binary (Codex practice). [high]
4. Tools in `tempfile` trees; `proptest` on `str_replace` and V4A (`parse(print(p)) == p`, edit-then-reverse identity); fuzz targets for SSE/V4A/frontmatter parsers. [design]
5. Evals separate from tests: Terminal-Bench adapter + 10 in-repo tasks, run on demand with the real provider, cost recorded in the ledger. §5.3 holds the first recorded run (pending T12.1).

## 6. Fact-check ledger
| # | Claim (report) | Verdict | Correction / source |
|---|---|---|---|
| 1 | ratatui 0.30.2 released 2026-06-19 (D) | confirmed | crates.io |
| 2 | rmcp 3.2.0 released 2026-08-31 (D) | confirmed | crates.io |
| 3 | MCP latest revision 2026-07-28 (E) | confirmed | modelcontextprotocol.io changelog |
| 4 | HTTP+SSE deprecated in 2026-07-28 (E) | confirmed, clarified | deprecated since 2025-03-26; reclassified 2026-07-28 |
| 5–7, 18 | Anthropic prices from finout.io (E) | unverifiable by the checker | plan uses the Claude API reference table cached 2026-06-24: Haiku 4.5 $1/$5, Sonnet 5 $2/$10, Opus 5 $5/$25, Fable 5.1 $10/$50; report E's "Sonnet promo ends Sept 1, then $3/$15" is unconfirmed → T1.7 re-verifies from the official pricing page |
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
