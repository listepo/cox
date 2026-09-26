//! The WASM plugin host (A52, `docs/design/plugins.md`): loads a plugin
//! module with extism and runs each plugin on a worker thread of its own. A
//! crate of its own because it is the only one that links extism and
//! wasmtime (plan.md §1.1), so nothing else pays for them.
//!
//! - [`host`] — `PluginHost`: load from bytes, the two queues, per-call
//!   deadlines and the memory cap (PL§4).
//! - [`error`] — `PluginError`, mapped from `extism::Error`.

#![warn(missing_docs)]

pub mod error;
pub mod host;

pub use error::PluginError;
pub use host::{CONTROL_DEPTH, EVENT_DEPTH, Lane, PluginHost};
