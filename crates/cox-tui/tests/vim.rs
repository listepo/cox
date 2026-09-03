//! Vim-lite (T5.7): a keypress table over the composer with `tui.vim` on.

use cox_protocol::types::{PermissionMode, SandboxMode};
use cox_tui::composer::Composer;
use cox_tui::state::{Msg, State, update};
use cox_tui::vim::Mode;
use crossterm::event::{KeyCode, KeyEvent};

fn press(c: &mut Composer, keys: &str) {
    for ch in keys.chars() {
        let code = match ch {
            '⎋' => KeyCode::Esc,
            _ => KeyCode::Char(ch),
        };
        c.key(KeyEvent::from(code));
    }
}

#[test]
fn vim_keypress_table() {
    let mut c = Composer::new();
    c.set_vim(true);
    c.set_text("one two three\nfour five");
    // Cursor starts at the end of the inserted text.
    let table: &[(&str, &str, (usize, usize), Mode)] = &[
        ("⎋", "one two three\nfour five", (1, 9), Mode::Normal),
        ("k0", "one two three\nfour five", (0, 0), Mode::Normal),
        ("w", "one two three\nfour five", (0, 4), Mode::Normal),
        ("3l", "one two three\nfour five", (0, 7), Mode::Normal),
        ("$", "one two three\nfour five", (0, 13), Mode::Normal),
        ("b", "one two three\nfour five", (0, 8), Mode::Normal),
        ("j", "one two three\nfour five", (1, 8), Mode::Normal),
        ("0x", "one two three\nour five", (1, 0), Mode::Normal),
        ("2x", "one two three\nr five", (1, 0), Mode::Normal),
        ("kdd", "r five", (0, 0), Mode::Normal),
        ("yyp", "r five\nr five", (1, 0), Mode::Normal),
        ("i", "r five\nr five", (1, 0), Mode::Insert),
        ("X⎋", "r five\nXr five", (1, 1), Mode::Normal),
        ("a", "r five\nXr five", (1, 2), Mode::Insert),
        ("⎋o", "r five\nXr five\n", (2, 0), Mode::Insert),
        ("new⎋", "r five\nXr five\nnew", (2, 3), Mode::Normal),
        // `dd` on the last line takes the newline before it too.
        ("dd", "r five\nXr five", (1, 0), Mode::Normal),
    ];
    for (keys, text, cursor, mode) in table {
        press(&mut c, keys);
        assert_eq!(c.text(), *text, "after {keys:?}");
        assert_eq!(c.cursor(), *cursor, "after {keys:?}");
        assert_eq!(c.vim_mode(), Some(*mode), "after {keys:?}");
    }
}

#[test]
fn vim_off_leaves_keys_alone_and_slash_vim_toggles_it() {
    let mut c = Composer::new();
    press(&mut c, "hjkl");
    assert_eq!(c.text(), "hjkl");
    assert_eq!(c.vim_mode(), None);

    let mut state = State::new(PermissionMode::Default, SandboxMode::ReadOnly);
    for k in [KeyCode::Char('/'), KeyCode::Esc] {
        update(&mut state, Msg::Key(KeyEvent::from(k)));
    }
    for ch in "vim".chars() {
        update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Char(ch))));
    }
    assert!(update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Enter))).is_empty());
    assert_eq!(state.composer.vim_mode(), Some(Mode::Insert));
    let line = cox_tui::status::line(&state).to_string();
    assert!(line.ends_with("INSERT"), "{line}");
    update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Esc)));
    assert!(
        cox_tui::status::line(&state)
            .to_string()
            .ends_with("NORMAL")
    );
}
