//! The wire contract: `Submission` in, `Event` out (plan.md §1.2), plus
//! every type reachable from them and from `Request`/`ToolSpec`. This file
//! has no logic — only shapes and their serde/schemars derives — because
//! `cox-protocol` is the one crate every other crate may depend on, and a
//! behaviourless contract is what keeps that dependency safe to add.
//!
//! Serde convention: struct-shaped enums use `#[serde(tag = "type",
//! rename_all = "snake_case")]` so a rollout line greps as
//! `"type":"tool_call_done"`; small field-less config enums use bare
//! `#[serde(rename_all = "snake_case")]`, serializing as a plain string.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::CoreError;
use crate::ids::{ArchiveId, CallId, ItemId, SessionId, TaskId, TurnId};

// ---------------------------------------------------------------------
// Field-less config/tag enums
// ---------------------------------------------------------------------

/// Who decided an approval (`ApprovalDecided::by`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DecidedBy {
    /// The user answered an `ApprovalRequired` prompt.
    User,
    /// An `allow`/`deny`/`ask` rule matched.
    Rule,
    /// An `AllowForSession` grant from earlier in the session matched.
    Session,
    /// The permission mode or approval policy decided without a rule.
    Policy,
    /// A `PreToolUse` hook decided (`Block`/`Modify`).
    Hook,
}

/// How risky a tool call is, independent of what it does (plan.md §1.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    /// Cannot change anything cox does not already show the model.
    ReadOnly,
    /// Writes inside the workspace.
    Write,
    /// Runs a process.
    Exec,
    /// Can destroy data or affect more than the immediate subject (`rm -rf`, `apply_patch` deleting > 5 files).
    Destructive,
}

/// Whether a tool may run alongside other calls in the same batch (plan.md §1.3 step 3.d.iv).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Concurrency {
    /// May run in parallel with other `Parallel` calls, up to `core.parallel_tools`.
    Parallel,
    /// Must run alone; other calls in the batch wait.
    Exclusive,
}

/// A routing tier (plan.md §1.4/D5): a job maps to a tier, a tier maps to a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Haiku-class or local; mechanical work, never chosen for the main coding turn.
    Cheap,
    /// Sonnet by default, Opus when picked; the main coding turn.
    Code,
    /// Fable 5.1 only, only via `/think`/`--deep`, always confirmed.
    Think,
}

/// What a request is *for* (plan.md §1.4); every job is pinned to one tier in config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Job {
    /// The main coding turn.
    Main,
    /// A `/think`/`--deep` plan.
    Plan,
    /// Compaction summary.
    Compact,
    /// Session title generation.
    Title,
    /// Tool-result or transcript summarisation.
    Summarize,
    /// Commit message generation.
    Commit,
    /// Memory extraction.
    Memory,
    /// An `explore` subagent.
    Explore,
    /// A background shell/HTTP subagent.
    Shell,
    /// A hook-driven LLM call.
    Hook,
}

/// Reasoning effort passed to the provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    /// Cheapest, fastest; used for `cheap`-tier jobs.
    Low,
    /// Default for `code`/`think` tiers.
    High,
    /// User-selected for a flagged large refactor.
    Xhigh,
}

/// Extended/adaptive thinking mode for a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Thinking {
    /// No thinking block requested.
    Off,
    /// Provider decides whether and how much to think.
    Adaptive,
}

/// Why a provider call or a turn stopped.
///
/// Reused for both `ProviderEvent::Stop` (one provider call) and
/// `Event::TurnDone` (the whole turn); a provider only ever emits
/// `EndTurn`/`Refusal`/`Error`, the others are added by `cox-core` once it
/// has aggregated multiple calls in a turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StopReason {
    /// The model finished normally.
    EndTurn,
    /// `core.max_turns` provider calls were used up without finishing.
    MaxTurns,
    /// `Submission::Interrupt` cancelled the turn.
    Interrupted,
    /// A budget cap stopped the turn before another call was made.
    Budget,
    /// The model refused to continue.
    Refusal {
        /// The provider's refusal text, if any.
        detail: String,
    },
    /// The turn ended in an unrecoverable error.
    Error,
}

