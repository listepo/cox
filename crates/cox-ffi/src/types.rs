//! The records and enums Swift sees (DT§4.4): cox-app's and cox-protocol's
//! own types, declared to UniFFI with `#[uniffi::remote]` rather than
//! mirrored, so there is no conversion code and a field added upstream
//! fails this build instead of drifting. Separate from the exported objects
//! because these are data only. Ids and paths cross as strings; the one
//! type UniFFI cannot carry as-is (a span's `[u8; 3]` colour) crosses as
//! the local `Span`, and the one record only this surface has
//! (`OpenRequest`) is declared here too.

use std::path::PathBuf;

use cox_app::Holder;
use cox_app::SessionInfo;
use cox_app::diffmodel::{DiffHunk, DiffLine, DiffLineKind, DiffModel};
use cox_app::doc::{Block as DocBlock, StyledDoc, StyledSpan, TextKind};
use cox_app::patch::{Block, BlockId, BlockKind, TimelinePatch, ToolState};
use cox_app::{
    Activity, ChangedFile, Changes, Checkpoint, Completion, Dropped, FileChange, Icon, InboxItem,
    Intent, Layer, Linked, McpLogin, McpServer, Need, Project, SearchHit, SessionEntry, Setting,
    SettingKind, SettingsView, Tally, TurnUsage, UsageView,
};
use cox_protocol::ids::{ArchiveId, CallId, SessionId, TaskId, TurnId};
use cox_protocol::plugin::ui::StyleToken;
use cox_protocol::traits::WorktreeInfo;
use cox_protocol::types::{
    ApprovalPolicy, ArchiveRef, Attachment, CompactReason, DecidedBy, Decision, Effort, Level,
    ModelId, PermissionMode, Risk, Segments, Source, StopReason, Tier, ToolCall, Usage, Why,
};
use serde_json::Value;

macro_rules! string_ids {
    ($($id:ident),*) => {$(
        uniffi::custom_type!($id, String, {
            remote,
            lower: |id| id.to_string(),
            try_lift: |s| Ok(s.parse()?),
        });
    )*};
}
string_ids!(SessionId, TurnId, CallId, ArchiveId, TaskId);

uniffi::custom_type!(BlockId, String, {
    remote,
    lower: |id| id.0,
    try_lift: |s| Ok(BlockId(s)),
});
uniffi::custom_type!(ModelId, String, {
    remote,
    lower: |id| id.0,
    try_lift: |s| Ok(ModelId(s)),
});
uniffi::custom_type!(PathBuf, String, {
    remote,
    lower: |path| path.to_string_lossy().into_owned(),
    try_lift: |s| Ok(PathBuf::from(s)),
});
// A tool's input and an edited approval: JSON text, as on the wire.
uniffi::custom_type!(Value, String, {
    remote,
    lower: |value| value.to_string(),
    try_lift: |s| Ok(serde_json::from_str(&s)?),
});
uniffi::custom_type!(StyledSpan, Span, {
    remote,
    lower: |s| Span {
        rgb: s.rgb.map(|[r, g, b]| u32::from_be_bytes([0, r, g, b])),
        text: s.text,
        token: s.token,
        bold: s.bold,
        italic: s.italic,
        strike: s.strike,
        underline: s.underline,
        link: s.link,
    },
    try_lift: |s| Ok(StyledSpan {
        rgb: s.rgb.map(|c| {
            let [_, r, g, b] = c.to_be_bytes();
            [r, g, b]
        }),
        text: s.text,
        token: s.token,
        bold: s.bold,
        italic: s.italic,
        strike: s.strike,
        underline: s.underline,
        link: s.link,
    }),
});

#[uniffi::remote(Record)]
pub struct Project {
    pub root: PathBuf,
    pub name: String,
    pub sessions: u64,
    pub cost_usd: f64,
    pub updated_at: String,
}

/// What `App::open` opens: a new session in `cwd`, or `resume`'s.
/// `theme` is the syntect theme code blocks are highlighted with.
#[derive(uniffi::Record)]
pub struct OpenRequest {
    pub cwd: String,
    pub resume: Option<SessionId>,
    pub theme: String,
}

