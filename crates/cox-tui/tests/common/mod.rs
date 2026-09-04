//! Helpers the TUI tests share: driving `update` with typed keys, so a test
//! reaches a slash command the way a user does.

use cox_tui::state::{Msg, State, update};
use crossterm::event::{KeyCode, KeyEvent};

/// Types `line` and presses Enter. A `/` at column 0 opens the palette; Esc
/// closes it and the rest is typed as text.
pub fn type_line(state: &mut State, line: &str) {
    let mut chars = line.chars();
    if let Some(c) = chars.next() {
        update(state, Msg::Key(KeyEvent::from(KeyCode::Char(c))));
        if c == '/' {
            update(state, Msg::Key(KeyEvent::from(KeyCode::Esc)));
        }
    }
    for c in chars {
        update(state, Msg::Key(KeyEvent::from(KeyCode::Char(c))));
    }
    update(state, Msg::Key(KeyEvent::from(KeyCode::Enter)));
}
