//! `cox-app` (DT§4.3): the UI-agnostic application core the desktop app
//! drives through `cox-ffi`. Pure logic over the core's `Event` stream, kept
//! out of every surface crate so the TUI could share it later and no UI
//! toolkit leaks in (`crates/cox/tests/deps.rs`).

pub mod patch;
pub mod summary;
pub mod timeline;

pub use patch::{Block, BlockId, BlockKind, TimelinePatch, ToolState};
pub use summary::Icon;
pub use timeline::Timeline;