/// `StyledSpan` with its theme colour as `0xRRGGBB`.
#[derive(uniffi::Record)]
pub struct Span {
    pub text: String,
    pub token: StyleToken,
    pub rgb: Option<u32>,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub underline: bool,
    pub link: Option<String>,
}

#[uniffi::remote(Enum)]
pub enum TimelinePatch {
    Reset {
        blocks: Vec<Block>,
    },
    Upsert {
        block: Box<Block>,
        after: Option<BlockId>,
    },
    AppendText {
        id: BlockId,
        text: String,
    },
    DocTail {
        id: BlockId,
        from: u32,
        blocks: Vec<DocBlock>,
    },
    Remove {
        id: BlockId,
    },
    Usage {
        usage: Box<UsageView>,
    },
}

#[uniffi::remote(Record)]
pub struct Block {
    pub id: BlockId,
    pub turn: u32,
    pub kind: BlockKind,
}

#[uniffi::remote(Enum)]
pub enum BlockKind {
    User {
        text: String,
        attachments: Vec<String>,
    },
    Assistant {
        text: String,
        doc: StyledDoc,
    },
    Thinking {
        text: String,
    },
    Tool {
        tool: String,
        summary: String,
        icon: Icon,
        risk: Risk,
        state: ToolState,
        tail: String,
        archive: Option<ArchiveRef>,
        diff: Option<DiffModel>,
        duration_ms: u64,
    },
    ToolGroup {
        summary: String,
        children: Vec<BlockId>,
        state: ToolState,
    },
    Approval {
        call: CallId,
        tool: String,
        summary: String,
        why: Why,
        source: Option<Source>,
        decision: Option<Decision>,
        by: Option<DecidedBy>,
    },
    Question {
        call: CallId,
        question: String,
        options: Vec<String>,
        answer: Option<String>,
    },
    Task {
        task: TaskId,
        label: String,
        tier: Tier,
        done: bool,
        cost_usd: f64,
        exit_code: Option<i32>,
    },
    Compaction {
        before_tokens: u32,
        after_tokens: u32,
        reason: CompactReason,
        summary: Option<String>,
    },
    Checkpoint {
        files: Vec<PathBuf>,
    },
    Notice {
        level: Level,
        text: String,
    },
    Error {
        text: String,
        fatal: bool,
    },
    TurnMeta {
        model: ModelId,
        tier: Tier,
        usage: Option<Usage>,
        stop: Option<StopReason>,
    },
}

#[uniffi::remote(Enum)]
pub enum ToolState {
    Running,
    Done,
    Failed,
}

#[uniffi::remote(Enum)]
pub enum Icon {
    Read,
    Edit,
    Shell,
    Search,
    Web,
    Todo,
    Ask,
    Agent,
    Mcp,
    Tool,
}

#[uniffi::remote(Record)]
pub struct StyledDoc {
    pub blocks: Vec<DocBlock>,
}

#[uniffi::remote(Enum)]
pub enum DocBlock {
    Text {
        kind: TextKind,
        lines: Vec<Vec<StyledSpan>>,
    },
    Code {
        lang: String,
        lines: Vec<Vec<StyledSpan>>,
    },
    Table {
        rows: Vec<Vec<String>>,
    },
    Rule,
}

#[uniffi::remote(Enum)]
pub enum TextKind {
    Paragraph,
    Heading(u8),
    List,
    Quote,
}

#[uniffi::remote(Enum)]
pub enum StyleToken {
    Text,
    Dim,
    Accent,
    User,
    Agent,
    Tool,
    Ok,
    Warn,
    Error,
    DiffAdd,
    DiffDel,
    DiffHunk,
    Border,
    Selection,
}

#[uniffi::remote(Record)]
pub struct UsageView {
    pub session: Tally,
    pub turn: Option<TurnUsage>,
    pub context_tokens: u32,
}

#[uniffi::remote(Record)]
pub struct Tally {
    pub sent: u32,
    pub received: u32,
    pub cache_read: u32,
    pub cache_write: u32,
    pub uncached: u32,
    pub cost_usd: f64,
    pub calls: u32,
    pub estimated: bool,
}

