# Design: cox for macOS — a native desktop client over the cox crates (A67, phase P37)

Status: **proposal, not approved.** Nothing here is in `plan.md` yet; §12 is the
draft amendment for the creator. Evidence: `research.md` §9 (cited R9.n).
Section references inside this file are DT§n.

## 0. Decisions at a glance

| # | Decision | Why |
| --- | --- | --- |
| DT-1 | macOS only, SwiftUI with AppKit where SwiftUI is too slow, minimum macOS 26, Apple Silicon only (creator, 2026-09-28) | Creator's scope. macOS 26 brings Liquid Glass, SwiftUI `WebView`, `TextEditor` over `AttributedString` (R9.3.11) — no back-deployment shims |
| DT-2 | Rust is linked **in-process** as a static library through **UniFFI** | Only maintained binding with async fn ↔ Swift `async` and Swift-implemented traits, proven by Element X, Bitwarden and Firefox (R9.3.1–9.3.6). An XPC helper or an app-server subprocess adds an IPC hop on the hottest path for no security gain: cox already sandboxes every shell command itself (R9.4.10) |
| DT-3 | **Rust owns the view model.** Swift renders and integrates with the OS; it never decides | "Separate view from business logic" taken literally: folding events into a transcript, costs, approvals, command parsing, markdown parsing and syntax highlighting are Rust, shared with the TUI. Swift gets ready-to-draw blocks and applies keyed patches (the matrix-rust-sdk timeline model, R9.3.8) |
| DT-4 | Three new crates: `cox-session` (assembly), `cox-app` (UI-agnostic app core), `cox-ffi` (UniFFI, the only crate that knows Swift exists) | Session assembly is stuck in the binary with `anyhow`, `&Cli` and `eprintln!` (R9.4.8); the TUI's fold logic is tied to ratatui. Both move to crates every surface can share |
| DT-5 | Same `~/.cox` home, same `cox.db`, same rollouts as the CLI | A session started in the terminal opens in the app and back. No second store |
| DT-6 | Developer ID + notarization + Hardened Runtime, **no App Sandbox**, Sparkle 2 updates, the `cox` CLI bundled inside the app | An App-Sandboxed host cannot nest `sandbox-exec` (R9.4.10) or read arbitrary repositories; Mac App Store is out of scope |
| DT-7 | The app is a sixth consumer of the one `Event` stream (D2) | No new core protocol. The gaps the GUI hits (DT§4.7) are fixed in `cox-protocol` for every surface |

## 1. Problem, goals, falsifiers

**Problem.** cox has four surfaces (TUI, `run -p`, ACP, MCP) and no GUI. Every
serious competitor now ships a desktop client (R9.1), and nearly all of them
are Electron or VS Code forks (R9.2). The creator wants a macOS client that
uses part of cox as a library and is better than the others.

**Goals.**

1. Everything the TUI can do, with no logic re-implemented in Swift.
2. Win on what the field is weak at (R9.2): responsiveness and memory,
   cost you can see, approvals you do not rubber-stamp, work you cannot lose,
   worktrees that do not rot on disk.
3. Keep the core's rules: pure state machine, lossless, cache-stable prefix,
   cost ledger, fail-open extensions.

