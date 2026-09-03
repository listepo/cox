//! The agent loop as a state machine: turns, context assembly, compaction,
//! the permission engine, hooks, model routing, budget. No I/O except
//! through traits in `cox-protocol`, so the loop can be tested by replaying
//! events instead of calling a model.

#![warn(missing_docs)]

mod budget;
mod context;
mod dedup;
mod hooks;
pub mod permission;
mod rollout;
mod session;
pub mod subagent;
mod truncate;
mod turn;

pub use context::{assemble, assemble_with};
pub use permission::{Engine, Outcome};
pub use rollout::History;
pub use session::{MemoryStore, Session};