/// Severity of a `Notice` event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Informational; no action needed.
    Info,
    /// Something degraded but the turn continued (e.g. a skipped hook).
    Warn,
    /// A budget threshold was crossed.
    Budget,
    /// A trust-boundary guard fired (sanitized output, sandbox denial).
    Security,
}

/// `permissions.mode` (plan.md §1.6/§1.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    /// Rules and risk decide as usual.
    Default,
    /// Only `Risk::ReadOnly` is allowed; everything else denies without asking.
    Plan,
    /// `Write` is allowed automatically; `Exec`/`Destructive` still ask unless a safe-command match.
    Auto,
    /// Everything is allowed; flag-only, banner shown.
    Bypass,
}

/// `permissions.approval` (plan.md §1.6/§1.8 step 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalPolicy {
    /// Anything not covered by an `allow` rule asks.
    Untrusted,
    /// Default: risk-based asking (step 7).
    OnRequest,
    /// `Exec` runs sandboxed without asking; asks only if the sandbox denies.
    OnFailure,
    /// Any `Ask` becomes `Deny` (headless default).
    Never,
}

/// `sandbox.mode` (plan.md §1.6/D7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxMode {
    /// No writes anywhere.
    ReadOnly,
    /// Writes confined to the workspace roots.
    WorkspaceWrite,
    /// No sandbox at all; requires explicit opt-in.
    DangerFullAccess,
}

/// Which hook trigger point fired (Claude Code hook protocol, D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    /// Before the user's text is pushed onto history; may block or rewrite it.
    UserPromptSubmit,
    /// Before a tool call runs; may `Block` or `Modify` its input.
    PreToolUse,
    /// After a tool call succeeds.
    PostToolUse,
    /// After a tool call fails.
    PostToolUseFailure,
    /// After a turn finishes normally.
    Stop,
    /// Before compaction runs; may `Block` it.
    PreCompact,
    /// After compaction runs.
    PostCompact,
}

/// `sandbox.linux_backend` (plan.md T4.2): which Linux confinement to use.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum LinuxBackend {
    /// `bwrap` when it can create namespaces here, else Landlock, else none.
    #[default]
    Auto,
    /// bubblewrap only.
    Bwrap,
    /// Landlock + seccomp only.
    Landlock,
    /// No confinement on Linux; the surface warns and forces `on-request`.
    None,
}

/// `[sandbox]` config, resolved for one call (plan.md §1.6/D7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SandboxPolicy {
    /// The sandbox mode in effect.
    pub mode: SandboxMode,
    /// Whether network access is allowed.
    pub network: bool,
    /// Extra writable roots beyond the workspace.
    pub writable: Vec<PathBuf>,
    /// Paths inside the workspace that stay read-only even in `workspace-write` (`.git`, `.cox`).
    pub readonly_in_workspace: Vec<PathBuf>,
    /// Which Linux backend confines the command; ignored elsewhere.
    pub linux_backend: LinuxBackend,
}

// ---------------------------------------------------------------------
// Structs and tagged enums reachable from `Event`/`Submission`/`Request`
// ---------------------------------------------------------------------

/// A file, image or other blob attached to a `UserTurn`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Attachment {
    /// Display name (usually the original filename).
    pub name: String,
    /// MIME type, e.g. `"image/png"`.
    pub media_type: String,
    /// Base64-encoded bytes.
    pub data_b64: String,
}

/// A unified diff for one file, produced by `edit`/`apply_patch`/`write`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Diff {
    /// The file the diff applies to.
    pub path: PathBuf,
    /// Unified diff text (`---`/`+++`/`@@` form).
    pub unified: String,
}

/// A pointer to a full tool output stored in the archive (plan.md §1.7/D6a).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ArchiveRef {
    /// The archive row's id (`cox expand <id>`).
    pub id: ArchiveId,
    /// Size of the archived payload, in bytes.
    pub bytes: u64,
}