#[uniffi::remote(Record)]
pub struct TurnUsage {
    pub turn: TurnId,
    pub tally: Tally,
    pub thinking_tokens: u32,
    pub ttft_ms: Option<u64>,
    pub tok_per_s: Option<f64>,
    pub exact: bool,
    pub sparkline: Vec<f64>,
    pub done: bool,
}

#[uniffi::remote(Enum)]
pub enum Intent {
    Send {
        text: String,
        attachments: Vec<Attachment>,
    },
    Approve {
        call: CallId,
        decision: Decision,
    },
    Answer {
        question: CallId,
        text: Option<String>,
    },
    Interrupt,
    Queue {
        text: String,
    },
    Compact {
        focus: Option<String>,
    },
    SetMode {
        mode: PermissionMode,
    },
    SwitchModel {
        tier: Tier,
        model: Option<ModelId>,
    },
    SetEffort {
        effort: Option<Effort>,
    },
    Rewind {
        to_turn: u32,
        code: bool,
        conversation: bool,
    },
    Redo,
    Fork {
        turn: Option<u32>,
    },
    Handoff {
        objective: String,
    },
    Background {
        call: CallId,
    },
    Shell {
        command: String,
        share: bool,
    },
    Command {
        line: String,
    },
}

#[uniffi::remote(Record)]
pub struct InboxItem {
    pub session: SessionId,
    pub source: Option<Source>,
    pub need: Need,
    pub expired: bool,
    pub seq: u64,
}

#[uniffi::remote(Enum)]
pub enum Need {
    Approval {
        call: ToolCall,
        why: Why,
    },
    Question {
        call_id: CallId,
        question: String,
        options: Vec<String>,
    },
    Failed {
        text: String,
    },
    TaskDone {
        task: TaskId,
        label: String,
        ok: bool,
    },
}

#[uniffi::remote(Enum)]
pub enum Activity {
    Idle,
    Running,
    WaitingOnYou,
    Failed,
}

#[uniffi::remote(Record)]
pub struct SessionEntry {
    pub info: SessionInfo,
    pub held_by: Option<Holder>,
}

#[uniffi::remote(Record)]
pub struct SessionInfo {
    pub id: String,
    pub title: Option<String>,
    pub cwd: String,
    pub created_at: String,
    pub updated_at: String,
    pub turns: i64,
    pub cost_usd: f64,
}

#[uniffi::remote(Record)]
pub struct Holder {
    pub pid: u32,
    pub surface: String,
    pub since: String,
}

#[uniffi::remote(Record)]
pub struct SearchHit {
    pub session: SessionInfo,
    pub turn: i64,
    pub snippet: String,
}

#[uniffi::remote(Record)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub main: bool,
    pub locked: Option<String>,
    pub stale: bool,
    pub merged: bool,
    pub bytes: u64,
}

#[uniffi::remote(Record)]
pub struct Changes {
    pub files: Vec<ChangedFile>,
    pub checkpoints: Vec<Checkpoint>,
    pub worktree: Option<Linked>,
}

#[uniffi::remote(Record)]
pub struct ChangedFile {
    pub path: PathBuf,
    pub change: FileChange,
    pub added: u32,
    pub removed: u32,
    pub call: CallId,
    pub turn: u32,
}

#[uniffi::remote(Enum)]
pub enum FileChange {
    Edited,
    Created,
    Deleted,
}

#[uniffi::remote(Record)]
pub struct Checkpoint {
    pub turn: u32,
    pub label: String,
    pub time: String,
}

#[uniffi::remote(Record)]
pub struct Linked {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub base: Option<String>,
    pub commit: Option<String>,
    pub bytes: u64,
}

#[uniffi::remote(Record)]
pub struct SettingsView {
    pub settings: Vec<Setting>,
    pub user_file: PathBuf,
    pub project_file: Option<PathBuf>,
    pub mcp: Vec<McpServer>,
    pub dropped: Vec<Dropped>,
}

