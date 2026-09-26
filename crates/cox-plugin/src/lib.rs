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
//! - [`install`] — the user plugin's on-disk layout: staging a package into
//!   `versions/<digest12>/` and the `current`/`previous` swap (PL§1b, T33.31).
//! - [`hostfn`] — the `cox:host/v1` host functions, each checked against
//!   the grant and the calling export, and `cox_init`'s input (PL§4, T33.9).
//! - [`hooks`] — `PluginHooks`: one plugin as a `Hook` source through
//!   `cox_hook` (PL§6, T33.11).
//! - [`context`] — the event-folded, redacted snapshot `cox_context`
//!   returns (PL§5, T33.9).
//! - [`provider`] — merges a granted plugin's declarative `[[provider]]`
//!   rows into `providers.custom` (PL§7a, T33.17).
//! - [`events`] — the session's `EventTap`: per-plugin drop-oldest rings
//!   delivered to `cox_on_event` in batches (PL§5, T33.10).

#![warn(missing_docs)]

pub mod context;
pub mod discover;
pub mod error;
pub mod events;
pub mod grant;
pub mod hooks;
pub mod host;
pub mod hostfn;
pub mod install;
pub mod provider;

pub use context::Context;
pub use discover::{Discovered, Plugin, Source, State, package_digest};
pub use error::PluginError;
pub use events::{PluginTap, Redraw, subscriptions};
pub use grant::Verdict;
pub use hooks::PluginHooks;
pub use host::{CONTROL_DEPTH, EVENT_DEPTH, Lane, PluginHost};
pub use hostfn::{HostEnv, init_input};