/// A request from the model to run a tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolCall {
    /// Correlates `ToolCallRequested` through `ToolCallDone`.
    pub id: CallId,
    /// The tool's registered name (or `mcp__<server>__<tool>`).
    pub name: String,
    /// The model-supplied arguments, validated against the tool's `input_schema`.
    pub input: Value,
    /// The call's risk classification, used by the permission engine.
    pub risk: Risk,
    /// What permission rules match on: the confined path, command line, URL, or MCP name.
    pub subject: String,
}

/// The outcome of a finished tool call, as it appears in history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolResult {
    /// Whether the call succeeded.
    pub ok: bool,
    /// What the model sees: possibly truncated, with a pointer trailer.
    pub visible: String,
    /// Where the untruncated output lives, if it was archived.
    pub archive: Option<ArchiveRef>,
    /// Size of the full (pre-truncation) output, in bytes.
    pub bytes: u64,
    /// Wall-clock time the call took.
    pub duration_ms: u64,
    /// A unified diff, for edit-shaped tools.
    pub diff: Option<Diff>,
}

/// An approval decision, for both `Submission::Approve` and `ApprovalDecided`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Decision {
    /// Run the call once.
    Allow,
    /// Run this call and grant future calls with the same tool + subject prefix, for this session.
    AllowForSession,
    /// Refuse the call.
    Deny {
        /// Shown to the model as the tool result.
        reason: String,
    },
    /// Run the call, but with edited input (e.g. a corrected path).
    Edit {
        /// The replacement input.
        input: Value,
    },
}

/// Why an `ApprovalRequired` was raised (plan.md §1.2/§1.8 step 9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Why {
    /// An `ask` rule matched.
    RuleAsk {
        /// The rule string that matched (e.g. `"Bash(git commit:*)"`).
        rule: String,
    },
    /// No rule matched; the call's risk classification requires asking.
    Risk {
        /// The call's risk.
        risk: Risk,
    },
    /// The sandbox denied the call and `approval == on-failure`.
    SandboxDenied {
        /// The sandbox backend's denial detail.
        detail: String,
    },
    /// The active approval policy forces asking regardless of risk.
    Policy {
        /// The policy in effect.
        policy: ApprovalPolicy,
    },
}

/// What kind of transcript item an `Item` is; each variant carries exactly
/// what that item needs to be replayed into history on resume (plan.md
/// §1.7: "resume rebuilds `history` from `ItemStarted`/`ItemDone` pairs").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ItemKind {
    /// The user's turn text plus any attachments.
    UserMessage {
        /// The submitted text.
        text: String,
        /// Attached files/images, if any.
        attachments: Vec<Attachment>,
    },
    /// The assistant's visible reply text.
    AssistantMessage {
        /// The accumulated text (from `TextDelta`s).
        text: String,
    },
    /// An extended-thinking block.
    Thinking {
        /// The accumulated thinking text.
        text: String,
        /// The provider's signature for replaying the block back, if it requires one.
        signature: Option<String>,
    },
    /// A tool call the model requested.
    ToolCall {
        /// The call itself.
        call: ToolCall,
    },
    /// A finished tool call's result.
    ToolResult {
        /// The call this result belongs to.
        call_id: CallId,
        /// The result.
        result: ToolResult,
    },
    /// A compaction summary.
    Summary {
        /// The summary text.
        text: String,
    },
    /// A cox-generated notice (not part of the model-visible transcript).
    Notice {
        /// The notice's severity.
        level: Level,
        /// The notice text.
        text: String,
    },
}

/// One entry in a session's rebuilt history: an id plus its kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Item {
    /// The item's id, shared by its `ItemStarted`/`ItemDone` pair.
    pub id: ItemId,
    /// The turn this item belongs to, if any (notices between turns have none).
    pub turn: Option<TurnId>,
    /// What the item is and its (possibly still-accumulating) content.
    pub kind: ItemKind,
}

