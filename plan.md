# cox — implementation plan and roadmap for a modular Rust TUI coding agent

Status: plan v2, 2026-09-02 (v1 expanded: full protocol types, config schema, storage schema, permission algorithm, context layout, compaction algorithm, tool catalogue, CLI and TUI surfaces, error taxonomy, per-task Goal/Files/Steps/Check/Done-when, dependency graph, risk register). Nothing implemented yet; `Cargo.toml` does not exist until T0.1. Companion evidence: `research.md` (competitor dissection, crate survey, fact-check ledger) and `report.html` (the same for humans). Agent instructions: `AGENTS.md` (`CLAUDE.md` is a symlink). Rust 1.97.1 is pinned in `mise.toml`; run cargo as `mise exec -- cargo …`. Finished tasks move to `done.md` verbatim with their Check output.

Name: **cox** — the coxswain steers the boat and calls the strokes; the crew (models, tools, MCP servers) does the rowing. Binary `cox`, crates `cox-*`, home `~/.cox/`.

How to read this file: §0 decisions are settled; §1 is the design every task must conform to (types, schemas, algorithms, surfaces); §2 is how a task is worked; §3 is the task list, one block per task; §4 is done; §5 is the roadmap; §6 amendments; §7 risks.

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
| D9 | **One SQLite file plus human-readable rollouts, through a sync ORM.** `~/.cox/cox.db` (Diesel 2.2 `sqlite` + bundled `libsqlite3-sys` 0.30 with FTS5, WAL): sessions, usage ledger, tool-output archive index, memory. Typed Diesel models and `schema.rs`; migrations embedded with `diesel_migrations`; FTS5 virtual tables via `diesel::sql_query` (Diesel cannot model `VIRTUAL TABLE`). `cox-store` is the only crate that contains SQL. Each session is also `~/.cox/sessions/<id>.jsonl` — the event stream itself — used for resume, replay tests and export. Archived payloads over 16 KiB live under `~/.cox/archive/`. | Same choice as rtok D13 (user request, 2026-09-02): typed models make the ledger queries (`stats`, budget, cache diagnostics) joins instead of hand-written SQL, and Diesel is sync, so hooks, tests and `cox stats` need no async runtime. Async ORMs (SeaORM, SQLx) would need a runtime per hook. Codex stores rollouts as JSONL; Claude Code uses JSONL; engram/claude-mem converge on SQLite+FTS5 (R§1.5). |
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

| Crate | Owns | Key deps (pinned in T0.1; versions verified 2026-09-02, R§4.5) |
|-------|------|------|
| `cox` | clap surface, dispatch, `doctor`, `config`, `stats`, `expand`, `record`, `sessions`, `self update` | clap 4.6, figment, toml_edit 0.25, anyhow, dotenvy 0.15 |
| `cox-protocol` | `Submission`, `Event`, `Item`, `ToolCall`, `ToolResult`, `Usage`, `Config`, traits `Provider`, `Tool`, `Store`, `Hook` | serde, serde_json, schemars 1, thiserror 2 |
| `cox-core` | `Session` state machine, turn loop, context assembly, cache breakpoints, permission `Engine`, `Router` (job → tier → model), compaction, budget, subagent spawning | tokio 1, tracing 0.1, globset (permission path rules, T2.2) |
| `cox-provider` | Anthropic Messages; OpenAI Responses; OpenAI Chat; `Scripted`; `Replay`; usage extraction; retry/backoff; token estimate | reqwest 0.12 (rustls), eventsource-stream 0.2.3, tiktoken-rs 0.12 |
| `cox-tools` | `read`, `grep`, `glob`, `edit`, `apply_patch`, `write`, `bash`, `todo`, `ask_user`, `agent`, `tool_search`, `web_fetch`, `expand`; `path::confine`; `sandbox::{seatbelt,bwrap,landlock}` | ignore 0.4.33, grep-searcher 0.1.17, globset, nucleo 0.5, similar 3.2, diffy 0.5, tree-sitter 0.25 + bash/rust/typescript/python/go grammars, shlex, landlock 0.4.7, seccompiler 0.5, nix |
| `cox-mcp` | MCP client (stdio, Streamable HTTP, OAuth), server discovery (`.mcp.json`, config), tool namespacing `mcp__<server>__<tool>`, `cox mcp` server | rmcp 3.2 (`client`, `server`, `auth`, `transport-io`, `transport-child-process`, `transport-streamable-http-client-reqwest`), async-trait (server tools as `Tool` impls, T7.6) |
| `cox-store` | `~/.cox/cox.db` Diesel models, `schema.rs`, embedded migrations, rollout writer/reader, archive, FTS5 search (`sql_query`), ledger queries | diesel 2.2 (`sqlite`, `returning_clauses_for_sqlite_3_35`, `r2d2` off), diesel_migrations 2.2, libsqlite3-sys 0.30 (`bundled`), directories 6, keyring 4 |
| `cox-ext` | instruction-file hierarchy, `SKILL.md`, commands, subagent definitions, hook runner (Claude JSON protocol), `.claude/settings.json` import | serde_yaml (frontmatter), shlex, tokio + nix `signal` (hook runner: `sh -c` with a process-group kill on timeout, T7.4) |
| `cox-tui` | TEA app, composer (tui-textarea-2 0.13, the ratatui-0.30 fork of tui-textarea 0.7), transcript cells, streaming markdown (pulldown-cmark 0.13 → spans; the plan said 0.10, same Tag/TagEnd API), syntect 5 highlighting, diff view, approval modal, status line, `/` commands, `@` file picker, `text::sanitize` | ratatui 0.30.2, crossterm 0.29, nucleo 0.5, pulldown-cmark 0.13, syntect 5.3 (fancy-regex, no onig), unicode-width 0.2, arboard 3 |
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
    Compacted     { summary: ItemId, dropped: Vec<ItemId>, before_tokens: u32, after_tokens: u32 },
    TaskCreated   { task: TaskId, label: String, tier: Tier },
    TaskCompleted { task: TaskId, result_item: ItemId, cost_usd: f64 },
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