**Non-goals (v1).** Windows/Linux GUI; Mac App Store; a code editor (the app
opens files in the user's editor); cloud execution; iOS.

**Budgets — the falsifiers.** If the shipped M1 misses any of these on an
M1 MacBook Air with 8 GB, the native-first argument failed and the design is
reopened:

| Metric | Budget | How measured |
| --- | --- | --- |
| Cold launch to an interactive window | ≤ 400 ms | `XCTApplicationLaunchMetric` |
| Idle memory, one open session of 2 000 blocks | ≤ 150 MB RSS | `XCTMemoryMetric` |
| Streaming 200 tokens/s | main thread busy ≤ 25 %, no hitch > 16 ms | signposts + SwiftUI Instruments template |
| Scrolling a 10 000-block transcript | ≤ 1 % hitch time | `XCTOSSignpostMetric.scrollDecelerationMetric` |
| Keypress to glyph in the composer | ≤ 16 ms | Instruments |
| Core never stalls on the UI | a UI that stops reading for 10 s does not delay a turn | Rust test in `cox-app` (DT§4.5) |

## 2. Where cox wins

The field converged (R9.2): session board, worktrees, diff review, plan mode,
MCP. Those are table stakes and are all in M1. The client wins on six things
that already exist in cox's core and that the competitors lack or hide:

| Edge | What the user sees | cox building block |
| --- | --- | --- |
| **Native speed** | Launches instantly, stays small, scrolls a 10k-block session smoothly | SwiftUI/AppKit, Rust in-process (DT-2) |
| **Cost you can see** | Live `$` and cache-hit % per turn, session, project; a budget cap that stops a turn before it overspends | `Event::Usage`, the cost ledger, `budget::decide` |
| **Approvals that carry information** | Every prompt says *why* (the rule or risk), shows the exact grant "allow for session" will add, lets you *edit* the command before it runs; one inbox for all sessions; allow/deny from a notification | `Why`, `why_text`, `grants_for`, `Decision::Edit`, `ApprovalRequired.source` |
| **Nothing is lost** | A truncated output expands in place; every tool call is a checkpoint; a timeline rewinds code, conversation, or both | `ArchiveRef` + `cox expand`, `Checkpoint`, `Submission::Rewind{code, conversation}` |
| **Any model** | Anthropic, OpenAI, OpenRouter, Ollama, LM Studio, vLLM; tier routing visible per turn | `cox-provider`, `cox-models`, `TurnStarted{tier, model}` |
| **Worktrees that clean up** | Each session's worktree with its disk size, merged/stale state and one-click prune | `Worktrees` trait, `GitWorktrees` |

Later (M2/M3) edges from R9.2: an ACP host that renders Claude Code, Codex and
Cursor (P35) sessions in the same UI (Zed's and JetBrains Air's model), a
browser pane the agent drives, best-of-n across models, plugin panels drawn
natively from the WASM widget tree.

## 3. Feature set

M1 is the first release; M2 and M3 follow in order. Each row names the cox
piece it stands on; "new" means a gap closed in DT§4.7.

### 3.1 M1 — a complete, native daily driver

| Feature | Behaviour | Stands on |
| --- | --- | --- |
| Projects and sessions sidebar | Projects (git roots) with their sessions, newest first; "Needs you" and "Running" sections on top | `list_sessions`, `latest_session_for_cwd`, `project_slugs` |
| Several live sessions at once | Each session runs independently; switching never stops one | one `Session` per handle in `cox-app` |
| Worktree per session (optional) | New session: "in place" or "new worktree"; the toolbar shows the branch | `Worktrees`, `enter_worktree` |
| Streaming transcript | Blocks for user, assistant (markdown), thinking, tool calls, approvals, questions, notices, compaction, subagents | Event fold in `cox-app` |
| Composer | Multi-line; `@file` mentions with fuzzy match; `/commands` with completion; `!` shell mode; paste/drag images and files; queue while busy | `commands::parse`, `nucleo` matcher, `UserShell`, attachments (new) |
| Approvals | Inline card + pinned copy above the composer; allow once, allow for session (shows the grant), deny with reason, edit then run | `ApprovalRequired`, `Decision` |
| Questions (`ask_user`) | Card with the options as buttons and a free-text field | `QuestionAsked` (new) |
| Permission mode and model | Toolbar controls; the change is echoed as typed state, not a notice | `SetPermissionMode`, `SwitchModel`, `SetEffort`, `StateChanged` (new) |
| Review | Changed files for the session, unified or side-by-side diff, revert a file to any checkpoint, comment on a line to send it to the agent | `ToolResult.diff`, `Checkpoint`, `Rewind` |
| Rewind timeline | Gutter marks per turn; "restore code", "restore conversation", or both; edit-and-resend a past prompt | `Rewind`, `Redo` |
| Inspector | Tabs: Changes, Plan (live todo), Context & Cost, Tasks (subagents/background), Info | `ToolResult.structured` (new), `Usage`, `TaskCreated/Completed` |
| Search | ⌘K palette over actions, sessions, files; full-text over every past session | `rollout_search` (FTS) |
| Settings | Generated from `docs/config.jsonschema`; every field shows where its value comes from; API keys go to the Keychain | `cox-config` `source_of`, `set` |
| MCP servers | Status per server, OAuth login in the browser | `cox_mcp::auth::login` with a GUI `Prompt` |
| Notifications | Turn done (with cost), approval needed (Allow/Deny buttons), budget warning; Dock badge = items waiting for you | `TurnDone`, `ApprovalRequired`, `Level::Budget` |
| Resume and fork | Open any past session, fork at a turn, hand off with an objective | `History::from_rollout`, `fork`, `handoff` |
| Onboarding and doctor | Pick a folder; checks provider keys, git, sandbox, shell env; imports Claude settings | `cox doctor` checks, claude layer |

### 3.2 M2 — the rest of the terminal, and verification

Integrated terminal pane (SwiftTerm, R9.3.15) in the session's cwd; browser
preview pane (SwiftUI `WebView`, R9.3.11) the agent can screenshot and read;
pop a session out into its own window and native window tabs; menu-bar extra
listing running sessions and waiting approvals; Spotlight indexing of session
titles and App Intents ("Ask cox in <project>") for Shortcuts; per-hunk revert.

### 3.3 M3 — beyond a single agent

ACP host: Claude Code, Codex, Gemini CLI and Cursor (P35) sessions in the same
sidebar and transcript (reusing `crates/cox-acp` and T35.3's client adapter);
best-of-n (one prompt, several models, each in a worktree, compared diffs);
plugin panels drawn from the `Widget` tree (R9.4.12); remote sessions over SSH
through a `cox app-server` that speaks the same patch protocol (DT§4.4).

## 4. Architecture

### 4.1 Layers

```
┌──────────────────────── Cox.app (Swift) ─────────────────────────┐
│ CoxUI        SwiftUI views, design tokens. Knows only CoxModel.   │
│ CoxPlatform  Notifications, Keychain bridge, Sparkle, OAuth,      │
│              NSWorkspace, SwiftTerm, WebView. No business rules.  │
│ CoxModel     @Observable stores; apply patches; send intents.     │
│              Depends on the CoreClient protocol, not on FFI.      │
│ CoxCore      UniFFI-generated Swift + CoreClient implementation.  │
└───────────────▲──────────────────────────────┬────────────────────┘
      patches   │ async pull                    │ intents (sync, non-blocking)
┌───────────────┴──────────────────────────────▼────────────────────┐
│ cox-ffi      UniFFI exports, one tokio runtime, foreign traits.   │  staticlib
├────────────────────────────────────────────────────────────────────┤
│ cox-app      Workspace, SessionController, Timeline fold, Patch   │
│              coalescer, Inbox, Status, Commands, Completion.      │
├────────────────────────────────────────────────────────────────────┤
│ cox-session  Builds a Session from config: provider, tools, MCP,  │
│              skills, hooks, plugins, checkpointer, worktrees.     │
├────────────────────────────────────────────────────────────────────┤
│ cox-core · cox-protocol · cox-store · cox-config · cox-render ·   │
│ cox-tools · cox-mcp · cox-ext · cox-plugin · … (unchanged roles)  │
└────────────────────────────────────────────────────────────────────┘
```

The TUI, `run -p` and ACP move onto `cox-session` too (one builder, R9.4.8),
and the TUI may later move its fold onto `cox-app`; the desktop does not wait
for that.

### 4.2 New crates and their dependency rules

| Crate | Owns | May depend on | Must not |
| --- | --- | --- | --- |
| `cox-session` | `open(SessionSpec) -> Result<Opened, SessionError>`: config → provider, tools, MCP, skills, agents, hooks, plugins, checkpointer, worktrees; `fork`, `handoff`, `resume`; login-shell environment resolution (DT§4.8). Warnings return as data, never printed | core, protocol, config, provider, tools, mcp, ext, store, plugin, sandbox | clap, anyhow, any `print`; cox-tui |
| `cox-app` | The UI-agnostic application core (DT§4.3). Pure logic over events; the only async parts are the per-session drain task and the controller | session, core, protocol, store, config, render (neutral part), search | ratatui, crossterm, uniffi |
| `cox-ffi` | UniFFI records/enums mirroring `cox-app` types, the exported objects, the runtime, foreign traits (DT§4.4). `crate-type = ["staticlib", "lib"]` | app, protocol | anything else directly |

`crates/cox/tests/deps.rs` gets one rule per crate in the same change
(crates.md step 3), plus: `uniffi` only in `cox-ffi`; `cox-app` does not pull
ratatui. The slim build of the `cox` binary does not link `cox-ffi`.

`cox-render` today emits ratatui `Line`s. Its markdown and syntect
highlighting gain a neutral output — `StyledDoc` (blocks of `StyledSpan{text,
token: StyleToken, bold, italic, link}`) — with the ratatui conversion behind
a `ratatui` feature the TUI enables. One highlighter and one theme for both
surfaces; the desktop needs no Swift markdown or tree-sitter library.

### 4.3 `cox-app`: the application core

```
Workspace                       one per process
 ├─ projects(), sessions(project, limit), search(q)         → rows
 ├─ open(OpenRequest{cwd, resume, worktree, model}) → SessionController
 ├─ inbox: Inbox                approvals + questions across all sessions
 └─ worktrees(project) → [WorktreeRow{path, branch, bytes, merged, stale}]

SessionController               one per open session
 ├─ timeline: Timeline          Event → Block fold (+ replay from rollout)
 ├─ status: Status              model, tier, effort, mode, context %, cost, cache %
 ├─ drain task                  always reads Session::events(); never blocks the core
 ├─ coalescer                   batches patches per frame (16 ms or 64 patches)
 └─ send(Intent)                maps to Submission; spawns UserTurn
```

**Block model.** A block has a stable `BlockId`, a `turn`, and a kind:

| Kind | Built from | Carries |
| --- | --- | --- |
| `User` | `ItemStarted{UserMessage}` | text, attachments |
| `Assistant` | `ItemStarted{AssistantMessage}` + `TextDelta`s | `StyledDoc` (parsed in Rust) |
| `Thinking` | `ThinkingDelta`s | text, seconds, collapsed |
| `Tool` | `ToolCallRequested` → `ToolCallOutput` → `ToolCallDone` | tool, one-line summary, icon key, risk, state, output tail (last 5 lines), full-output `ArchiveRef`, `DiffModel`, duration |
| `ToolGroup` | consecutive read/grep/glob/outline calls | "Explored 7 files", children |
| `Approval` | `ApprovalRequired` / `ApprovalDecided` | call, why text, grant preview, source (subagent), state |
| `Question` | `QuestionAsked` (new) | question, options, state |
| `Task` | `TaskCreated` / `TaskCompleted` / `TaskMessage` | label, tier, cost, status, child session id |
| `Compaction` | `Compacted` | before → after tokens, reason, summary |
| `Checkpoint` | `Checkpoint` | turn, files |
| `Notice`, `Error` | `Notice`, `Error` | level, text, retryable |
| `TurnMeta` | `TurnStarted` + `Usage` + `TurnDone` | model, tokens in/out, cache read/write, cost, duration, stop reason |
| `Plugin` | `Advised`, plugin renderers | `Widget` tree (M3) |

Summaries ("Ran `cargo test` — exit 0 · 4.2 s", "Edited `crates/x.rs` +12 −3")
are produced in Rust so the TUI, ACP titles and the app say the same thing.

**Patches.** Swift never sees `Event`. It sees:

```
enum TimelinePatch {
  Reset { blocks: Vec<Block> }                 // open, resume, rewind
  Upsert { block: Block, after: Option<BlockId> }
  AppendText { id: BlockId, text: String }      // thinking, tool output tail
  DocTail { id: BlockId, from: u32, blocks: Vec<DocBlock> }  // markdown: closed blocks are frozen, only the tail is re-sent
  Remove { id: BlockId }
  Status { status: Status }
}
```

Keyed by id, not index, so SwiftUI identity is stable and a dropped patch can
be healed by `Reset`. Streaming markdown re-parses only the open tail block.

**Inbox.** Every pending approval and question from every session, oldest
first, with `session`, `source` and an expiry flag. Drives the "Needs you"
section, the Dock badge and notifications.

**Intents.** `Send{text, attachments}`, `Approve{call, decision}`,
`Answer{question, text}`, `Interrupt`, `Queue{text}`, `Compact`, `SetMode`,
`SwitchModel`, `SetEffort`, `Rewind`, `Redo`, `Fork{turn}`, `Handoff`,
`Background{call}`, `Shell{command, share}`, `Command{line}` (parsed by the
shared command table). `send` never awaits a turn: `UserTurn` is spawned, as
`run.rs` already does (R9.4.3).

### 4.4 The FFI surface

A sketch; names are indicative, the shape is the decision.

```rust
#[derive(uniffi::Object)]
pub struct App { /* Workspace, runtime handle */ }

#[uniffi::export(async_runtime = "tokio")]
impl App {
    #[uniffi::constructor]
    pub fn new(home: String, host: Arc<dyn Host>) -> Result<Arc<Self>, AppError>;
    pub fn projects(&self) -> Vec<ProjectRow>;
    pub fn sessions(&self, project: Option<String>, limit: u32) -> Vec<SessionRow>;
    pub async fn search(&self, query: String, limit: u32) -> Vec<SearchHit>;
    pub async fn open(&self, req: OpenRequest) -> Result<Arc<SessionHandle>, AppError>;
    pub async fn next_app_patches(&self) -> Vec<AppPatch>;   // sidebar, inbox, badge
    pub fn settings(&self) -> SettingsView;                   // values + provenance
    pub fn set_setting(&self, key: String, json: String) -> Result<(), AppError>;
    pub fn store_key(&self, provider: String, secret: String) -> Result<(), AppError>;
}

#[uniffi::export(async_runtime = "tokio")]
impl SessionHandle {
    pub fn snapshot(&self) -> Vec<Block>;
    pub async fn next_patches(&self) -> Option<Vec<TimelinePatch>>; // None: closed
    pub fn send(&self, intent: Intent) -> Result<(), AppError>;      // never blocks
    pub async fn expand(&self, archive: String) -> Result<String, AppError>;
    pub fn complete(&self, prefix: String, kind: CompletionKind) -> Vec<Completion>;
    pub fn close(&self);
}

#[uniffi::export(with_foreign)]
pub trait Host: Send + Sync {          // implemented in Swift (CoxPlatform)
    fn open_url(&self, url: String);    // MCP OAuth, links
    fn notify(&self, note: Note);       // optional; the app also reads the inbox
}
```

Rules:

- **Pull, not push.** `next_patches` is an async pull (R9.3.2): backpressure
  and cancellation come free with Swift `Task` cancellation. The only foreign
  trait is `Host`, for things Rust must ask the OS to do.
- **Records are plain data**, generated as Swift structs and enums; errors are
  one `AppError` enum mapped from the crates' `thiserror` enums.
- **The patch types derive `Serialize` and `JsonSchema` too.** The same stream
  can later go over a socket (`cox app-server`, M3 remote sessions) or be
  recorded as a fixture for Swift tests (DT§8) without a second protocol.
- Secrets: the Rust side keeps resolving keys with `resolve_key` (keyring).
  Tests inject the lookup, never the real Keychain (A49); `store_key` is the
  only write path.

### 4.5 Threads, runtime, backpressure, cancellation

- **One tokio runtime per process**, created by `cox-ffi` on first use
  (UniFFI's tokio feature, R9.3.2). Nothing calls `block_on` inside it.
- **The core is never blocked by the UI.** `Session::events()` is a bounded
  channel of 256 and `emit` awaits (R9.4.2). The drain task in `cox-app`
  reads it continuously, folds into the timeline and pushes patches into an
  unbounded but *coalescing* buffer: consecutive `AppendText`/`DocTail` for
  the same block merge, so a stalled UI costs memory proportional to the
  number of changed blocks, not to the number of tokens. Budget test: DT§1.
- **Swift side.** One `Task` per open session awaits `next_patches()` and
  applies the batch on the `MainActor` in one transaction. Batching at 16 ms
  keeps main-actor hops to at most one per frame.
- **Cancellation.** Closing a session cancels its Swift task and calls
  `close()`; the agent turn keeps running unless the user interrupts
  (`Intent::Interrupt` → `Session::interrupt`). Quitting the app with running
  turns asks first.
- **Sessions open elsewhere.** The TUI and the app share `cox.db`; `Presence`
  (already in the protocol) marks a session open in another surface, and the
  app shows it read-only with a "take over" action. SQLite concurrency across
  two processes is an open question (DT§11 Q6).

### 4.6 Swift side

| Package | Contents | Depends on |
| --- | --- | --- |
| `CoxCore` | `binaryTarget` `CoxFFI.xcframework`; generated `cox_ffi.swift`; `LiveCoreClient` adapting it to the `CoreClient` protocol | — |
| `CoxModel` | `@Observable @MainActor` stores: `AppStore` (projects, sessions, inbox, badge), `SessionStore` (ordered blocks by id, status, composer draft), `SettingsStore`. `apply(_ patches:)` and `send(_ intent:)` only | `CoreClient` protocol |
| `CoxUI` | Views and the design system (DT§5.9) | `CoxModel` |
| `CoxPlatform` | `Host` implementation, notifications with actions, Sparkle, OAuth handoff, `NSWorkspace` "open in editor", SwiftTerm and `WebView` panes (M2) | `CoxModel` |
| App target | `@main`, scenes, menus, entitlements, Info.plist, assets | all |

**What Swift may do:** lay out, animate, localize dates and numbers, map a
`StyleToken` to a color, keep UI-only state (scroll position, which blocks
are expanded, window frames), talk to the OS.
**What Swift may not do:** decide a permission, compute a cost, parse a
command or markdown, compute a diff, choose a model, touch `~/.cox` or git.
A review rule: any `if` in Swift that inspects a tool name or an event kind
to decide behaviour is a bug — the kind comes from Rust already decided.

Strict concurrency is on (Swift 6 language mode). Generated UniFFI code is
wrapped so its partial `Sendable` coverage (R9.3.4) stays inside `CoxCore`.

### 4.7 Core changes the GUI needs (fixed for every surface)

| # | Gap (R9.4) | Change | Who else benefits |
| --- | --- | --- | --- |
| G1 | Assembly in the binary; ACP bypasses it (9.4.8) | `cox-session` crate; TUI, `run`, ACP call it | ACP gets MCP, skills, hooks, checkpoints |
| G2 | `UserTurn.attachments` ignored (9.4.4) | Images and file attachments reach the request (per provider capability) | TUI paste, ACP |
| G3 | Todo only as text (9.4.5) | `ToolResult.structured: Option<Value>`; TUI and ACP drop their re-parsers | TUI, ACP |
| G4 | `ask_user` side channel (9.4.6) | `Event::QuestionAsked{id, call, question, options, source}` + `Submission::Answer{id, text}` | rollout replay, ACP, `run -p` |
| G5 | Mode/effort echoed as a string (9.4.7) | `Event::StateChanged{mode, effort}` | TUI status line |
| G6 | No session title | `Event::TitleSet{title}` from a low-cost `Job::Title` call after the first turn | TUI, sessions list |
| G7 | Stale counts in `protocol.md` (9.4.1) | Fix the doc | — |
| G8 | Instruction files never reach the prompt (9.4.9) | Wire the `AGENTS.md` chain (spun off as its own task) | every surface |
| G9 | Plugin UI channel uses TUI types | Neutral `PluginRequest` in `cox-app` (M3) | — |

G2–G6 change `cox-protocol`, so the schema test regenerates
`docs/protocol.jsonschema` and each lands as its own ≤ 200-LOC card.

### 4.8 The login-shell environment

An app launched from Finder or the Dock does not get the user's shell `PATH`,
so `bash` would not find `cargo`, `mise` or `node`, and env-var API keys are
invisible. `cox-session` resolves the environment once at startup: run the
user's login shell (`$SHELL -l -i -c` printing `env -0`) with a 3 s timeout,
parse, and use it as the base environment for tools; on timeout fall back to
`launchd`'s environment and show a notice. The CLI keeps its inherited
environment. Keys the app itself stores live in the Keychain.

## 5. Interface design

### 5.1 Window anatomy

Default window 1 440 × 900, minimum 900 × 600. A three-column
`NavigationSplitView`:

```
┌ toolbar ─────────────────────────────────────────────────────────────────────┐
│ ◧  cox › main ⎇ wt/fix-login     [Sonnet 5 · high ▾] [Ask|Plan|Auto]  $0.42 · ctx 38% ■ ◨ │
├──────────────┬──────────────────────────────────────────────┬─────────────────┤
│ SIDEBAR 250  │ TRANSCRIPT  (reading column ≤ 760 pt)        │ INSPECTOR 320   │
│ ⌕ Filter     │  12 │ You: fix the login redirect …          │ Changes · Plan  │
│ NEEDS YOU 2  │     │ Assistant text …                       │ Context · Tasks │
│ RUNNING 1    │     │ ▸ Explored 6 files                     │ · Info          │
│ ▾ cox        │     │ ✎ Edited src/auth.rs  +12 −3           │                 │
│   ● Fix log… │     │ ⚠ Run `git push`?  [Allow] [Session] … │                 │
│   ○ Bench …  │                                              │                 │
│ ▸ other-proj │ ┌ composer ────────────────────────────────┐ │                 │
│ ＋ New ⌘N    │ │ Ask cox…  @file  /cmd  !shell     ⏎      │ │                 │
│              │ └ 📎  Plan ⇧⇥   Sonnet 5 · high   think ─────┘ │                 │
└──────────────┴──────────────────────────────────────────────┴─────────────────┘
```

- **Toolbar** (Liquid Glass): sidebar toggle; breadcrumb *project › branch ›
  worktree* (click: switch branch/worktree); model chip (tier · model · effort,
  menu); permission-mode segmented control; cost pill (`$` for the session and
  context-window fill; click opens Context & Cost); Stop button while a turn
  runs (⌘.); inspector toggle. **Bypass mode** paints a thin red strip under
  the whole toolbar for as long as it is on.
- **Sidebar**: filter field; "Needs you" (sessions with a pending approval or
  question, orange count); "Running"; then projects as disclosure groups.
  A row: status glyph (● running, ◐ waiting for you, ○ idle, ✕ error),
  title, one line of last activity, cost at the right on hover. Context menu:
  rename, fork, open worktree in Finder/editor, archive, delete (with
  confirm). Footer: New session (⌘N), provider-health dot.
- **Transcript**: one centered reading column, max 760 pt, with a 36 pt left
  gutter for turn numbers and checkpoint marks. The composer is docked at
  the bottom of the column and grows up to 40 % of the height.
- **Inspector** (⌥⌘I), tabs:
  - *Changes* — files touched this session with +/− and the tool call that
    touched them; click opens Review.
  - *Plan* — the live todo list with statuses.
  - *Context & Cost* — a stacked bar of the window (system, tools,
    instructions, history, free), cache-hit %, "Compact now"; a per-turn cost
    table (input, output, cache read, cache write, `$`), session and project
    totals, the budget cap and how close it is.
  - *Tasks* — subagents and background calls: label, tier, state, cost; click
    opens the child transcript in the inspector.
  - *Info* — session id, cwd, worktree, config provenance, rollout path.

### 5.2 Transcript blocks

- **User message** — full-width block with a subtle filled background
  (quaternary fill, 10 pt radius), attachments as thumbnails under the text.
  Hover: "Edit and resend" (rewinds the conversation to before this turn and
  prefills the composer), "Copy".
- **Assistant message** — plain text on the window background, no bubble,
  markdown drawn from the Rust `StyledDoc`. Code blocks: header with language,
  Copy and "Open in editor" (when the block names a path); monospaced body;
  long blocks (> 40 lines) fold with "Show 120 more lines".
- **Thinking** — one secondary-color row "Thought for 12 s ▸"; expands to
  the reasoning in secondary text. Streaming: the row shows a live timer.
- **Tool call** — one row: SF Symbol per tool (`doc.text` read, `pencil`
  edit/write, `terminal` bash, `magnifyingglass` grep/glob, `globe` web,
  `checklist` todo, `person.2` agent), the Rust summary, and a trailing state
  (spinner, ✓, ✕ with exit code, duration). While running, a monospaced
  five-line live tail of the output sits under the row. Expanded: the input
  (command or pretty JSON), the output (truncated outputs end with "Show full
  output · 84 KB", which calls `expand` and never refetches from the model),
  and the diff for edits. Consecutive read/grep/glob/outline calls fold into
  one "Explored 6 files" row. High-risk calls carry a risk badge.
- **Approval** — a card with an orange edge, in the transcript where it
  happened and pinned above the composer while pending. Content: *what* (the
  command, highlighted; for an edit, the diff), *why* (from `why_text`:
  "matches ask rule `Bash(git push:*)`", "risk high: writes outside the
  workspace", "sandbox denied network"), *who* (a subagent label when
  `source` is set). Buttons: **Allow** (⏎), **Allow for session** (⌘⏎, with
  the exact grant it adds, e.g. "`git push *` until this session ends"),
  **Edit…** (opens the command in an editable field; runs as
  `Decision::Edit`), **Deny** (⎋, optional reason field). After a decision
  the card shrinks to one line: "Allowed by you · for session".
- **Question** — a card with the question, one button per option and a text
  field; answered cards collapse to "You answered: …".
- **Subagent task** — a nested card with label, tier, live status and cost;
  "Open" shows its transcript in the inspector.
- **Compaction** — a centered divider: "Context compacted · 142k → 31k
  tokens ▸ summary".
- **Checkpoint** — a dot in the gutter; hover: "Restore code to here".
- **Notice / budget / security** — slim single lines, color by level; a
  budget stop adds "Raise cap and continue".
- **Error** — a red-edged block with the message and "Retry".
- **Turn meta** — on hover over a turn, a secondary line: model, tokens in /
  out, cache %, `$`, duration, stop reason.

Selection: text selection is enabled per block; "Copy as Markdown" on every
block and on a multi-block selection made with ⇧-click in the gutter.
Cross-block drag selection depends on the DT§9 benchmark outcome.

### 5.3 Composer

- `TextEditor` over `AttributedString` (R9.3.11). ⏎ sends, ⇧⏎ new line.
- `@` opens a file picker ranked by the Rust fuzzy matcher; the chosen file
  becomes a pill. `/` lists commands from the shared command table with
  their help text. A leading `!` switches to shell mode: monospaced font,
  terminal icon, "share output with the agent" toggle (`UserShell{share}`).
- Paste or drop images and files; they show as removable chips.
- While a turn runs, ⏎ queues the message (the send button shows "Queued ·
  1"); ⌘⏎ interrupts and sends now; ⌘. interrupts.
- ↑ in an empty composer walks the prompt history (`user_prompts`).
- The chips row below: attachment button, permission mode (⇧⇥ cycles), model
  and effort, "think" toggle.

### 5.4 Review

⌘⇧R, or "Review" in the Changes tab, replaces the transcript column with a
split: file list (grouped by turn, +/− counts) and the diff (unified or side
by side, toggle ⌘⌥D), syntax colored by the same Rust highlighter. Per file:
"Revert to before turn N" (checkpoint restore), "Open in editor". Click a
line number to add a comment; comments collect into a draft and "Send to
agent" posts one message with file:line anchors. "Rewind to here" on any
turn in the list. Per-hunk revert is M2.

### 5.5 Command palette and keyboard

⌘K opens one palette over actions, sessions (fuzzy on title), files in the
current project and slash commands. The whole app is keyboard-drivable:

| Key | Action |
| --- | --- |
| ⌘N / ⌘⇧N | New session / new session in a worktree |
| ⌘1…⌘9 | Jump to the n-th session in the sidebar |
| ⌘K | Palette |
| ⌘L | Focus composer |
| ⌘. | Interrupt |
| ⏎ / ⌘⏎ / ⎋ | On a pending approval: allow / allow for session / deny |
| ⌘⇧R | Review |
| ⌥⌘I / ⌘⌃S | Inspector / sidebar |
| ⌘⇧F | Search all sessions |
| ⌘[ / ⌘] | Previous / next turn in the transcript |
| ⌘+ / ⌘− | Text size |

### 5.6 Notifications, Dock, menu bar

Notifications only when the app is not frontmost or the session is not
visible: "Turn finished · $0.18", "Approval needed: `git push`" with
**Allow** and **Deny** actions (allow-for-session and edit need the app, by
design), "Budget reached". Dock badge = pending approvals + questions across
all sessions. The M2 menu-bar extra lists running sessions and the inbox.

### 5.7 Settings

A separate Settings window (⌘,) generated from `docs/config.jsonschema` with
hand-tuned grouping: General, Models & Providers, Permissions, Sandbox,
Budget, MCP Servers, Plugins, Appearance, Advanced. Each field shows a
provenance badge — *default*, *user*, *project*, *env*, *flag* — from
`source_of`; a field overridden by the project config shows the project file
and is read-only here (project guard keys stay guarded). Edits go through
`cox-config`'s comment-preserving `set` (user file only). Provider keys are
entered in a secure field and go to the Keychain through `store_key`. MCP:
status per server, Log in / Log out; login opens the browser via `Host`.

### 5.8 Onboarding and empty states

First launch: "Open a project" (folder picker or drop a folder), then a
checklist from the doctor checks — provider key found, git available,
sandbox works, shell environment resolved — each with a fix button. Offer to
import Claude Code settings. Empty transcript: three example prompts built
from the project (e.g. "Explain the architecture of <name>"). An error state
always names the cause and one action.

### 5.9 Typography, color, motion, accessibility

- **Type.** Body SF Pro Text 13 pt, line height 1.45; meta 11 pt secondary;
  markdown headings 17 / 15 / 13 pt semibold; code SF Mono 12 pt (any
  installed monospaced font can be chosen). Text size scales 85–150 % (⌘+/⌘−).
- **Color.** Semantic tokens only, one-to-one with `cox-render`'s
  `StyleToken` (Text, Dim, Accent, Success, Warning, Error, DiffAdd, DiffDel,
  Border, Selection, syntax roles), each an asset color with light, dark and
  increased-contrast variants. The accent follows the system accent. Syntax
  colors come from the same theme as the TUI, so a screenshot of either looks
  like the same product. Mode colors: Plan blue, Auto accent, Bypass red.
- **Motion.** No per-token animation; new blocks fade in over 120 ms; state
  changes (spinner → ✓) cross-fade; everything respects Reduce Motion.
- **Accessibility.** Each block is one accessibility element with a Rust
  summary as its label; the finished assistant message is announced once, not
  per token; VoiceOver rotors for Approvals, Tool calls and Errors; every
  control reachable by keyboard; contrast checked in both themes.
- **Language.** English first, String Catalogs from day one; Russian next.

## 6. Where things live

```
apps/cox/
├─ crates/
│  ├─ cox-session/            NEW  session assembly (from crates/cox/src/session.rs)
│  ├─ cox-app/                NEW  workspace, timeline fold, patches, inbox, status
│  │  └─ src/{lib,workspace,controller,timeline,patch,inbox,status,intent,complete}.rs
│  ├─ cox-ffi/                NEW  UniFFI surface, runtime, foreign traits
│  │  └─ src/{lib,types,app,session,host,error}.rs
│  ├─ cox-render/                  + neutral StyledDoc; ratatui behind a feature
│  └─ cox/                         TUI, run, ACP call cox-session; deps.rs rules
├─ desktop/macos/             NEW
│  ├─ Cox.xcodeproj                thin: app target, entitlements, Info.plist, assets
│  ├─ App/                         CoxApp.swift (@main, scenes, commands)
│  ├─ Packages/
│  │  ├─ CoxCore/                  Package.swift (binaryTarget ../../build/CoxFFI.xcframework)
│  │  ├─ CoxModel/                 stores + Tests/
│  │  ├─ CoxUI/                    views, DesignSystem/ + Tests/ (snapshots)
│  │  └─ CoxPlatform/              Host, notifications, Sparkle, terminal, web
│  ├─ Fixtures/                    patch streams recorded from scripted scenarios
│  └─ UITests/
├─ scripts/desktop/xcframework.sh  NEW  cargo (aarch64) → xcodebuild -create-xcframework
└─ justfile                        + desktop-xcframework, desktop-test, desktop-bench
```

The app keeps all Swift code in local Swift packages so the `.xcodeproj`
stays small and merge-friendly; only the app target lives in it.

## 7. Build, packaging, distribution

- **Rust → XCFramework.** `scripts/desktop/xcframework.sh` builds
  `cox-ffi` for `aarch64-apple-darwin` only (no Intel, DT§11 Q2) in release with split debug info, sets
  `MACOSX_DEPLOYMENT_TARGET=26.0` explicitly (R9.3.7), runs
  `uniffi-bindgen-swift`, and packs `build/CoxFFI.xcframework` + the
  generated Swift into `Packages/CoxCore`. Pattern from Firefox's script
  (R9.3.6). Run through `mise exec --` like every cargo command.
- **Dev loop.** An Xcode "Run Script" phase calls the script only when a Rust
  input changed; CI builds from scratch. Rust never compiles inside every
  Swift build.
- **Bundled CLI.** The same build puts the `cox` binary in
  `Cox.app/Contents/Helpers/cox`; the app offers "Install command-line tool"
  (a symlink in `/usr/local/bin` or `~/.local/bin`), so app and CLI never
  drift in version or database schema.
- **Signing.** Developer ID Application, Hardened Runtime, notarized with
  `notarytool`, stapled. No App Sandbox (DT-6). Entitlements: none beyond the
  defaults unless a plugin runtime needs JIT (wasmtime: `allow-jit` — to be
  confirmed by the first signed build, DT§11 Q5).
- **Updates.** Sparkle 2 (R9.3.17) with an EdDSA-signed appcast on GitHub
  Releases; updates never kill running turns: "Install on quit", or after
  asking when sessions are running (the Codex SIGKILL-on-update complaint,
  R9.2).
- **Other channels.** A Homebrew cask pointing at the same notarized DMG.
- **CI.** A macOS job: build the XCFramework, `swift test` for the packages,
  UI tests on the scripted scenarios, a notarization dry run on tags.

## 8. Testing

- **Rust first.** `cox-app` is where the logic is, so it carries the tests:
  `insta` snapshots of the block list for every scripted scenario in
  `tests/` (the same scenarios the TUI and e2e runs use); a test that a
  consumer which stops reading for 10 s does not delay a turn; patch
  coalescing; replay-from-rollout equals live fold; inbox across two
  sessions. No network, no API key, no real Keychain.
- **Fixtures across the boundary.** A `cox-ffi` test binary records the
  patch stream of each scenario to `desktop/macos/Fixtures/*.json`
  (the patch types are serde, DT§4.4). Swift tests and SwiftUI previews
  replay them through a `FixtureCoreClient`, so previews show real
  transcripts with no running core.
- **Swift.** Swift Testing for the stores (patch application, intents
  emitted); swift-snapshot-testing for views in light, dark and
  increased-contrast (R9.3.19); XCUITest for the smoke path: open a project,
  send a prompt, approve, review, rewind — against `cox-ffi`'s
  `Scripted` provider build flag.
- **Performance.** The DT§1 budgets as XCTest metrics, run by
  `just desktop-bench`, and recorded in `research.md` §4.x like the other
  measurements.
- **Real runs.** Manual checks use a scratch `COX_HOME`, like every other
  surface.

## 9. Transcript rendering: the one open technical bet

SwiftUI `LazyVStack` in a `ScrollView` is the simplest way to draw
variable-height rich blocks but is known to degrade on very long lists
(**unverified** community reports, R9.3); an `NSTableView` with
`NSHostingView` cells gives true reuse. The decision is a measured gate, not
a guess: card T37.23 builds the transcript on `LazyVStack` with stable ids
and a realized window (turns older than the last 30 collapse to one-line turn
summaries until scrolled to), runs the DT§1 scroll and streaming budgets on
the 10k-block fixture, and switches to the `NSTableView` host if it misses.
Cross-block text selection, if the creator wants it (DT§11 Q7), needs a
single TextKit 2 document (`NSTextView`) instead; that is a third option the
same benchmark can include.

## 10. Trust boundaries

Nothing new is trusted. Every model- and tool-originated string the app
shows went through `cox_sanitize::sanitize` in Rust before it became a block;
Swift renders text only (no HTML from the model, links shown with their
target and opened through `Host` after a confirmation for non-`https`
schemes). Tool calls are still decided by `cox_permission::Engine`; the app
only answers the question the engine asked. Paths still go through
`cox_sandbox::path::confine`; shell commands still run under
`cox_sandbox::sandbox::Policy` — the desktop adds no "run in the app's own
context" path. The M2 browser pane is a separate `WebPage` with no
JavaScript bridge to the app.

## 11. Open questions for the creator

1. ~~**Plan decisions.** Amend D1, D2, D11?~~ Resolved 2026-09-28: option A,
   in-process static library; `plan.md` §0 and A67 carry the new wording.
2. ~~**Hardware floor.**~~ Resolved 2026-09-28: macOS 26+ on Apple Silicon
   only, no Intel.
3. ~~**Process model.**~~ Resolved 2026-09-28 with Q1: in-process UniFFI. The
   patch types stay serde so a socket server can be added later for remote
   sessions without redesign.
4. ~~**License and repo.**~~ Resolved 2026-09-28: this repository, `desktop/macos/`, under the repository's licence.
5. ~~**Swift dependencies**~~ Resolved 2026-09-28: `research.md` §9.5 and A67;
   cross-block selection engine decided by spike T37.37.
6. ~~**Concurrent processes on `cox.db`.**~~ Resolved 2026-09-28: one shared
   database (WAL and busy timeout already on, `cox-store/src/lib.rs:190`);
   one process drives a session under an OS file lock, others follow or fork
   (T37.34); IMMEDIATE write transactions and a `data_version` change feed
   (T37.35); an older binary refuses a newer schema (T37.36).
7. ~~**Cross-block text selection**~~ Resolved 2026-09-28: cross-block selection, on by default, with a setting to turn it off.
8. ~~**External agents (ACP host)**~~ Resolved 2026-09-28: stays M3; moves into the plan when a planned card is blocked by it.

## 12. Plan amendment (applied as A67, phase P37)

Applied to `plan.md` on 2026-09-28: A67 changes §0 D1, D2 and D11 (option A,
in-process static library) and adds phase P37 with cards T37.0–T37.36. The
cards, their order and their checks live in `plan.md` only; M2 and M3 are in
`roadmap.md`. The view-layer guide and tokens are `desktop/design/DESIGN.md`.