/// Per-request token/cost accounting (plan.md §1.2/§1.9); one row per provider call.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Usage {
    /// Input tokens billed at the full rate.
    pub input_tokens: u32,
    /// Output tokens generated.
    pub output_tokens: u32,
    /// Input tokens served from cache (billed at the cache-read rate).
    pub cache_read_tokens: u32,
    /// Input tokens written to cache (billed at the cache-write rate).
    pub cache_write_tokens: u32,
    /// True when the provider reported no usage and cox estimated it.
    pub estimated: bool,
    /// Computed cost of this call, in USD.
    pub cost_usd: f64,
    /// Wall-clock latency of this call.
    pub latency_ms: u64,
}

impl Usage {
    /// Tokens the model actually saw for this call: input + cache read + cache write
    /// (plan.md §1.9: "context_tokens ... writes ... (input + cache read + cache write)").
    /// Excludes `output_tokens`, which the model produced rather than read.
    pub fn context_tokens(&self) -> u32 {
        self.input_tokens
            .saturating_add(self.cache_read_tokens)
            .saturating_add(self.cache_write_tokens)
    }
}

/// A parsed `.claude/commands/*.md`-style slash command the surface could
/// not resolve to a built-in `Submission` variant, forwarded to `cox-ext`
/// for execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SlashCommand {
    /// The command name, without the leading `/`.
    pub name: String,
    /// Everything after the command name, already tokenized.
    pub args: Vec<String>,
}

/// A hook runner's verdict for one hook invocation (plan.md §1.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HookOutcome {
    /// Proceed unchanged.
    Continue,
    /// Stop the action the hook gated.
    Block {
        /// Shown to the user/model as the reason.
        reason: String,
    },
    /// Proceed with different input (e.g. a rewritten prompt or tool input).
    Modify {
        /// The replacement input.
        input: Value,
    },
    /// The hook itself failed to run; fail-open per D14/AGENTS.md.
    Failed {
        /// What went wrong (timeout, non-zero exit, bad JSON).
        error: String,
    },
}

/// A submission into the core state machine: everything a surface can ask
/// `cox-core` to do (plan.md §1.2/§1.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Submission {
    /// Start (or continue) a turn with user text.
    UserTurn {
        /// The submitted text.
        text: String,
        /// Attached files/images.
        attachments: Vec<Attachment>,
        /// Required to be `true` for the turn to route to `Tier::Think` (plan.md invariant #9).
        confirm_think: bool,
    },
    /// Answer a pending `ApprovalRequired`.
    Approve {
        /// The call being decided.
        call_id: CallId,
        /// The decision.
        decision: Decision,
    },
    /// Cancel the running turn; tools get the shared cancellation token.
    Interrupt,
    /// Compact the session now, optionally focused on something specific.
    Compact {
        /// What the summary should emphasise, if given.
        focus: Option<String>,
    },
    /// Change the tier's model for the rest of the session.
    SwitchModel {
        /// Which tier to change.
        tier: Tier,
        /// The model to switch to; `None` restores the tier's configured default.
        model: Option<ModelId>,
    },
    /// Change the active permission mode.
    SetPermissionMode {
        /// The new mode.
        mode: PermissionMode,
    },
    /// A slash command the surface parsed but did not resolve itself.
    Command {
        /// The parsed command.
        command: SlashCommand,
    },
    /// A hook runner's result for a hook the core is waiting on.
    HookResult {
        /// The hook invocation's id.
        hook_id: String,
        /// The outcome.
        outcome: HookOutcome,
    },
    /// Wind down the session cleanly.
    Shutdown,
}

