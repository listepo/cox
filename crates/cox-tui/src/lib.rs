//! The ratatui app in TEA form (`State`, `update`, `view`); all terminal
//! output goes through here. Separate from `cox-core` so the agent loop has
//! no notion of a terminal and the TUI can be tested by feeding it `Event`s.

pub mod app;
pub mod banner;
pub mod cells;
pub mod commands;
pub mod composer;
pub mod diff;
pub mod glyph;
pub mod markdown;
pub mod modal;
pub mod picker;
pub mod state;
pub mod status;
pub mod tasks;
pub mod text;
pub mod view;
pub mod vim;
