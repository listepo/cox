//! The WASM plugin host (A52, `docs/design/plugins.md`): loads a plugin
//! module with extism and runs each plugin on a worker thread of its own. A
//! crate of its own because it is the only one that links extism and
//! wasmtime (plan.md §1.1), so nothing else pays for them.
//!
//! - [`host`] — `PluginHost`: load from bytes, the two queues, per-call
//!   deadlines and the memory cap (PL§4).
//! - [`error`] — `PluginError`, mapped from `extism::Error`.
//! - [`discover`] — finds user and project plugins, validates each
//!   manifest and computes its package digest (PL§1, T33.4).
//! - [`grant`] — the pure grant check and the granted-capability list
//!   (PL§3, T33.6).

#![warn(missing_docs)]

pub mod discover;
pub mod error;
pub mod grant;
pub mod host;

pub use discover::{Discovered, Plugin, Source, State, package_digest};
pub use error::PluginError;
pub use grant::Verdict;
pub use host::{CONTROL_DEPTH, EVENT_DEPTH, Lane, PluginHost};