/// Everything a consumer (TUI, `stream-json`, ACP, the rollout file) can
/// observe from a session (plan.md §1.2/§1.3). Every surface consumes the
/// same stream; nothing is emitted after `TurnDone` for that turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// A new session began.
    SessionStarted {
        /// The session's id.
        session: SessionId,
        /// Hash of the effective config, for `resume_builds_identical_request`-style checks.
        config_digest: String,
        /// The working directory the session started in.
        cwd: PathBuf,
    },
    /// A new turn began.
    TurnStarted {
        /// The turn's id.
        turn: TurnId,
        /// Which job this turn is (usually `Job::Main`).
        job: Job,
        /// The tier routed to.
        tier: Tier,
        /// The specific model used.
        model: ModelId,
    },
    /// A new transcript item began accumulating.
    ItemStarted {
        /// The item's id.
        item: ItemId,
        /// What kind of item this is (may still be filling in text).
        kind: ItemKind,
    },
    /// Streamed text for an `AssistantMessage` item.
    TextDelta {
        /// The item this delta belongs to.
        item: ItemId,
        /// The next chunk of text.
        text: String,
    },
    /// Streamed text for a `Thinking` item.
    ThinkingDelta {
        /// The item this delta belongs to.
        item: ItemId,
        /// The next chunk of thinking text.
        text: String,
    },
    /// The model requested a tool call.
    ToolCallRequested {
        /// The requested call.
        call: ToolCall,
    },
    /// A tool call needs a decision before it can run.
    ApprovalRequired {
        /// The call awaiting a decision.
        call: ToolCall,
        /// Why it needs one.
        why: Why,
    },
    /// A pending approval was decided.
    ApprovalDecided {
        /// The call that was decided.
        call_id: CallId,
        /// The decision.
        decision: Decision,
        /// Who/what decided it.
        by: DecidedBy,
    },
    /// Streamed stdout/stderr from a running tool, already sanitised for display.
    ToolCallOutput {
        /// The call producing output.
        call_id: CallId,
        /// The next chunk of output.
        delta: String,
    },
    /// A tool call finished.
    ToolCallDone {
        /// The call that finished.
        call_id: CallId,
        /// Its result.
        result: ToolResult,
    },
    /// A transcript item finished accumulating.
    ItemDone {
        /// The item that finished.
        item: ItemId,
    },
    /// A provider call's usage/cost was recorded.
    Usage {
        /// The turn this usage belongs to.
        turn: TurnId,
        /// The recorded usage.
        usage: Usage,
    },
    /// Compaction ran and replaced older items with a summary.
    Compacted {
        /// The new summary item's id.
        summary: ItemId,
        /// Items now skipped when building requests (rollout keeps them).
        dropped: Vec<ItemId>,
        /// Context tokens before compaction.
        before_tokens: u32,
        /// Context tokens after compaction.
        after_tokens: u32,
    },
    /// A background subagent task was created.
    TaskCreated {
        /// The task's id.
        task: TaskId,
        /// A short human-readable label.
        label: String,
        /// The tier it runs on.
        tier: Tier,
    },
    /// A background subagent task finished.
    TaskCompleted {
        /// The task that finished.
        task: TaskId,
        /// The item holding its result.
        result_item: ItemId,
        /// What it cost, in USD.
        cost_usd: f64,
    },
    /// A tier's model changed mid-session.
    ModelSwitched {
        /// Which tier changed.
        tier: Tier,
        /// The previous model.
        from: ModelId,
        /// The new model.
        to: ModelId,
    },
    /// An informational or warning message, not part of the model-visible transcript.
    Notice {
        /// Severity.
        level: Level,
        /// The message.
        text: String,
    },
    /// A turn finished.
    TurnDone {
        /// The turn that finished.
        turn: TurnId,
        /// Why it stopped.
        stop: StopReason,
    },
    /// An error occurred.
    Error {
        /// What went wrong.
        error: CoreError,
        /// Whether the whole session must end (`StoreError::Corrupt`, `Config`) or just the turn.
        fatal: bool,
    },
}

// ---------------------------------------------------------------------
// Provider-neutral request/response shapes (plan.md §1.2)
// ---------------------------------------------------------------------

/// A newtype around a provider's model identifier (e.g. `"claude-sonnet-5"`).
/// Deliberately a bare string, not an enum: models are configured, not
/// compiled in (`config/default.toml` [tiers.*].model).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct ModelId(pub String);

impl std::fmt::Display for ModelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Which provider backend is in play; matches the `[providers.*]` config sections (D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    /// Anthropic Messages API.
    Anthropic,
    /// OpenAI Responses or Chat Completions API.
    OpenAi,
    /// A local OpenAI-compatible server (Ollama, vLLM, LM Studio, …).
    Local,
}

