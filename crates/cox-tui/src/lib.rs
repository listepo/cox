//! The ratatui app in TEA form (`State`, `update`, `view`); all terminal
//! output goes through here. Separate from `cox-core` so the agent loop has
//! no notion of a terminal and the TUI can be tested by feeding it `Event`s.

pub mod app;
pub mod banner;
pub mod composer;
pub mod picker;
pub mod state;
pub mod view;