Prices for the ledger (Anthropic first-party, from the Claude API reference cached 2026-06-24; re-verify in T1.7): Haiku 4.5 $1/$5, Sonnet 5 $2/$10, Opus 5 $5/$25, Fable 5.1 $10/$50 per MTok; cache write 1.25×, cache read 0.1× of input (Fable 5.1 cache read $0.25/MTok). `config/prices.toml` carries `verified_on` per row; a row older than 90 days makes `cox doctor` warn.

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

Trigger: after `TurnDone`, when `context_tokens_last_call ≥ context.compact_at × max_context`, or on `/compact [focus]`, or when a provider returns a context-length error (then compaction runs before retrying once).

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
| `bash` | `command`; `timeout_s` (120); `background: bool` | Exec / Destructive (classified) | streamed stdout+stderr, exit code | sandboxed; env allowlist; cwd = workspace |
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
| `Enter` | send | `Shift+Enter` / `Alt+Enter` | newline |
| `Esc` | interrupt turn / close modal | `Ctrl+C` ×2 within 1 s | quit |
| `Tab` | cycle permission mode default → plan → auto | `Ctrl+O` | transcript overlay (full scrollback, search `/`) |
| `Ctrl+T` | toggle thinking visibility | `Ctrl+E` | expand last tool output |
| `@` | file picker (nucleo) | `/` at line start | command palette |
| `y` / `s` / `n` / `e` in approval modal | allow / allow for session / deny / edit command | `Ctrl+R` | prompt history search |
| `PageUp/PageDown`, mouse wheel | scroll transcript | `Ctrl+L` | redraw |

Slash commands (parsed in the surface, executed as `Submission::Command`): `/model [tier] [model]`, `/think <prompt>` (confirm dialog with price), `/compact [focus]`, `/cost`, `/permissions`, `/sandbox <mode>`, `/resume`, `/sessions`, `/expand <id>`, `/agents`, `/skills`, `/hooks`, `/mcp`, `/doctor`, `/clear` (new session, same cwd), `/vim`, `/help`, `/quit`. Markdown files in `.claude/commands` and `.cox/commands` appear in the same palette (T7.3).

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

Work only on `main` — no `cox/<task-id>` or other task branches. Commit `<task-id>: <title>` on `main`; any new dependency needs a row in §1.1 and a reason in the commit. If the Check cannot pass without exceeding the size limit, split the task with an amendment in §6 and do the first half. Skipped or failing steps are reported in `done.md`, never silently.

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

### P13 — Observability (goal: agent traces and logs in any OTLP backend)

#### T13.3 Observability documentation and smoke stack
Model: haiku · Status: open · Depends: T13.2 · Size: ~120
Goal: a user can view cox data in SigNoz, Jaeger, Grafana/Tempo, or any OTLP-compatible service without code changes.
Files: `docs/observability.md`, `website/content/docs/observability.md`, `docker-compose.telemetry.yml`.
Steps: (1) Document standard OTEL variables, secure content capture, resource naming and backend endpoint examples. (2) Provide a local Collector + Jaeger + Grafana/Tempo smoke stack. (3) Link from README and Hugo navigation. (4) Verify emitted spans with the stack and record the commands.
Check:
```bash
docker compose -f docker-compose.telemetry.yml config && test -f docs/observability.md
```
Done when: one scripted cox run appears in Jaeger and Grafana with its session → provider → tool hierarchy.

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
| v0.2 | — | WASM plugins (extism), LSP diagnostics, Gemini, images, worktrees, repo map, architect/editor mode | — |