/// One block of the system prompt, with its own cache eligibility (plan.md §1.9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SystemBlock {
    /// The block's text.
    pub text: String,
    /// Whether this block may sit before a cache breakpoint.
    pub cache: bool,
}

/// Who sent a `Message` in a `Request`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// The end user (also carries tool results, by provider convention).
    User,
    /// The model.
    Assistant,
}

/// One piece of a `Message`'s content (plan.md §1.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    /// Plain text.
    Text {
        /// The text.
        text: String,
    },
    /// An extended-thinking block, replayed back to providers that require it verbatim.
    Thinking {
        /// The thinking text.
        text: String,
        /// The provider's signature for this block, if required.
        signature: Option<String>,
    },
    /// The model's request to use a tool.
    ToolUse {
        /// The call's id.
        id: CallId,
        /// The tool name.
        name: String,
        /// The (possibly still-accumulating) input.
        input: Value,
    },
    /// A tool's result, sent back to the model.
    ToolResult {
        /// Which call this answers.
        call_id: CallId,
        /// The result content (already truncated/sanitised for the model).
        content: String,
        /// Whether the tool call failed.
        is_error: bool,
    },
    /// An inline image.
    Image {
        /// MIME type.
        media_type: String,
        /// Base64-encoded bytes.
        data_b64: String,
    },
    /// A reference to archived content instead of the content itself (microcompaction).
    Pointer {
        /// Where the full content lives.
        archive: ArchiveRef,
        /// A short description shown in its place.
        summary: String,
    },
}

/// One message in a `Request`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Message {
    /// Who sent it.
    pub role: Role,
    /// Its content blocks.
    pub content: Vec<Content>,
}

/// A provider-neutral request; providers translate this to their own wire
/// format, and nothing above `cox-provider` knows what that format is
/// (plan.md §1.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Request {
    /// The routing tier this request was assembled for.
    pub tier: Tier,
    /// The job this request serves.
    pub job: Job,
    /// The specific model to call.
    pub model: ModelId,
    /// System prompt blocks, in cache-stable order (plan.md §1.9).
    pub system: Vec<SystemBlock>,
    /// Available tool specs, already filtered (deferred tools absent unless discovered).
    pub tools: Vec<ToolSpec>,
    /// The conversation so far.
    pub messages: Vec<Message>,
    /// Requested reasoning effort.
    pub effort: Effort,
    /// Max output tokens.
    pub max_tokens: u32,
    /// Extended-thinking mode.
    pub thinking: Thinking,
    /// Indices into `system` + `messages` (in that concatenated order) marking cache breakpoints; at most 3.
    pub cache_breakpoints: Vec<usize>,
    /// Sequences that stop generation.
    pub stop_sequences: Vec<String>,
}

/// One event from a provider's streamed response (plan.md §1.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProviderEvent {
    /// The stream started; names the model that answered (may differ from the requested alias).
    MessageStart {
        /// The model that is responding.
        model: ModelId,
    },
    /// The next chunk of assistant text.
    TextDelta {
        /// The text chunk.
        text: String,
    },
    /// The next chunk of thinking text.
    ThinkingDelta {
        /// The text chunk.
        text: String,
    },
    /// A tool-use block started.
    ToolUseStart {
        /// The call's id.
        id: CallId,
        /// The tool name.
        name: String,
    },
    /// The next chunk of a tool-use block's JSON input.
    ToolUseInputDelta {
        /// The raw JSON chunk (accumulate and parse once `ToolUseEnd` arrives).
        text: String,
    },
    /// The current tool-use block finished.
    ToolUseEnd,
    /// The stream stopped.
    Stop {
        /// Why it stopped.
        stop: StopReason,
    },
    /// Final usage for this call.
    Usage {
        /// The recorded usage.
        usage: Usage,
    },
    /// The provider is retrying after a transient failure.
    Retrying {
        /// Which retry attempt this is (1-based).
        attempt: u32,
        /// How long cox waited before this attempt.
        after_ms: u64,
    },
    /// The call failed.
    Error {
        /// The failure.
        error: crate::errors::ProviderError,
    },
}

