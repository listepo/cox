//! `cox-app` (DT§4.3): the UI-agnostic application core the desktop app
//! drives through `cox-ffi`. Pure logic over the core's `Event` stream, kept
//! out of every surface crate so the TUI could share it later and no UI
//! toolkit leaks in (`crates/cox/tests/deps.rs`).

pub mod complete;
pub mod inbox;
pub mod intent;
pub mod patch;
pub mod timeline;
pub mod workspace;

pub use complete::{Completer, Completion};
pub use inbox::{Activity, Inbox, InboxItem, Need};
pub use intent::{Dispatch, Intent, IntentError, dispatch};
pub use patch::{Block, BlockId, BlockKind, TimelinePatch, ToolState};
pub use timeline::Timeline;
pub use workspace::{ProjectRow, SearchHit, SessionEntry, Workspace, WorkspaceError};