Order of value if time is short: M1 → M2 → P8 (T8.1–T8.3) → P6 → P7 → the rest. M4 before M3 if cost is the pain; M3 before M4 if adoption is.

## 6. Plan amendments

- A1 2026-09-02 D9, §1.1, §1.7, T0.4 — Diesel 2.2 (sqlite, bundled libsqlite3-sys with FTS5, diesel_migrations) replaces rusqlite as the store layer, matching rtok D13. Why: user request; typed models for ledger joins; sync so hooks and tests need no runtime. Effect on other tasks: T1.7, T8.3, T8.4, T10.1, T10.3 write Diesel queries in `cox-store`, never SQL elsewhere.
- A2 2026-09-02 §2, `AGENTS.md` — agents work only on `main`; no `cox/<task-id>` branches. Why: user request. Effect: claim, commit and finish tasks on `main`.
- A3 2026-09-02 §2, `AGENTS.md` — don't duplicate code or logic; reuse an existing helper or extract one shared helper at the responsible layer. Why: user request.
- A4 2026-09-02 §2, `AGENTS.md` — every implemented task is marked done and moved to `done.md` with its Check output. Why: user request.
- A5 2026-09-02 §1.2, `cox-protocol::Tool`, T3.5 — added `Tool::risk(&self, input) -> Risk`, defaulting to `spec().risk`; `cox-core::turn::run_tools` now asks the tool instead of reading `spec().risk`. Why: T3.5 step 4 and the §4 tool table require `apply_patch` to be `Destructive` only when a patch deletes > 5 files, and a static `ToolSpec` cannot express a per-call risk. Effect on other tasks: none — every other tool inherits the default; T2.2's permission engine keeps reading `ToolCall.risk`.
- A6 2026-09-02 D13, §1.1, §1.6, T0.7 — `dotenvy` 0.15 on `cox` loads `.env` then `.env.local` from cwd (walk up) into unset process env before figment. Why: user request (local API keys / `COX_*` without a second config file). Effect: not a figment layer and does not override set variables, so D12 tests and CI keep winning; provenance stays `env`. `cox-core` does not take this dep.
- A7 2026-09-02 `website/` — added a standalone Hugo documentation site using Tailwind CSS (modern home page plus architecture and configuration references). Why: user request. Effect: no runtime crate or release behaviour changes; publish with `hugo --source website`.
- A8 2026-09-02 `website/`, `.github/workflows/deploy-pages.yml` — deploy the Hugo site to GitHub Pages from `main`, building within `website/` and publishing `website/public`. Why: user request. Effect: the deploy workflow installs the pinned Tailwind dependencies and runs only when site/workflow files change.
- A9 2026-09-04 §1.1, §1.6, D3 — provider registry in two types (`docs/design/providers.md`). Type-1 (native `Provider` impl per *wire protocol*): `AnthropicProvider`, new `OpenAiResponsesProvider` (`POST /responses`, bearer-optional), `OpenAiChatProvider` (+`models` list, `from_parts`). Type-2 (compatible, zero code): any `[providers.<name>]` table (`CompatibleProviderConfig`: `base_url`, `api_key_env`, `api`, `model`, `context_window`, `models`) served by the shared Chat/Responses clients; seed `deepseek`, `openrouter` (curated), `moonshot`, `z-ai` with per-model `{id, context_window, efforts}` from models.dev and matching `prices.toml` rows (20 rows; `qwen3-coder` costed 0). Why: user request — adding DeepSeek/OpenRouter must be config lines, never a `DeepseekProvider` struct duplicating the Chat client. Effect: `ProvidersConfig` loses `deny_unknown_fields` (flattened `custom` map, Hooks/Mcp precedent — a typo'd table parses but fails closed in router/session); `Router::pick` accepts custom names (Local family id, section-model pin, per-model effort clamp to greatest-supported-≤-request); `provider_for` builds Responses-or-Chat per `api` (unknown `api` bails at startup); `--provider <name>` propagates to all tiers for any non-first-party name; `cox-provider::http` unifies key resolve/error mapping (5xx is now `Overloaded` on every backend); `Effort` gains `Ord`; `figment` becomes a `cox-protocol` dev-dep for the default.toml shape test. Deferred: per-model effort *enforcement* beyond the clamp (gateway models pass through), keyring fallback for custom keys (env-only), shared retry policy for OpenAI-shaped clients, `cox doctor` prices-age check.
- A10 2026-09-04 D16, P13 — implement vendor-neutral OpenTelemetry observability as three bounded tasks: OTLP/HTTP traces+logs exporter, GenAI semantic instrumentation, then backend documentation/smoke stack. Why: user requested full AI-agent telemetry visible in Maple, SigNoz, Jaeger and Grafana. Effect: standard OTEL environment variables remain the portability contract; raw prompt/completion/tool content is opt-in only because it can contain source code and secrets; operational metadata, usage, costs and errors are always exported when telemetry is enabled.

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
