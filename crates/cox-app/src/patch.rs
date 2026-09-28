//! The block model and `TimelinePatch` (DT§4.3): what a UI sees instead of
//! `Event`. Separate from the fold so `cox-ffi` (T37.14) mirrors plain data
//! without the fold's bookkeeping; every type is serde, so a recorded patch
//! stream is a fixture the Swift tests replay (DT§8).

use std::path::PathBuf;

use cox_protocol::ids::{CallId, TaskId};
use cox_protocol::types::{
    ArchiveRef, CompactReason, DecidedBy, Decision, Diff, Level, ModelId, Risk, Source, StopReason,
    Tier, Usage, Why,
};
use cox_render::doc::{Block as DocBlock, StyledDoc};
use serde::{Deserialize, Serialize};

use crate::summary::Icon;

/// How many trailing lines of output a running `Tool` block keeps.
pub const TAIL_LINES: usize = 5;

/// A block's key. It is derived from the ids in the events that build the
/// block (or, for blocks without one, from the event's position), so a
/// rollout replayed from the start keys every block as the live run did.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BlockId(pub String);

/// One transcript block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub id: BlockId,
    /// The turn's ordinal (`TurnStarted.seq`); 0 before the first turn.
    pub turn: u32,
    pub kind: BlockKind,
}

/// What a block shows (DT§4.3 block table).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BlockKind {
    /// Attachments by name; their bytes stay in the rollout.
    User {
        text: String,
        attachments: Vec<String>,
    },
    /// `text` is the markdown source, `doc` its parse.
    Assistant {
        text: String,
        doc: StyledDoc,
    },
    Thinking {
        text: String,
    },
    Tool {
        tool: String,
        /// The one-line summary (`summary::summary`), past tense once done.
        summary: String,
        icon: Icon,
        risk: Risk,
        state: ToolState,
        /// The last `TAIL_LINES` lines of output.
        tail: String,
        archive: Option<ArchiveRef>,
        diff: Option<Diff>,
        duration_ms: u64,
    },
    /// Consecutive read/grep/glob/outline calls of one step: "Explored 3
    /// files". The children stay `Tool` blocks right after this one, so
    /// their patches are unchanged; a UI shows them when it expands.
    ToolGroup {
        summary: String,
        children: Vec<BlockId>,
        /// `Running` while any child runs, else `Failed` if any failed.
        state: ToolState,
    },
    Approval {
        call: CallId,
        tool: String,
        summary: String,
        why: Why,
        source: Option<Source>,
        /// `None` while pending.
        decision: Option<Decision>,
        by: Option<DecidedBy>,
    },
    Question {
        call: CallId,
        question: String,
        options: Vec<String>,
        /// `None` while pending.
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
        /// The summary that replaced the dropped turns; `None` when the
        /// stream did not carry its `Summary` item.
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
    /// Summed over the turn's provider calls; `stop` once it ended.
    TurnMeta {
        model: ModelId,
        tier: Tier,
        usage: Option<Usage>,
        stop: Option<StopReason>,
    },
}

/// Where a tool call stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolState {
    Running,
    Done,
    Failed,
}

/// One change to the block list, keyed by id so a UI's identity is stable
/// and a dropped patch is healed by `Reset`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TimelinePatch {
    /// The whole list: open, resume, or healing after a gap.
    Reset {
        blocks: Vec<Block>,
    },
    /// Replaces the block with this id in place, or inserts it after
    /// `after` (`None`: first).
    Upsert {
        block: Box<Block>,
        after: Option<BlockId>,
    },
    /// Appends to a `Thinking` block's text or a `Tool` block's tail (which
    /// then keeps its last `TAIL_LINES` lines).
    AppendText {
        id: BlockId,
        text: String,
    },
    /// Replaces an `Assistant` doc's blocks from index `from` on: blocks a
    /// stream has closed are never re-sent.
    DocTail {
        id: BlockId,
        from: u32,
        blocks: Vec<DocBlock>,
    },
    Remove {
        id: BlockId,
    },
}

/// The last `TAIL_LINES` lines of `text`, a trailing newline kept so the
/// next appended chunk starts its own line.
pub fn tail(text: &str) -> &str {
    let body = text.strip_suffix('\n').unwrap_or(text);
    let start = body
        .rmatch_indices('\n')
        .nth(TAIL_LINES - 1)
        .map_or(0, |(i, _)| i + 1);
    &text[start..]
}
