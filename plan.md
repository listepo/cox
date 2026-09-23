# cox

https://github.com/listepo/cox

A modular terminal coding agent in Rust (coxswain: steers work while models, tools, and extensions row). TUI, headless, ACP, MCP.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T22.2 | done | P0 | 2 | 0% | Claude Code / claude-sonnet-5 |
| T22.4 | todo | P1 | 2 | 0% | |
| T22.7 | done | P1 | 1 | 0% | |
| T23.2 | todo | P1 | 2 | 0% | |
| T23.3 | todo | P1 | 2 | 0% | |
| T23.4 | todo | P2 | 1 | 0% | |
| T23.5 | todo | P0 | 2 | 0% | |
| T23.6 | todo | P3 | 1 | 0% | |
| T23.7 | todo | P2 | 3 | 0% | |
| T24.3 | todo | P1 | 1 | 0% | |
| T24.6 | todo | P1 | 2 | 0% | |
| T24.7 | todo | P2 | 2 | 0% | |
| T24.8 | done | P1 | 1 | 0% | |
| T25.2 | todo | P0 | 2 | 0% | |
| T25.3 | todo | P1 | 2 | 0% | |
| T25.5 | todo | P1 | 3 | 0% | |
| T25.6 | todo | P1 | 2 | 0% | |
| T25.8 | todo | P2 | 2 | 0% | |
| T26.4 | todo | P2 | 1 | 0% | |
| T27.2 | todo | P1 | 2 | 0% | |
| T27.4 | todo | P3 | 2 | 0% | |
| T28.1 | todo | P1 | 2 | 0% | |
| T28.2 | done | P2 | 1 | 0% | |
| T28.4 | done | P1 | 2 | 0% | Claude Code / claude-sonnet-5 |
| T29.2 | todo | P2 | 1 | 0% | |
| T30.1 | todo | P2 | 2 | 0% | |
| T30.2 | done | P2 | 2 | 0% | Claude Code / claude-haiku-4-5 |
| T30.3 | todo | P2 | 2 | 0% | |

## Reference

Name: **cox** — the coxswain steers the boat and calls the strokes; the crew (models, tools, MCP servers) does the rowing. Binary `cox`, crates `cox-*`, home `~/.cox/`.

How to read this file: §0 decisions are settled; §1 is the design every task must conform to (types, schemas, algorithms, surfaces); §2 is how a task is worked; numbered tasks that are finished live in `done.md`; later approved work is in `roadmap.md`; §6 amendments; §7 risks. Active work, when any, is the table at the top of this file.

## 0. Decisions (read before any task)

| # | Decision | Why (evidence in research.md) |
|---|----------|-------------------------------|
| D1 | **One Cargo workspace, one static binary, ten in-tree crates (§1). No WASM or dylib plugin host in v0.1.** Extensibility in v0.1 is *data and processes*: instruction files, `SKILL.md`, command and subagent markdown, hook subprocesses, MCP servers. A WASM host (extism) is v0.2. | Claude Code, Codex, Gemini CLI and Copilot all reach their ecosystems through markdown + hooks + MCP, not through in-process plugins (R§2). A plugin ABI is the one thing that cannot be changed later; defer it until the `Tool`/`Event` contract has survived a release. |
| D2 | **The core is a pure state machine: `Submission` in, `Event` out.** `cox-core` owns turns, context assembly, permissions, routing, compaction. It never touches the network, filesystem or a process except through traits defined in `cox-protocol`. TUI, `stream-json`, ACP and the JSONL rollout are four consumers of one event stream. | Codex's SQ/EQ protocol is the reason it ships a TUI, an `exec` mode, an app-server for IDEs and an MCP server from one core (R§1.2). It is also what makes the loop testable without a model: a scripted provider plus a golden event log. |
| D3 | **Own thin provider layer; no LLM framework crate.** `cox-provider` implements the Anthropic Messages API (streaming, tool use, `cache_control`, adaptive thinking, `effort`, `fallbacks`, `count_tokens`), the OpenAI Responses API, and OpenAI Chat Completions (Ollama, vLLM, LM Studio, llama.cpp, OpenRouter, DeepSeek). SSE via `eventsource-stream`. | rig/genai lag the wire formats that decide cost: cache breakpoints, thinking-block replay, server tools, per-message effort, refusal fallbacks (R§4.3). Each provider is ~500 LOC; a framework is a dependency on someone else's release cadence. Codex hand-rolls its client too and ships `eventsource-stream 0.2.3` (R§1.3). |
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