#[uniffi::remote(Record)]
pub struct Dropped {
    pub key: String,
    pub value: String,
    pub kept: String,
    pub reason: String,
}

#[uniffi::remote(Record)]
pub struct McpServer {
    pub name: String,
    pub source: String,
    pub login: McpLogin,
}

#[uniffi::remote(Enum)]
pub enum McpLogin {
    Stdio,
    LoggedOut,
    LoggedIn { expires: Option<String> },
    Expired,
    Unreadable { error: String },
}

#[uniffi::remote(Record)]
pub struct Setting {
    pub key: String,
    pub value: Value,
    pub layer: Layer,
    pub editable: bool,
    pub kind: SettingKind,
    pub description: String,
}

#[uniffi::remote(Enum)]
pub enum Layer {
    Default,
    User,
    Project,
    ClaudeSettings,
    Env,
    Flag,
}

#[uniffi::remote(Enum)]
pub enum SettingKind {
    Toggle,
    Integer { min: Option<f64>, max: Option<f64> },
    Number { min: Option<f64>, max: Option<f64> },
    Text,
    Choice { options: Vec<String> },
    List,
    Other,
}

#[uniffi::remote(Record)]
pub struct Completion {
    pub insert: String,
    pub detail: String,
}

#[uniffi::remote(Record)]
pub struct ToolCall {
    pub id: CallId,
    pub name: String,
    pub input: Value,
    pub risk: Risk,
    pub subject: String,
    pub segments: Option<Segments>,
}

#[uniffi::remote(Record)]
pub struct Segments {
    pub commands: Vec<String>,
    pub opaque: bool,
}

#[uniffi::remote(Record)]
pub struct Attachment {
    pub name: String,
    pub media_type: String,
    pub data_b64: String,
}

#[uniffi::remote(Record)]
pub struct DiffModel {
    pub path: PathBuf,
    pub hunks: Vec<DiffHunk>,
}

#[uniffi::remote(Record)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[uniffi::remote(Record)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub spans: Vec<StyledSpan>,
}

#[uniffi::remote(Enum)]
pub enum DiffLineKind {
    Context,
    Add,
    Del,
}

#[uniffi::remote(Record)]
pub struct ArchiveRef {
    pub id: ArchiveId,
    pub bytes: u64,
}

#[uniffi::remote(Record)]
pub struct Source {
    pub session: SessionId,
    pub agent: Option<String>,
    pub preset: Option<String>,
}

#[uniffi::remote(Record)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_write_tokens: u32,
    pub estimated: bool,
    pub cost_usd: f64,
    pub latency_ms: u64,
}

#[uniffi::remote(Enum)]
pub enum Decision {
    Allow,
    AllowForSession,
    Deny { reason: String },
    Edit { input: Value },
}

#[uniffi::remote(Enum)]
pub enum Why {
    RuleAsk { rule: String },
    Risk { risk: Risk },
    SandboxDenied { detail: String },
    Policy { policy: ApprovalPolicy },
}

#[uniffi::remote(Enum)]
pub enum StopReason {
    EndTurn,
    MaxTurns,
    Interrupted,
    Budget,
    Refusal { detail: String },
    Error,
}

#[uniffi::remote(Enum)]
pub enum ApprovalPolicy {
    Untrusted,
    OnRequest,
    OnFailure,
    Never,
}

#[uniffi::remote(Enum)]
pub enum DecidedBy {
    User,
    Rule,
    Session,
    Policy,
    Hook,
}

#[uniffi::remote(Enum)]
pub enum Risk {
    ReadOnly,
    Write,
    Exec,
    Destructive,
}

#[uniffi::remote(Enum)]
pub enum Tier {
    Cheap,
    Code,
    Think,
}

#[uniffi::remote(Enum)]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
}

#[uniffi::remote(Enum)]
pub enum PermissionMode {
    Default,
    Plan,
    Auto,
    Bypass,
}

#[uniffi::remote(Enum)]
pub enum CompactReason {
    PreCall,
    PostTurn,
    Manual,
    ContextTooLong,
}

#[uniffi::remote(Enum)]
pub enum Level {
    Info,
    Warn,
    Budget,
    Security,
}