/// What a provider implementation can do; used to skip unsupported request shapes
/// instead of sending them and getting `ProviderError::Unsupported`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Caps {
    /// Supports prompt caching (`cache_control`/automatic prefix caching).
    pub cache: bool,
    /// Supports extended/adaptive thinking.
    pub thinking: bool,
    /// Supports first-party server tools (web search/fetch passthrough).
    pub server_tools: bool,
    /// Supports a dedicated token-counting endpoint.
    pub count_tokens: bool,
    /// The model's max context window, in tokens.
    pub max_context: u32,
}

/// A tool's advertised shape (plan.md §1.2/§1.11).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolSpec {
    /// The tool's registered name.
    pub name: String,
    /// Shown to the model.
    pub description: String,
    /// JSON Schema for the tool's input.
    pub input_schema: Value,
    /// True for tools found only through `tool_search`, absent from `system[0]` until discovered.
    pub deferred: bool,
    /// Default risk classification for calls to this tool.
    pub risk: Risk,
    /// Whether calls to this tool may run in parallel with others.
    pub concurrency: Concurrency,
}

/// A tool's raw result, before the core archives and truncates it (plan.md §1.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolOutput {
    /// The full, untruncated output.
    pub text: String,
    /// Whether the call failed.
    pub is_error: bool,
    /// A unified diff, for edit-shaped tools.
    pub diff: Option<Diff>,
    /// Machine-readable payload alongside `text`, for surfaces that want structure.
    pub structured: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    fn sample_usage() -> Usage {
        Usage {
            input_tokens: 100,
            output_tokens: 20,
            cache_read_tokens: 30,
            cache_write_tokens: 5,
            estimated: false,
            cost_usd: 0.01,
            latency_ms: 250,
        }
    }

    #[test]
    fn usage_sums_cache_fields() {
        let usage = sample_usage();
        assert_eq!(usage.context_tokens(), 100 + 30 + 5);
    }

    #[rstest]
    #[case::session_started(Event::SessionStarted { session: SessionId::new(), config_digest: "deadbeef".into(), cwd: PathBuf::from("/tmp") })]
    #[case::turn_started(Event::TurnStarted { turn: TurnId::new(), job: Job::Main, tier: Tier::Code, model: ModelId("claude-sonnet-5".into()) })]
    #[case::item_started(Event::ItemStarted { item: ItemId::new(), kind: ItemKind::UserMessage { text: "hi".into(), attachments: vec![] } })]
    #[case::text_delta(Event::TextDelta { item: ItemId::new(), text: "chunk".into() })]
    #[case::thinking_delta(Event::ThinkingDelta { item: ItemId::new(), text: "chunk".into() })]
    #[case::tool_call_requested(Event::ToolCallRequested { call: ToolCall { id: CallId::new(), name: "read".into(), input: serde_json::json!({"path": "a.rs"}), risk: Risk::ReadOnly, subject: "a.rs".into() } })]
    #[case::approval_required(Event::ApprovalRequired { call: ToolCall { id: CallId::new(), name: "bash".into(), input: Value::Null, risk: Risk::Exec, subject: "ls".into() }, why: Why::Risk { risk: Risk::Exec } })]
    #[case::approval_decided(Event::ApprovalDecided { call_id: CallId::new(), decision: Decision::Allow, by: DecidedBy::User })]
    #[case::tool_call_output(Event::ToolCallOutput { call_id: CallId::new(), delta: "stdout line".into() })]
    #[case::tool_call_done(Event::ToolCallDone { call_id: CallId::new(), result: ToolResult { ok: true, visible: "done".into(), archive: None, bytes: 4, duration_ms: 10, diff: None } })]
    #[case::item_done(Event::ItemDone { item: ItemId::new() })]
    #[case::usage(Event::Usage { turn: TurnId::new(), usage: sample_usage() })]
    #[case::compacted(Event::Compacted { summary: ItemId::new(), dropped: vec![ItemId::new()], before_tokens: 1000, after_tokens: 200 })]
    #[case::task_created(Event::TaskCreated { task: TaskId::new(), label: "explore".into(), tier: Tier::Cheap })]
    #[case::task_completed(Event::TaskCompleted { task: TaskId::new(), result_item: ItemId::new(), cost_usd: 0.002 })]
    #[case::model_switched(Event::ModelSwitched { tier: Tier::Code, from: ModelId("claude-sonnet-5".into()), to: ModelId("claude-opus-5".into()) })]
    #[case::notice(Event::Notice { level: Level::Warn, text: "hook skipped".into() })]
    #[case::turn_done(Event::TurnDone { turn: TurnId::new(), stop: StopReason::EndTurn })]
    #[case::error(Event::Error { error: CoreError::Interrupted, fatal: false })]
    fn event_json_roundtrip(#[case] event: Event) {
        let json = serde_json::to_string(&event).expect("serialize");
        let back: Event = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(event, back);
    }

    #[rstest]
    #[case::user_turn(Submission::UserTurn { text: "fix the bug".into(), attachments: vec![], confirm_think: false })]
    #[case::approve(Submission::Approve { call_id: CallId::new(), decision: Decision::Deny { reason: "no".into() } })]
    #[case::interrupt(Submission::Interrupt)]
    #[case::compact(Submission::Compact { focus: Some("auth flow".into()) })]
    #[case::switch_model(Submission::SwitchModel { tier: Tier::Code, model: Some(ModelId("claude-opus-5".into())) })]
    #[case::set_permission_mode(Submission::SetPermissionMode { mode: PermissionMode::Plan })]
    #[case::command(Submission::Command { command: SlashCommand { name: "compact".into(), args: vec![] } })]
    #[case::hook_result(Submission::HookResult { hook_id: "pre-tool-use".into(), outcome: HookOutcome::Continue })]
    #[case::shutdown(Submission::Shutdown)]
    fn submission_json_roundtrip(#[case] submission: Submission) {
        let json = serde_json::to_string(&submission).expect("serialize");
        let back: Submission = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(submission, back);
    }

    /// plan.md T0.2 step 4: grep the serialized form for the `type` tag and
    /// assert it is snake_case, over one value per `Event` variant.
    #[rstest]
    #[case::session_started(Event::SessionStarted { session: SessionId::new(), config_digest: "d".into(), cwd: PathBuf::from(".") })]
    #[case::turn_started(Event::TurnStarted { turn: TurnId::new(), job: Job::Main, tier: Tier::Code, model: ModelId("m".into()) })]
    #[case::tool_call_done(Event::ToolCallDone { call_id: CallId::new(), result: ToolResult { ok: true, visible: "ok".into(), archive: None, bytes: 0, duration_ms: 0, diff: None } })]
    #[case::model_switched(Event::ModelSwitched { tier: Tier::Cheap, from: ModelId("a".into()), to: ModelId("b".into()) })]
    fn event_tags_are_snake_case(#[case] event: Event) {
        let json = serde_json::to_value(&event).expect("serialize");
        let tag = json
            .get("type")
            .and_then(|v| v.as_str())
            .expect("has type tag");
        assert!(
            tag.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "tag {tag:?} is not snake_case"
        );
    }

    #[test]
    fn tool_spec_schema_generates() {
        #[derive(JsonSchema)]
        #[allow(dead_code)]
        struct ReadInput {
            path: String,
            lines: Option<String>,
        }

        let schema = schemars::schema_for!(ReadInput);
        let schema_value = serde_json::to_value(&schema).expect("schema serializes");
        let spec = ToolSpec {
            name: "read".into(),
            description: "Read a file".into(),
            input_schema: schema_value.clone(),
            deferred: false,
            risk: Risk::ReadOnly,
            concurrency: Concurrency::Parallel,
        };
        assert_eq!(spec.input_schema, schema_value);
        assert!(schema_value.get("properties").is_some());
    }
}
