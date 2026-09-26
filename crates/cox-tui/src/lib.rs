//! The ratatui app in TEA form (`State`, `update`, `view`); all terminal
//! output goes through here. Separate from `cox-core` so the agent loop has
//! no notion of a terminal and the TUI can be tested by feeding it `Event`s.

pub mod app;
pub mod banner;
pub mod cells;
pub mod color;
pub mod commands;
pub mod composer;
pub mod diff;
pub mod glyph;
pub mod keymap;
pub mod link;
pub mod markdown;
pub mod modal;
pub mod picker;
pub mod plugin_ui;
pub mod state;
pub mod status;
pub mod svg;
pub mod tasks;
pub mod term;
pub mod theme;
pub mod view;
pub mod vim;

/// T32.1: `text::sanitize` moved to its own crate (guard (b), reuse (d) —
/// `docs/design/crates.md`); re-exported here at the old path so
/// `cox_tui::text::sanitize` keeps working for existing callers.
pub use cox_sanitize as text;