Deferred to **v0.2+** (not rejected): WASM plugin host (extism 1.30); LSP client (diagnostics into context); Gemini provider; image input and `ratatui-image`; git worktree isolation for subagents; web search provider abstraction beyond Anthropic server tools; A2A; voice; `gix` instead of shelling out to `git`; aider-style repo map with PageRank; two-model architect/editor mode.

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
| `cox` | clap surface, dispatch, `doctor`, `config`, `stats`, `expand`, `record`, `sessions`, `self update` | clap 4.6, figment, toml_edit 0.25, anyhow, dotenvy 0.15 |
| `cox-protocol` | `Submission`, `Event`, `Item`, `ToolCall`, `ToolResult`, `Usage`, `Config`, traits `Provider`, `Tool`, `Store`, `Hook` | serde, serde_json, schemars 1, thiserror 2 |
| `cox-core` | `Session` state machine, turn loop, context assembly, cache breakpoints, permission `Engine`, `Router` (job → tier → model), compaction, budget, subagent spawning | tokio 1, tracing 0.1, globset (permission path rules, T2.2) |
| `cox-provider` | Anthropic Messages; OpenAI Responses; OpenAI Chat; `Scripted`; `Replay`; usage extraction; retry/backoff; token estimate | reqwest 0.12 (rustls), eventsource-stream 0.2.3, tiktoken-rs 0.12 |
| `cox-tools` | `read`, `grep`, `glob`, `edit`, `apply_patch`, `write`, `bash`, `todo`, `ask_user`, `agent`, `tool_search`, `web_fetch`, `expand`; `path::confine`; `sandbox::{seatbelt,bwrap,landlock}` | ignore 0.4.33, grep-searcher 0.1.17, globset, nucleo 0.5, similar 3.2, diffy 0.5, tree-sitter 0.25 + bash/rust/typescript/python/go grammars, shlex, landlock 0.4.7, seccompiler 0.5, nix |
| `cox-mcp` | MCP client (stdio, Streamable HTTP, OAuth), server discovery (`.mcp.json`, config), tool namespacing `mcp__<server>__<tool>`, `cox mcp` server | rmcp 3.2 (`client`, `server`, `auth`, `transport-io`, `transport-child-process`, `transport-streamable-http-client-reqwest`), async-trait (server tools as `Tool` impls, T7.6), keyring 4 (OAuth tokens as `cox/mcp/<server>`, T22.5), reqwest 0.13 (the version rmcp implements its HTTP client trait for; the workspace row stays 0.12 for the providers) |
| `cox-store` | `~/.cox/cox.db` Diesel models, `schema.rs`, embedded migrations, rollout writer/reader, archive, FTS5 search (`sql_query`), ledger queries | diesel 2.2 (`sqlite`, `returning_clauses_for_sqlite_3_35`, `r2d2` off), diesel_migrations 2.2, libsqlite3-sys 0.30 (`bundled`), directories 6, keyring 4 |
| `cox-ext` | instruction-file hierarchy, `SKILL.md`, commands, subagent definitions, hook runner (Claude JSON protocol), `.claude/settings.json` import | serde_yaml (frontmatter), shlex, tokio + nix `signal` (hook runner: `sh -c` with a process-group kill on timeout, T7.4), regex 1 (hook `matcher` regexes, T22.3) |
| `cox-tui` | TEA app, composer (tui-textarea-2 0.13, the ratatui-0.30 fork of tui-textarea 0.7), transcript cells, streaming markdown (pulldown-cmark 0.13 → spans; the plan said 0.10, same Tag/TagEnd API), syntect 5 highlighting, diff view, approval modal, status line, `/` commands, `@` file picker, `text::sanitize`, OSC 11 background detection for `tui.theme = "auto"` (T22.6), theme files and `/theme` (T24.2) | ratatui 0.30.2, crossterm 0.29, nucleo 0.5, pulldown-cmark 0.13, syntect 5.3 (fancy-regex, no onig), unicode-width 0.2, arboard 3, terminal-colorsaurus 1.0, toml_edit 0.25, similar 3.2 (word diffs, the approval modal's proposed edit — T24.5) |
| `cox-acp` | Agent Client Protocol 2.0 server: session/prompt, permission requests, client fs/terminal | agent-client-protocol 2.0 |

Dev-deps (workspace): insta 1.48, proptest 1.11, wiremock 0.6, rstest 0.26, assert_cmd 2, predicates 3, assert_fs, tempfile 3, pretty_assertions, vt100 0.16, portable-pty 0.9, libfuzzer-sys 0.4 (fuzz crate only); tools: cargo-nextest, cargo-deny, cargo-audit, cargo-insta, cargo-dist, cargo-fuzz (nightly job only).

Dependency direction (enforced by a test in T0.1 that parses `cargo metadata`): `cox` → everything; `cox-tui`, `cox-acp` → `cox-core`, `cox-protocol`; `cox-core` → `cox-protocol` only; `cox-provider`, `cox-tools`, `cox-mcp`, `cox-store`, `cox-ext` → `cox-protocol` only. No crate below `cox` depends on `cox-core`.

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
| `Tab` | cycle permission mode default → plan → auto | `Ctrl+O` | transcript overlay (full scrollback, search `/`) |
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

1. `prefix_bytes_identical_between_turns` (T2.3) · 2. `truncate_is_lossless_via_archive` (T2.5) · 3. `all_tool_results_return_in_one_message` (T2.1) · 4. `deny_beats_allow` (T2.2) · 5. `compaction_keeps_last_two_turns_verbatim` (T8.1) · 6. `resume_builds_identical_request` (T2.4) · 7. `no_event_after_turn_done` (T2.1) · 8. `every_request_has_a_usage_row` (T1.7) · 9. `think_requires_confirmation` (T9.1) · 10. `broken_hook_is_skipped_not_fatal` (T7.4) · 11. `sandbox_denies_write_outside_workspace` (T4.1/T4.2) · 12. `every_flag_has_a_config_key` (T0.3) · 13. `no_crate_below_cox_depends_on_core` (T0.1) · 14. `sanitize_strips_escapes` (T5.6).

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

#### T21.1 JevProvider type-1 native

Model: - · Status: done 2026-09-20 · Depends: T21.0 · Size: ~550
Goal: the System One wire format behind the `Provider` trait: `build_body` (Request → `{state, model, questions}`), `parse_response` (one `{answers, usage}` body → JSON TextDelta + Stop + Usage), `http_error` (shared taxonomy), `JevProvider` client (bearer key, retry, cancel).
Files: `crates/cox-provider/src/jev.rs`, `crates/cox-provider/src/lib.rs`, `crates/cox-protocol/src/types.rs` (`ProviderId::Jev`), `crates/cox-protocol/src/config.rs` (`JevProviderConfig` + `providers.typesafe`), `crates/cox-protocol/default.toml`, `crates/cox-provider/prices.toml` (`jev-latest` $0.042/0), `docs/config.md` (regenerated).
Steps: 1. pure translator + parsers with 7 unit tests (choice/score/noul, empty-answers-is-Parse, unknown-kind-is-Parse, error taxonomy); 2. thin non-SSE client over `retry::stream_with_retry`; 3. config section + prices row + regenerated docs.
Check: `mise exec -- cargo nextest run -p cox-provider jev` — 7 passed.
Done when: the wire shape is proven without a key; no caller routes to it yet (that is T21.2).
Out of scope: any `cox-core`/`cox` call site (judge layer is T21.3).

Check output:
```
$ mise exec -- cargo nextest run -p cox-provider jev
7 tests run: 7 passed, 0 skipped
```

#### T21.2 Route and build the typesafe provider

Model: - · Status: done 2026-09-20 · Depends: T21.1 · Size: ~30
Goal: `tiers.<t>.provider = "typesafe"` routes (`ProviderId::Jev`, section-model pin like `local`) and builds (`JevProvider::with_key` via the shared key-resolve helper); the ledger names the row `typesafe`.
Files: `crates/cox-core/src/router.rs`, `crates/cox-core/src/session.rs` (`provider_name`), `crates/cox/src/session.rs` (`provider_for`).
Steps: 1. router match arm + pin; 2. ledger name; 3. session builder via `http::resolve_key_env_or_keyring` (missing key is `Auth`, the fail-open path).
Check: `mise exec -- cargo clippy --workspace --all-targets -- -D warnings` exits 0.
Done when: a tier can name `typesafe` end to end; nothing names it by default (all tiers keep their models).
Out of scope: the judge layer that would actually call it (T21.3).

Check output:
```
$ mise exec -- cargo clippy --workspace --all-targets -- -D warnings
Finished `dev` profile
```

#### T21.0 Jev integration scope gate

Model: - · Status: done 2026-09-20 · Depends: - · Size: ~100
Goal: fix what Jev is, where it plugs into cox, and what kills the idea — on paper, before any provider code or dependency.
Files: `docs/design/v0.2-jev.md`, `plan.md`, `roadmap.md`.
Steps: 1. write the guest/host contract sketch (own System One JSON over `POST /v1/systemone`, Python/JS SDKs, no OpenAPI; keys via waitlist at `console.typesafe.ai`); 2. map the candidate call sites (router `pick`, permission classification, compaction/memory salience, skill suggestion) against Jev's three primitives (Choice / Score / Noul) and the cookbook patterns (intent routing, confidence-gated routing, hierarchical classification, skill suggestion, LLM guardrails); 3. name the falsifier that moves it up; 4. move the roadmap line into this card.
Check: `test -f docs/design/v0.2-jev.md && grep -q Falsifier docs/design/v0.2-jev.md`.
Done when: the doc exists (Problem / The field / cox / Falsifiers / Review), states the integration type from §1 (type-1 native wire vs type-2 compatible preset vs data-only like skills/hooks), and the roadmap line is struck through in the branch.
Out of scope: any `cox-provider` code, any new dependency row in §1.1, any key handling (that is the implementation task).

Check output:
```
$ test -f docs/design/v0.2-jev.md && grep -q Falsifier docs/design/v0.2-jev.md && echo ok
ok
```

#### T19.8 Run CI on pull requests

Model: - · Status: done 2026-09-19 · Depends: - · Size: small
Goal: CI runs on `pull_request` to `main`, so release and scoping branches get the signal before merge.
Files: `.github/workflows/ci.yml`.
Steps: 1. add the `pull_request` trigger; 2. keep `workflow_dispatch`; 3. confirm CI green on the branch PR.
Check: `grep -q 'pull_request' .github/workflows/ci.yml`.
Done when: draft PRs show the CI signal that promotes them to ready.
Out of scope: any release workflow change (that is T20.4).

Check output:
```
$ grep -q 'pull_request' .github/workflows/ci.yml && echo ok
ok
```

#### T20.1 release-plz proposes, never tags

Model: - · Status: done 2026-09-19 · Depends: - · Size: small
Goal: `release-plz.yml` runs only `release-pr`; tagging is `release.yml`'s job (publish first, tag last).
Files: `.github/workflows/release-plz.yml`, `release-plz.toml`.
Steps: 1. drop the `release` job, keep `release-pr` with the loud token check; 2. add the pending-release gate; 3. align header comments to the ketch model.
Check: `grep -q 'command: release-pr' .github/workflows/release-plz.yml && ! grep -q 'command: release$' .github/workflows/release-plz.yml`.
Done when: merging to `main` opens or refreshes the release PR and never creates a tag.
Out of scope: building or publishing binaries (that is T20.2–T20.4).

Check output:
```
$ grep -q 'command: release-pr' .github/workflows/release-plz.yml && ! grep -q 'command: release$' .github/workflows/release-plz.yml && echo ok
ok
```

#### T20.2 scripts/package.sh builds the release tarball

Model: - · Status: done 2026-09-19 · Depends: - · Size: ~75 LOC
Goal: one script builds `cox-<target>.tar.xz` per target, used by CI and release alike.
Files: `scripts/package.sh`.
Steps: 1. `cargo build --profile dist --locked --target`; 2. stage binary + README, optional sign; 3. pack with sha256 print.
Check: `bash -n scripts/package.sh`.
Done when: the release `build` job packages through this script.
Out of scope: the cask (that is T20.3).

Check output:
```
$ bash -n scripts/package.sh && echo ok
ok
```

#### T20.3 scripts/cask.sh generates the Homebrew cask

Model: - · Status: done 2026-09-19 · Depends: T20.2 · Size: ~60 LOC
Goal: `Casks/cox.rb` in `listepo/homebrew-tap` is generated, never hand-edited.
Files: `scripts/cask.sh`.
Steps: 1. take `<version> <sha256-aarch64> <sha256-intel>`; 2. validate shas; 3. print the cask.
Check: `bash -n scripts/cask.sh && scripts/cask.sh 0.0.0 $(printf '%064d' 0) $(printf '%064d' 1) | grep -q 'cask "cox"'`.
Done when: the release `tap` job writes the tap file from this script.
Out of scope: the publish itself (that is T20.4).

Check output:
```
$ bash -n scripts/cask.sh && scripts/cask.sh 0.0.0 $(printf '%064d' 0) $(printf '%064d' 1) | grep -q 'cask "cox"' && echo ok
ok
```

#### T20.4 release.yml publishes first, tags last

Model: - · Status: done 2026-09-19 · Depends: T20.1–T20.3 · Size: large
Goal: merging to `main` with an untagged version runs verify → 4-target build → draft release → publish (creates `v<version>`) → tap cask.
Files: `.github/workflows/release.yml`.
Steps: 1. `version` gate; 2. `verify` (same gate as CI); 3. `build` matrix via `scripts/package.sh`; 4. `publish` draft-then-undraft; 5. `tap` cask after publish.
Check: `ruby -ryaml -e "puts YAML.load_file('.github/workflows/release.yml')['jobs'].keys.sort.join(', ')"` prints `build, publish, tap, verify, version`.
Done when: a tag exists iff a release completed; `install.sh` and `cox self update` find only complete releases.
Out of scope: the ketch registry entry (that is T20.5).

Check output:
```
$ ruby -ryaml -e "puts YAML.load_file('.github/workflows/release.yml')['jobs'].keys.sort.join(', ')"
build, publish, tap, verify, version
```

#### T20.5 ketch.toml for cox, dist tail removed

Model: - · Status: done 2026-09-20 · Depends: T20.4 · Size: small
Goal: cox installs via ketch; no dead cargo-dist jobs remain in `release.yml`.
Files: `ketch.toml`, `.github/workflows/release.yml`, `release-plz.toml`, `crates/cox/src/self_update.rs`.
Steps: 1. cut the leftover dist block (252 lines); 2. add `ketch.toml`; 3. copy it to `cox/` in `listepo/ketch-registry`; 4. reword cargo-dist comments.
Check: release jobs list plus `ketch registry validate` passes 5 packages.
Done when: `ketch install cox` picks the platform tarball; registry validates.
Out of scope: merging PR #26 (merge starts the first ketch-model release).

Check output:
```
$ ruby -ryaml -e "puts YAML.load_file('.github/workflows/release.yml')['jobs'].keys.sort.join(', ')"
build, publish, tap, verify, version
$ ketch registry validate /Users/listepo/GitHub/listepo/packages/ketch-registry
validated 5 packages
$ KETCH_ROOT=/tmp/ketch-cox-test ketch install cox --verbose
installed cox v0.1.0
```

#### T20.6 Pin CI toolchain back to 1.97.1

Model: - · Status: done 2026-09-20 · Depends: - · Size: tiny
Goal: CI green after Dependabot #16 bumped the pin to nonexistent `1.120.0` (same class as A22).
Files: `.github/workflows/ci.yml`, `.github/workflows/release-plz.yml`.
Steps: 1. revert four `1.120.0` pins to `1.97.1` (the `mise.toml` pin); 2. push; 3. confirm CI green on PR #26.
Check: `! grep -rn 'rust-toolchain@1.120.0' .github/workflows/ && grep -q 'rust-toolchain@1.97.1' .github/workflows/ci.yml`.
Done when: the CI run on `release/ketch-model` is green.
Out of scope: bumping the real toolchain (moves `mise.toml` + pins together per A22).

Check output:
```
$ ! grep -rn 'rust-toolchain@1.120.0' .github/workflows/ && grep -q 'rust-toolchain@1.97.1' .github/workflows/ci.yml && echo ok
ok
```

#### T20.7 P20 cards and A24 in plan.md

Model: - · Status: done 2026-09-20 · Depends: T20.1–T20.6 · Size: small
Goal: the ketch-model release work is recorded as P20 cards (T19.8, T20.1–T20.6) with amendment A24, so `main` carries its own history.
Files: `plan.md`.
Steps: 1. add P20 section with per-task cards and Check outputs; 2. add A24; 3. verify every Check passes on the branch.
Check: `grep -q '#### T20.6' plan.md && grep -q '^- A24' plan.md`.
Done when: PR #26 merges with the plan describing what it did.
Out of scope: runtime code (P20 is release plumbing + plan records).

Check output:
```
$ grep -q '#### T20.6' plan.md && grep -q '^- A24' plan.md && echo ok
ok
```

#### T20.8 Ulid::generate after Dependabot #18

Model: - · Status: done 2026-09-20 · Depends: - · Size: tiny
Goal: workspace compiles after Dependabot #18 bumped `ulid` 1.2.1 → 3.0.0, which renamed `Ulid::new()` to `Ulid::generate()`.
Files: `crates/cox-protocol/src/ids.rs`.
Steps: 1. call `Ulid::generate()` in the `ulid_id!` macro (wrapper keeps `new()`); 2. clippy + `cox-protocol` tests; 3. push.
Check: `mise exec -- cargo clippy --workspace --all-targets -- -D warnings` exits 0.
Done when: CI `verify`/`Test` pass on the release run.
Out of scope: any id-type rename (callers keep `SessionId::new()`).

Check output:
```
$ mise exec -- cargo clippy --workspace --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s)
$ mise exec -- cargo nextest run -p cox-protocol
49 tests run: 49 passed, 0 skipped
```

#### T19.1 WASM plugins scope gate
Model: opus · Status: done 2026-09-19 · Depends: - · Size: ~60
Goal: the extism host contract is fixed on paper before any code.
Files: `docs/design/v0.2-wasm.md`, `plan.md`, `roadmap.md`.
Steps: 1. write the guest/host contract sketch; 2. name the falsifier that moves it up; 3. move the roadmap line into the P19 card.
Check: `test -f docs/design/v0.2-wasm.md && grep -q Falsifier docs/design/v0.2-wasm.md`.
Done when: the doc exists and the roadmap line is struck through in this branch.
Out of scope: any `extism` dependency (that is the implementation task).

Check output:
```
$ test -f docs/design/v0.2-wasm.md && grep -q Falsifier docs/design/v0.2-wasm.md && echo ok
ok
```

#### T19.2 LSP diagnostics scope gate
Model: sonnet · Status: done 2026-09-19 · Depends: - · Size: ~60
Goal: fix where LSP diagnostics enter the loop (tool vs hook vs core) on paper.
Files: `docs/design/v0.2-lsp.md`, `plan.md`, `roadmap.md`.
Steps: 1. sketch the diagnostics source and its trust boundary; 2. name the falsifier; 3. move the roadmap line into the P19 card.
Check: `test -f docs/design/v0.2-lsp.md && grep -q Falsifier docs/design/v0.2-lsp.md`.
Done when: the doc exists and states the entry point.
Out of scope: any LSP client code.

Check output:
```
$ test -f docs/design/v0.2-lsp.md && grep -q Falsifier docs/design/v0.2-lsp.md && echo ok
ok
```

#### T19.3 Gemini provider scope gate
Model: sonnet · Status: done 2026-09-19 · Depends: - · Size: ~60
Goal: decide whether Gemini is a native wire protocol or a compatible preset (A9 type-1 vs type-2) on paper.
Files: `docs/design/v0.2-gemini.md`, `plan.md`, `roadmap.md`.
Steps: 1. compare Gemini API against Anthropic/OpenAI shapes; 2. pick type-1 vs type-2 with reason; 3. move the roadmap line into the P19 card.
Check: `test -f docs/design/v0.2-gemini.md && grep -q Falsifier docs/design/v0.2-gemini.md`.
Done when: the doc names the integration type and its falsifier.
Out of scope: any provider code.

Check output:
```
$ test -f docs/design/v0.2-gemini.md && grep -q Falsifier docs/design/v0.2-gemini.md && echo ok
ok
```

#### T19.4 Images scope gate
Model: sonnet · Status: done 2026-09-19 · Depends: - · Size: ~60
Goal: fix how image inputs travel Submission → provider (and what v0.1 refuses) on paper.
Files: `docs/design/v0.2-images.md`, `plan.md`, `roadmap.md`.
Steps: 1. sketch the content-block shape per wire format; 2. name the refusal behaviour; 3. move the roadmap line into the P19 card.
Check: `test -f docs/design/v0.2-images.md && grep -q Falsifier docs/design/v0.2-images.md`.
Done when: the doc states the block shape and the v0.1 refusal.
Out of scope: any multimodal code.

Check output:
```
$ test -f docs/design/v0.2-images.md && grep -q Falsifier docs/design/v0.2-images.md && echo ok
ok
```

#### T19.5 Worktrees scope gate
Model: haiku · Status: done 2026-09-19 · Depends: - · Size: ~40
Goal: fix the worktree↔session mapping (one session per worktree?) on paper.
Files: `docs/design/v0.2-worktrees.md`, `plan.md`, `roadmap.md`.
Steps: 1. sketch session→worktree mapping and git tooling reuse; 2. name the falsifier; 3. move the roadmap line into the P19 card.
Check: `test -f docs/design/v0.2-worktrees.md && grep -q Falsifier docs/design/v0.2-worktrees.md`.
Done when: the doc states the mapping.
Out of scope: any worktree automation.

Check output:
```
$ test -f docs/design/v0.2-worktrees.md && grep -q Falsifier docs/design/v0.2-worktrees.md && echo ok
ok
```

#### T19.6 Repo map scope gate
Model: haiku · Status: done 2026-09-19 · Depends: - · Size: ~40
Goal: fix what a repo map contains and where it sits in context assembly on paper.
Files: `docs/design/v0.2-repomap.md`, `plan.md`, `roadmap.md`.
Steps: 1. sketch map contents and budget; 2. place it relative to cache breakpoints; 3. move the roadmap line into the P19 card.
Check: `test -f docs/design/v0.2-repomap.md && grep -q Falsifier docs/design/v0.2-repomap.md`.
Done when: the doc states contents and placement.
Out of scope: any map builder code.

Check output:
```
$ test -f docs/design/v0.2-repomap.md && grep -q Falsifier docs/design/v0.2-repomap.md && echo ok
ok
```

#### T19.7 Architect/editor mode scope gate
Model: sonnet · Status: done 2026-09-19 · Depends: - · Size: ~40
Goal: fix what architect/editor mode changes (tiers, tools, approvals) on paper.
Files: `docs/design/v0.2-modes.md`, `plan.md`, `roadmap.md`.
Steps: 1. sketch the two modes as router/tool presets; 2. name the falsifier; 3. move the roadmap line into the P19 card.
Check: `test -f docs/design/v0.2-modes.md && grep -q Falsifier docs/design/v0.2-modes.md`.
Done when: the doc states the mode table.
Out of scope: any mode switching code.

Check output:
```
$ test -f docs/design/v0.2-modes.md && grep -q Falsifier docs/design/v0.2-modes.md && echo ok
ok
```


### P22 — Trust (goal: every config key, hook event and documented command does what the docs say; evidence in research.md §8.5 #32)

#### T22.4 Mouse: wire `tui.mouse` or delete the key

Model: sonnet · Status: open · Depends: — · Size: ~120 · Priority: P1 · Complexity: 2
Goal: with `tui.mouse = true` the wheel scrolls the transcript overlay and pickers and a click on a folded tool card unfolds it; with `false` the terminal keeps native text selection.
Files: `crates/cox-tui/src/app.rs`, `crates/cox-tui/src/state.rs`, `crates/cox-tui/src/view.rs`.
Steps: (1) `app.rs`: `EnableMouseCapture` after raw mode iff `config.tui.mouse`; `DisableMouseCapture` in the restore path (also on panic hook). (2) `Msg::Mouse(MouseEvent)`: `ScrollUp/ScrollDown` → the same scroll path as `PageUp/PageDown` with 3 lines per tick; `Down(Left)` inside the viewport → hit-test the rendered cell rows (`view` records `cell_rows: Vec<(Range<u16>, CellId)>` in `State` during draw) → toggle fold. (3) `Ctrl+Shift+M`-free design: no toggle key; the config key is the switch, documented in `docs/config.md`. (4) If step 2 exceeds the size limit, deliver wheel scrolling only and file the click as a follow-up card in §6.
Check:
```bash
mise exec -- cargo nextest run -p cox-tui update_mouse_wheel_scrolls_overlay update_mouse_click_unfolds_card
mise exec -- cargo nextest run -p cox-tui --test shell pty_no_mouse_capture_when_disabled
```
Done when: the PTY e2e with `tui.mouse = false` sees no `?1000h`/`?1006h` in the output; with `true` the sequences appear once and are disabled on exit.
Out of scope: drag selection inside the TUI (the terminal's own selection covers it when mouse is off).

### P23 — Terminal capabilities (goal: one probe, every feature optional, `doctor` shows the verdict)

#### T23.2 Flicker-free scrollback (`scrolling-regions`)

Model: sonnet · Status: open · Depends: — · Size: ~40 + test · Priority: P1 · Complexity: 2
Goal: `insert_before` scrolls the region above the viewport instead of repainting everything.
Files: `Cargo.toml`, `crates/cox-tui/tests/shell.rs`.
Steps: (1) Add `scrolling-regions` to the ratatui feature list (verified present in 0.30.2, ledger #29). (2) PTY test: stream 40 finished cells through the real binary with the scripted provider and count full-viewport repaints in the vt100 screen diff (a repaint = every viewport row rewritten in one frame); assert ≤ 1 per inserted cell. (3) Record the before/after count in the commit message; if the count does not drop on the vt100 parser, keep the feature off and record why in §6 (falsifier).
Check:
```bash
mise exec -- cargo nextest run -p cox-tui --test shell pty_insert_before_repaints_at_most_once_per_cell
```
Done when: the test passes with the feature on and the number in the commit message is lower than before.
Out of scope: resize handling (T23.7).

#### T23.3 OSC 8 hyperlinks

Model: sonnet · Status: open · Depends: T23.0 · Size: ~120 · Priority: P1 · Complexity: 2
Goal: `path:line` in tool headers and markdown links are clickable when `caps.osc8`; emitted after `text::sanitize`, never from model text.
Files: `crates/cox-tui/src/cells.rs`, `crates/cox-tui/src/markdown.rs`, `crates/cox-tui/src/text.rs`.
Steps: (1) `text.rs`: `pub struct Link { text: String, target: String }` that only the renderer constructs; `sanitize` keeps stripping any OSC 8 that arrives in input. (2) `cells.rs`: tool headers for `read`/`edit`/`write`/`apply_patch`/`grep` wrap the subject in `Link { target: file://<confined absolute path>#L<line> }`; markdown `[text](https://…)` becomes `Link` for `http(s)` only. (3) `view.rs`/`app.rs`: when drawing a `Link` and `caps.osc8`, write `ESC ] 8 ; ; target ST` before and `ESC ] 8 ; ; ST` after the span (a `Span` with a custom marker rendered by the backend hook; ratatui has no widget — implement in the `insert_before` writer and the frame writer, one helper). (4) `tui.hyperlinks = true` config key (default true) as the manual switch.
Check:
```bash
mise exec -- cargo nextest run -p cox-tui osc8_emitted_only_when_supported sanitize_strips_model_osc8
```
Done when: a snapshot rendered with `caps.osc8 = true` contains `\x1b]8;;file://` around a read path and none around any model-supplied text.
Out of scope: opening links from the keyboard (terminals handle the click).

#### T23.4 OSC 52 clipboard

Model: sonnet · Status: open · Depends: T23.0 · Size: ~70 · Priority: P2 · Complexity: 1
Goal: `y` on a cell in the transcript overlay and `Cmd::Copy` copy through the terminal (works over SSH/tmux) when `caps.osc52`.
Files: `Cargo.toml`, `crates/cox-tui/src/app.rs`, `crates/cox-tui/src/state.rs`.
Steps: (1) Enable crossterm's `osc52` feature (verified present in 0.29, ledger #30). (2) `app.rs`: `Cmd::Copy(text)` → `execute!(stdout, CopyToClipboard::to_clipboard_from(text))` when `caps.osc52`, else `Notice(Info, "clipboard: terminal does not support OSC 52")`. (3) `state.rs`: in the `Ctrl+O` overlay, `y` copies the selected cell's plain text (already produced by `cells::cell_lines`) and `Y` the whole transcript; status line flashes `copied` for one tick.
Check:
```bash
mise exec -- cargo nextest run -p cox-tui overlay_y_emits_copy_of_cell
mise exec -- cargo nextest run -p cox-tui --test shell pty_copy_writes_osc52
```
Done when: the PTY e2e sees `\x1b]52;c;` with the base64 of the cell text.
Out of scope: paste (bracketed paste already exists), native clipboard crates.

#### T23.5 Notifications

Model: sonnet · Status: open · Depends: T23.0, T22.3 · Size: ~120 · Priority: P0 · Complexity: 2
Goal: `TurnDone`, `ApprovalRequired` and a question while the terminal is unfocused ring the terminal (OSC 9 or 777 plus BEL); `tui.notify = auto|always|off`.
Files: `crates/cox-tui/src/app.rs`, `crates/cox-tui/src/state.rs`, `config/default.toml`.
Steps: (1) `app.rs`: `EnableFocusChange` when `caps.focus`; `Input::FocusGained/FocusLost` → `Msg::Focus(bool)`. (2) `state.rs`: `Cmd::Notify { title, body }` emitted on the three events when `notify == always` or (`auto` and unfocused). (3) `app.rs` writes `ESC ] 9 ; body BEL` (OSC 777 `notify;title;body` when `TERM_PROGRAM`/`VTE_VERSION` say VTE) followed by `BEL`; the `Notification` hook (T22.3) gets the same payload. (4) `docs/config.md` row and a `doctor` line.
Check:
```bash
mise exec -- cargo nextest run -p cox-tui update_emits_notify_only_when_unfocused
mise exec -- cargo nextest run -p cox-tui --test shell pty_turn_done_writes_osc9
```
Done when: the PTY e2e sees `\x1b]9;` after `TurnDone` with focus lost and nothing with focus held.
Out of scope: OS-native notification daemons.

#### T23.6 OSC 9;4 progress

Model: haiku · Status: open · Depends: T23.0 · Size: ~50 · Priority: P3 · Complexity: 1
Goal: indeterminate progress in the tab or taskbar while a turn runs, cleared on idle, only when `caps.osc9_4`.
Files: `crates/cox-tui/src/app.rs`, `crates/cox-tui/src/state.rs`.
Steps: (1) `Cmd::Progress(Option<u8>)`: `Some(0)` with state 3 (indeterminate) on `TurnStarted`, `None` (state 0) on `TurnDone`/`Error`; approval pending → state 4 (paused). (2) `app.rs` writes `ESC ] 9 ; 4 ; <state> ; <pct> ST`. (3) Config `tui.progress = true`.
Check:
```bash
mise exec -- cargo nextest run -p cox-tui progress_sequence_follows_turn_state
```
Done when: the sequence test passes and the PTY e2e on a terminal without the capability sees no `9;4`.
Out of scope: percentages (a turn has no known length).

#### T23.7 Resize hardening

Model: sonnet · Status: open · Depends: T23.2 · Size: ~80 · Priority: P2 · Complexity: 3
Goal: a resize mid-stream leaves no duplicated or stale lines in scrollback (ratatui #2086 class).
Files: `crates/cox-tui/src/app.rs`, `crates/cox-tui/tests/shell.rs`.
Steps: (1) On `Input::Resize`, set `state.resizing = true`, skip `insert_before` and `draw` until the next tick with a stable size (two identical size reads 16 ms apart), then `terminal.clear()` of the viewport region and a full redraw. (2) Re-measure the inline viewport height (`VIEWPORT_ROWS` clamped to the new height − 2). (3) PTY test resizes 120×40 → 80×24 while a reply streams, then asserts the vt100 scrollback contains each finished cell exactly once.
Check:
```bash
mise exec -- cargo nextest run -p cox-tui --test shell pty_resize_mid_stream_keeps_scrollback_unique
```
Done when: the test passes on macOS and Linux CI.
Out of scope: tmux pane-resize quirks beyond what the vt100 fixture reproduces (documented in `docs/compat.md`).

### P24 — Looks (goal: a reviewer calls it beautiful; every state has a snapshot and an SVG)

#### T24.3 `two-face` syntax set

Model: haiku · Status: open · Depends: — · Size: ~40 · New dependency: `two-face` (needs the creator's approval and a §1.1 row) · Priority: P1 · Complexity: 1
Goal: ~250 languages (TS/TSX, Kotlin, Swift, Zig, TOML, Dockerfile…) for +0.6 MiB; `read`, `mode=outline` output and diffs share the set.
Files: `Cargo.toml`, `crates/cox-tui/src/markdown.rs`, `crates/cox-tui/tests/cells.rs`.
Steps: (1) Replace `SyntaxSet::load_defaults_newlines()` with `two_face::syntax::extra_newlines()` behind the existing `syntax_set()` accessor. (2) Keep `syntect` default-features off (fancy-regex, no onig). (3) Snapshot a `.tsx` and a `Dockerfile` read.
Check:
```bash
mise exec -- cargo nextest run -p cox-tui --test cells tsx_read_is_highlighted
mise exec -- cargo build --release -p cox && ls -l target/release/cox
```
Done when: the two snapshots show highlighting and the release binary grows by less than 1 MiB (number in the commit message).
Out of scope: language auto-detection beyond file extension and first-line shebang.

#### T24.6 Footer hints and `?` help

Model: sonnet · Status: open · Depends: T24.1 · Size: ~120 · Priority: P1 · Complexity: 2
Goal: the composer placeholder row shows 3–5 context-dependent hints; `?` on an empty composer opens the full keymap overlay from the one table that also feeds `/help` and the docs.
Files: `crates/cox-tui/src/view.rs`, `crates/cox-tui/src/modal.rs`, `crates/cox-tui/src/commands.rs`.
Steps: (1) `commands.rs`: `pub const KEYMAP: &[(&str, &str, Context)]` (`key`, `action`, `Idle|Running|Modal|Overlay`) — the single source; `/help` renders it; a doc test asserts `docs/getting-started.md`'s keymap table matches. (2) `view.rs`: placeholder = the first 3–5 entries for the current context (`Enter send · Shift+Tab mode · @ file · / command · ? help` idle; `Esc stop · Ctrl+B background · Ctrl+O transcript` running). (3) `modal.rs`: `Help` overlay listing `KEYMAP` grouped by context, `Esc`/`?` closes; `?` only when the composer is empty (otherwise it is a character).
Check:
```bash
mise exec -- cargo nextest run -p cox-tui help_overlay_snapshot placeholder_hints_follow_context keymap_table_matches_docs
```
Done when: the overlay snapshot exists and the doc test pins `docs/getting-started.md`.
Out of scope: keybinding customisation (T25.5 extends the same table).

#### T24.7 Motion and narrow-width polish

Model: sonnet · Status: open · Depends: T24.1 · Size: ~120 · Priority: P2 · Complexity: 2
Goal: `tui.motion = full|reduced`; markdown tables fall back to `key: value` records under 60 columns; long headers truncate with the glyph-table ellipsis.
Files: `crates/cox-tui/src/cells.rs`, `crates/cox-tui/src/markdown.rs`, `config/default.toml`.
Steps: (1) `reduced`: the spinner is the static `glyphs.busy` glyph, no elapsed-time shimmer, the thinking cell does not animate its fold marker. (2) `markdown.rs`: when a table's natural width exceeds the viewport, render each row as `Header: value` lines separated by a blank line (Codex's fallback). (3) `cells.rs`: headers use `text::truncate` with the ellipsis glyph at `width − 1`.
Check:
```bash
mise exec -- cargo nextest run -p cox-tui --test cells reduced_motion_spinner_is_static table_falls_back_to_records_at_50_columns
```
Done when: both snapshots exist and `docs/config.md` documents `tui.motion`.
Out of scope: tachyonfx-style effects (none planned).

### P25 — Composer and flow (goal: the keys a Claude Code or Codex user already has in their fingers)

#### T25.2 `Shift+Tab` mode cycle and plan-mode view

Model: sonnet · Status: in progress · Depends: T23.1 · Size: ~120 · Priority: P0 · Complexity: 2
Goal: `Shift+Tab` cycles default → plan → auto; `Tab` completes `@`/`/` only; the composer prompt and status line show the mode; in plan mode denied writes render as a dim "planned" line instead of an error card.
Files: `crates/cox-tui/src/state.rs`, `crates/cox-tui/src/view.rs`, `crates/cox-tui/src/cells.rs`.
Steps: (1) Move the mode cycle from `Tab` to `BackTab` (crossterm reports `Shift+Tab` as `KeyCode::BackTab` everywhere, Kitty or not); `Tab` in the composer triggers picker completion when a `@`/`/` token is under the cursor, else inserts nothing. (2) Prompt glyph per mode from the glyph table: `>` default, `▷` plan, `»` auto, `!` bypass, coloured with `theme.mode_*`. (3) `cells.rs`: a `ToolCallDone` whose result is `denied: plan mode` renders `▷ planned: edit src/lib.rs` in `theme.dim` (no error tint). (4) §1.13 table updated.
Check:
```bash
mise exec -- cargo nextest run -p cox-tui --test keys backtab_cycles_modes tab_completes_mention
mise exec -- cargo nextest run -p cox-tui --test cells plan_mode_denial_renders_as_planned_line
```
Done when: snapshots for the three prompts exist and `docs/getting-started.md` says `Shift+Tab`.
Out of scope: a plan document view (the model's plan is ordinary markdown).

#### T25.3 `!` shell line

Model: sonnet · Status: open · Depends: — · Size: ~110 · Priority: P1 · Complexity: 2
Goal: a composer line starting with `!` runs through the `bash` tool (same sandbox, rules and archive) as a user-initiated card; the result reaches the model only with `!!`.
Files: `crates/cox-tui/src/commands.rs`, `crates/cox-tui/src/state.rs`, `crates/cox/src/session.rs`.
Steps: (1) `commands.rs`: `!cmd` → `Action::Shell { cmd, share: false }`, `!!cmd` → `share: true`. (2) `Submission::UserShell { command, share }` (new protocol variant, §1.2 amendment in the commit): the core runs the `bash` tool through the permission engine and sandbox exactly like a model call, emits the usual `ToolCall*` events with `origin: User`, and appends a `UserMessage` with the output only when `share`. (3) The card shows `$ cmd` as its header; `Esc` cancels through the same token.
Check:
```bash
mise exec -- cargo nextest run -p cox-core bang_line_runs_sandboxed_and_stays_out_of_history bang_bang_line_enters_history
```
Done when: the loop scenarios pass and the next request after `!ls` is byte-identical to the one before it (prefix invariant).
Out of scope: an interactive shell (T15.4 completion already helps the line).

#### T25.5 Keybindings file

Model: sonnet · Status: open · Depends: T24.6 · Size: ~160 · Priority: P1 · Complexity: 3
Goal: `~/.cox/keybindings.toml` rebinds any action in the keymap table; Claude Code's `keybindings.json` is imported read-only for the actions that exist in both; conflicts are reported by `doctor`.
Files: `crates/cox-tui/src/keymap.rs` (new), `crates/cox-tui/src/state.rs`, `crates/cox-ext/src/claude_settings.rs`.
Steps: (1) `keymap.rs`: `Action` enum generated from `KEYMAP` (T24.6), `Binding { key, modifiers, context }`, parser for `"ctrl+enter"`, `"shift+tab"`, `"alt+m"`; `Keymap::resolve(KeyEvent, Context) -> Option<Action>`. (2) `state.rs` dispatches through `Keymap` instead of the literal `match` (the literal table becomes the default `Keymap`). (3) `claude_settings.rs`: read `~/.claude/keybindings.json` (`{ "bindings": [{ "key", "command", "when" }] }`) and map the commands cox has (`send`, `newline`, `interrupt`, `mode.cycle`, `transcript`, `help`); unknown commands ignored with a debug log. (4) `doctor`: two actions on one key in one context → warning naming both.
Check:
```bash
mise exec -- cargo nextest run -p cox-tui keymap_parses_chords keymap_rebinds_send claude_keybindings_import_maps_known_commands
```
Done when: rebinding `send` to `ctrl+enter` works in the PTY e2e and `docs/config.md` documents the file.
Out of scope: chords (`ctrl+x ctrl+s`), per-mode vim remaps.

#### T25.6 `/init`

Model: sonnet · Status: in progress · Depends: — · Size: ~140 · Priority: P1 · Complexity: 2
Goal: writes an `AGENTS.md` skeleton for the repo on the `cheap` tier after showing the diff for approval; never overwrites without `--force`.
Files: `crates/cox-tui/src/commands.rs`, `crates/cox-core/src/init.rs` (new), `crates/cox/src/session.rs`.
Steps: (1) `init.rs`: detect manifests (`Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`, `mise.toml`, `justfile`) and derive build/test/lint commands; render a template (`# <name>`, layout table from the top-level directories, commands, "conventions" left as a `cheap`-tier summary of the README if present). (2) `Submission::Command { name: "init" }` → the core runs the `cheap` job `init`, then emits an `ApprovalRequired` for the `write` of `AGENTS.md` (the existing diff modal shows it). (3) `cox init [--force]` subcommand for headless use.
Check:
```bash
mise exec -- cargo nextest run -p cox-core init_detects_cargo_and_just init_refuses_to_overwrite
COX_HOME=/tmp/cox-scratch COX_PROVIDER=scripted mise exec -- cargo run -q -- init --cwd fixtures/init-sample
```
Done when: the fixture repo gets an `AGENTS.md` matching a snapshot and a second run refuses.
Out of scope: rewriting an existing `AGENTS.md`.

#### T25.8 Cross-session prompt history

Model: sonnet · Status: open · Depends: — · Size: ~90 · Priority: P2 · Complexity: 2
Goal: `Ctrl+R` searches the user turns of every session of this project (newest first) through `rollout_fts`.
Files: `crates/cox-tui/src/picker.rs`, `crates/cox/src/session.rs`.
Steps: (1) `Store::rollout_search(project_slug, query, limit)` exists for `cox sessions --grep`; the binary feeds `Kind::History` candidates from it on open and re-queries as the user types (debounced 100 ms through `Msg::Tick`). (2) Rows show `2d ago · <first 80 chars>`; `Enter` inserts the text. (3) Current-session entries come first, unchanged.
Check:
```bash
mise exec -- cargo nextest run -p cox-tui history_picker_lists_other_sessions_after_current
```
Done when: the picker snapshot shows both groups.
Out of scope: a global (cross-project) history.

### P26 — Checkpoints and rewind (goal: `/rewind` that also covers what the shell changed)

#### T26.4 `/undo`, `/redo`

Model: haiku · Status: open · Depends: T26.2 · Size: ~60 · Priority: P2 · Complexity: 1
Goal: aliases for a one-step code-only rewind and its inverse.
Files: `crates/cox-tui/src/commands.rs`, `crates/cox-core/src/session.rs`.
Steps: (1) `/undo` = `Rewind { to_turn: current − 1, code: true, conversation: false }`. (2) `/redo` = restore the checkpoints written *by* the last rewind (they carry `call_id = NULL` and a `rewind` marker row) — one step. (3) Both in the palette and `/help`.
Check:
```bash
mise exec -- cargo nextest run -p cox-core undo_then_redo_is_identity
```
Done when: the test passes and the fold line of a tool card mentions `/undo` after an edit.
Out of scope: multi-step redo history.

### P27 — Agents you can see (goal: no "raw scaffolding noise")

#### T27.2 Approvals labelled by source; agent cards

Model: sonnet · Status: open · Depends: — · Size: ~150 · Priority: P1 · Complexity: 2
Goal: an approval or question says which agent is asking; `/agents` shows one card per live agent instead of a line.
Files: `crates/cox-protocol/src/types.rs`, `crates/cox-tui/src/modal.rs`, `crates/cox-tui/src/picker.rs`.
Steps: (1) `Event::ApprovalRequired` gains `source: Source { session: SessionId, agent: Option<String>, preset: Option<String> }` (subagent sessions forward their approvals to the parent surface already — attach the label there); the ACP and stream-json surfaces emit it as a field. (2) Modal header: `explore-2 asks: bash cargo test` in `theme.agent`; the main session shows no prefix. (3) `/agents` card: name, preset, model, tokens, cost, last tool, elapsed, state (from the T16.1 presence records plus the live task registry); `Enter` on a card opens its rollout read-only in the transcript overlay.
Check:
```bash
mise exec -- cargo nextest run -p cox-tui approval_modal_shows_source_agent agents_cards_snapshot
mise exec -- cargo nextest run -p cox-core subagent_approval_carries_source
```
Done when: the two snapshots exist and `docs/protocol.jsonschema` regenerates with the new field.
Out of scope: talking to an agent mid-task (`@agent` messaging).

#### T27.4 `/loop`

Model: sonnet · Status: open · Depends: T25.1 · Size: ~120 · Priority: P3 · Complexity: 2
Goal: `/loop <interval> <prompt>` repeats a turn on a timer with its own budget cap; `cox run --loop <interval>` for scripts.
Files: `crates/cox-tui/src/commands.rs`, `crates/cox-tui/src/state.rs`, `crates/cox/src/run.rs`.
Steps: (1) `State.loop: Option<Loop { prompt, interval, next_at, budget_usd, spent }>`; `Msg::Tick` enqueues the prompt (T25.1 queue) when due and the session is idle. (2) Status line shows `↻ 4m12s`; `/loop stop` or `Esc` on an empty composer stops it; `budget.session_usd` still applies on top. (3) `cox run --loop 5m -p "…" --max-iterations N` in `run.rs` (headless, exit 0 after N or budget).
Check:
```bash
mise exec -- cargo nextest run -p cox-tui loop_enqueues_when_due_and_idle
mise exec -- cargo nextest run -p cox run_loop_stops_after_max_iterations
```
Done when: both tests pass and `docs/getting-started.md` documents the command.
Out of scope: cloud schedules.

### P28 — Context and cost visibility (goal: the ledger and the routing are visible, not just recorded)

#### T28.1 Status-line segments

Model: sonnet · Status: in progress · Depends: T24.1 · Size: ~120 · Priority: P1 · Complexity: 2
Goal: `ctx` is a mini bar with the cached share in the accent colour; `$` shows spend over the session cap; effort and mode badges; segments drop from the right on narrow terminals in a documented order.
Files: `crates/cox-tui/src/status.rs`, `crates/cox-tui/src/theme.rs`, `docs/getting-started.md`.
Steps: (1) `ctx ▰▰▰▱▱ 41%` where filled cells in `theme.accent` mark cached tokens and `theme.text` uncached (from the last `Usage`). (2) `$0.83/5` (cap from `budget.session_usd`, `theme.warn` above `warn_at`). (3) Badges `[plan]`, `effort:xhigh` when non-default. (4) Drop order at narrow widths: git counts → cache → tasks → effort → model → cost → ctx (documented); a `--plain` variant (T29.1) prints the same text once per turn.
Check:
```bash
mise exec -- cargo nextest run -p cox-tui --test status status_at_60_100_160_columns
```
Done when: three snapshots exist and `docs/screenshots/finished_turn.svg` is regenerated.
Out of scope: a user-scripted status line (Claude Code style) — a later card if asked.

### P29 — Accessibility (goal: usable with a screen reader and without motion)

#### T29.2 Reduced motion and daltonized themes

Model: haiku · Status: open · Depends: T24.2, T24.7 · Size: ~60 · Priority: P2 · Complexity: 1
Goal: `cox-dark-daltonized` and `cox-light-daltonized` theme files ship built in; `tui.motion = reduced` is documented with them.
Files: `crates/cox-tui/src/theme.rs` (embedded theme files), `docs/config.md`, `crates/cox-tui/tests/frames.rs`.
Steps: (1) Two theme files using blue/orange for add/del and ok/error (no red/green pair). (2) Frame snapshot per theme. (3) An "Accessibility" section in `docs/config.md` listing `--plain`, `tui.motion`, the two themes and `NO_COLOR`.
Check:
```bash
mise exec -- cargo nextest run -p cox-tui --test frames daltonized_dark daltonized_light
```
Done when: both snapshots exist and the docs section is present.
Out of scope: a colour-vision simulator.

### P30 — Lean profile and footprint (goal: numbers cox can publish that no vendor does)

#### T30.1 `profile = "minimal"`

Model: sonnet · Status: in progress · Depends: T22.2 · Size: ~140 · Priority: P2 · Complexity: 2
Goal: a config profile whose assembled prefix is ≤ 1 000 tokens; `doctor` prints the prefix token count for the active profile; a test pins the cap.
Files: `config/default.toml`, `crates/cox-core/src/context.rs`, `crates/cox/src/doctor.rs`.
Steps: (1) `[profiles.minimal]`: `context.deferred_tools = true` with the core eight only, `system_prompt = "minimal"` (a second embedded prompt ≤ 300 tokens), `instruction_budget_tokens = 2000`, no skills index, no memory index; `cox --profile minimal` and `core.profile` key. (2) `context::assemble` honours the profile (the prefix layout §1.9 is unchanged — blocks are just smaller or empty). (3) `doctor`: `prefix: 912 tokens (profile minimal)` using the T1.8 estimator. (4) Test `minimal_prefix_under_1000_tokens` over the fixtures workspace.
Check:
```bash
mise exec -- cargo nextest run -p cox-core minimal_prefix_under_1000_tokens prefix_bytes_identical_between_turns
```
Done when: the test passes and `docs/config.md` documents profiles.
Out of scope: automatic profile selection.

#### T30.3 Eval run with a verification step

Model: sonnet · Status: open · Depends: a funded API key · Size: ~100 · Priority: P2 · Complexity: 2
Goal: the T12.1 harness gains a "verify before done" instruction and a `PostToolUse` test-runner hook preset; one Terminal-Bench 2.x run is recorded with its ledger cost.
Files: `evals/run.py`, `evals/hooks/verify.sh` (new), `research.md`.
Steps: (1) Harness system addendum: "before reporting done, run the task's tests and show the output"; (2) hook preset: after `edit`/`apply_patch`/`write` run the project's test command when one is detected (`just test`, `cargo nextest`, `npm test`, `pytest`) with a 120 s cap and feed failures back as `additionalContext`; (3) run the 10 in-repo tasks with and without the preset, then one TB 2.x run; record pass rate, cost, and tokens in `research.md` §5.3.
Check:
```bash
python3 evals/run.py --provider anthropic --model claude-sonnet-5 --preset verify
```
Done when: §5.3 has the table with both configurations and the run's cost from `cox stats`.
Out of scope: leaderboard submission.

## 4. Definition of done for v0.1

1. `cox` runs a multi-turn coding session against Anthropic, OpenAI Responses and a local Ollama model with the same tool set, with the sandbox on, on macOS and Linux.
2. `cargo test --workspace` passes offline with no API key in under 90 s on CI; every widget, transcript cell and loop scenario has a snapshot.
3. A user with `.claude/settings.json`, `CLAUDE.md`, `.claude/commands`, `.claude/agents`, `.mcp.json` and rtok hooks gets identical behaviour without editing them.
4. `cox stats` shows cost by tier and job; the `just bench` table in `research.md` §4.6 shows measured savings for each D6 mechanism; cache-read ratio on turn ≥ 3 of a typical session is ≥ 80 %.
5. `cox run -p` and `cox acp` pass their conformance tests; `cox mcp` serves `read`/`grep`/`glob` to Claude Code.
6. No `unwrap`/`panic!` outside tests; `cargo deny` clean; fuzz jobs green.
7. The fourteen invariants in §1.15 each have a passing, named test.

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
