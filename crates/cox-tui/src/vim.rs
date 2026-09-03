//! Vim-lite (T5.7): normal/insert modes over the composer's textarea, on
//! when `tui.vim` is set. Only the keys the plan names — `Esc`/`i`/`a`/`o`,
//! `hjkl`, `w`/`b`/`0`/`$`, `dd`/`yy`/`p`/`x`, counts — so the table stays
//! on one screen; anything else in normal mode is ignored, except `Enter`
//! and control keys, which fall through so submit and interrupt still work.
//! Separate from `composer` so the non-vim path carries no modal state.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_textarea::{CursorMove, TextArea};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    Normal,
    #[default]
    Insert,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Vim {
    pub mode: Mode,
    /// Digits typed before a command.
    count: String,
    /// The first half of `dd`/`yy`.
    pending: Option<char>,
    /// The yank came from `dd`/`yy`, so `p` puts it on a line of its own.
    linewise: bool,
}

impl Vim {
    /// Runs `key` in vim terms. `true` means it was consumed here; `false`
    /// means insert mode (or a pass-through key) and the composer edits.
    pub fn key(&mut self, key: KeyEvent, area: &mut TextArea<'static>) -> bool {
        match self.mode {
            Mode::Insert => {
                if key.code == KeyCode::Esc {
                    self.mode = Mode::Normal;
                    return true;
                }
                false
            }
            Mode::Normal => self.normal(key, area),
        }
    }

    fn normal(&mut self, key: KeyEvent, area: &mut TextArea<'static>) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) || key.code == KeyCode::Enter {
            return false;
        }
        let KeyCode::Char(c) = key.code else {
            self.count.clear();
            self.pending = None;
            return true;
        };
        if c.is_ascii_digit() && !(c == '0' && self.count.is_empty()) {
            self.count.push(c);
            return true;
        }
        let n = self.count.parse().unwrap_or(1);
        self.count.clear();
        if let Some(op) = self.pending.take() {
            if op == c {
                for _ in 0..n {
                    let (row, _) = area.cursor();
                    let line = area.lines().get(row).cloned().unwrap_or_default();
                    if op == 'd' {
                        delete_line(area, row);
                    } else {
                        area.move_cursor(CursorMove::Head);
                    }
                    area.set_yank_text(line);
                    self.linewise = true;
                }
            }
            return true;
        }
        let repeat = |area: &mut TextArea<'static>, mv: CursorMove| {
            for _ in 0..n {
                area.move_cursor(mv);
            }
        };
        match c {
            'i' => self.mode = Mode::Insert,
            'a' => {
                area.move_cursor(CursorMove::Forward);
                self.mode = Mode::Insert;
            }
            'o' => {
                area.move_cursor(CursorMove::End);
                area.insert_newline();
                self.mode = Mode::Insert;
            }
            'h' => repeat(area, CursorMove::Back),
            'j' => repeat(area, CursorMove::Down),
            'k' => repeat(area, CursorMove::Up),
            'l' => repeat(area, CursorMove::Forward),
            'w' => repeat(area, CursorMove::WordForward),
            'b' => repeat(area, CursorMove::WordBack),
            '0' => area.move_cursor(CursorMove::Head),
            '$' => area.move_cursor(CursorMove::End),
            'x' => {
                for _ in 0..n {
                    area.delete_next_char();
                }
            }
            'p' if self.linewise => {
                for _ in 0..n {
                    area.move_cursor(CursorMove::End);
                    area.insert_newline();
                    let text = area.yank_text();
                    area.insert_str(text);
                    area.move_cursor(CursorMove::Head);
                }
            }
            'p' => {
                for _ in 0..n {
                    area.paste();
                }
            }
            'd' | 'y' => self.pending = Some(c),
            _ => {}
        }
        true
    }
}

/// Removes line `row` and its newline; the last line takes the newline
/// before it, and the cursor lands at the head of what is left.
fn delete_line(area: &mut TextArea<'static>, row: usize) {
    let last = row + 1 == area.lines().len();
    area.move_cursor(CursorMove::Head);
    area.start_selection();
    if last {
        area.move_cursor(CursorMove::End);
    } else {
        area.move_cursor(CursorMove::Down);
    }
    area.cut();
    if last && row > 0 {
        area.delete_char();
    }
    area.move_cursor(CursorMove::Head);
}
