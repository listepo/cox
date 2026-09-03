//! The Agent Client Protocol adapter over the same `Event` stream used by
//! the TUI. Separate crate because it is an alternate surface (Zed,
//! JetBrains), not a variation of the terminal UI.

pub mod client_tools;
pub mod map;
pub mod server;

pub use client_tools::ClientLink;
pub use server::{FactoryRequest, ServerState, SessionFactory, serve_channel, serve_stdio};
