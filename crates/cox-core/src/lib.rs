//! The agent loop as a state machine: turns, context assembly, compaction,
//! the permission engine, hooks, model routing, budget. No I/O except
//! through traits in `cox-protocol`, so the loop can be tested by replaying
//! events instead of calling a model.

#![warn(missing_docs)]

mod budget;
pub mod cache_diag;
mod checkpoint;
mod compact;
mod context;
mod dedup;
mod hooks;
pub mod init;
pub mod memory_extract;
pub mod redact;
mod rewind;
mod rollout;
pub mod router;
mod session;
pub mod subagent;
pub mod tasks;
mod truncate;
mod turn;

/// T32.8: the permission engine moved to its own crate (guard (b), pure —
/// `docs/design/crates.md` C8); re-exported here at the old path so
/// `cox_core::permission::Engine` keeps working for existing callers.
pub use cox_permission as permission;

pub use context::{assemble, assemble_with, microcompact};
pub use permission::{Engine, Outcome};
pub use rollout::{History, HistoryTurn};
pub use session::{MemoryStore, Session};
