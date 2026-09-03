//! The composer (T5.2): a `tui-textarea` wrapped so the keys that mean
//! something to cox — submit, newline, `@`, `/`, history — are decided here
//! and everything else is plain editing. Pure like the rest of `state`; the
//! pickers those keys open live in `picker` because `Ctrl+R` history search
//! is the same list over different candidates.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Style;
use tui_textarea::TextArea;

use crate::vim::{Mode, Vim};

/// What a key did beyond editing text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Nothing,
    Submit(String),
    OpenFiles,
    OpenCommands,
    OpenHistory,
}

#[derive(Debug, Clone)]
pub struct Composer {
    area: TextArea<'static>,
    /// Submitted texts, oldest first.
    history: Vec<String>,
    /// The history entry on screen while browsing with `Up`/`Down`.
    browsing: Option<usize>,
    /// `tui.vim`: normal/insert modes over the same textarea.
    vim: Option<Vim>,
}

fn fresh() -> TextArea<'static> {
    let mut area = TextArea::default();
    // The default underlines the cursor line, which reads as a mistake in a
    // one-line prompt.
    area.set_cursor_line_style(Style::default());
    area.set_placeholder_text("message · @ file · / command");
    area
}

impl Default for Composer {
    fn default() -> Self {
        Self::new()
    }
}

impl Composer {
    pub fn new() -> Self {
        Self {
            area: fresh(),
            history: Vec::new(),
            browsing: None,
            vim: None,
        }
    }

    pub fn set_vim(&mut self, on: bool) {
        self.vim = on.then(Vim::default);
    }

    /// The vim mode, or `None` when vim keys are off.
    pub fn vim_mode(&self) -> Option<Mode> {
        self.vim.as_ref().map(|v| v.mode)
    }

    pub fn text(&self) -> String {
        self.area.lines().join("\n")
    }

    pub fn is_empty(&self) -> bool {
        self.area.lines().iter().all(|l| l.trim().is_empty())
    }

    pub fn line_count(&self) -> usize {
        self.area.lines().len()
    }

    /// `(row, col)` of the cursor inside the text.
    pub fn cursor(&self) -> (usize, usize) {
        self.area.cursor()
    }

    pub fn widget(&self) -> &TextArea<'static> {
        &self.area
    }

    pub fn history(&self) -> &[String] {
        &self.history
    }

    pub fn set_text(&mut self, text: &str) {
        self.area = fresh();
        self.area.insert_str(text);
    }

    /// Inserts at the cursor: a paste, or what a picker chose.
    pub fn insert(&mut self, text: &str) {
        self.area.insert_str(text);
    }

    pub fn key(&mut self, key: KeyEvent) -> Edit {
        if let Some(vim) = &mut self.vim
            && vim.key(key, &mut self.area)
        {
            return Edit::Nothing;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let (row, col) = self.area.cursor();
        match key.code {
            KeyCode::Enter
                if key
                    .modifiers
                    .intersects(KeyModifiers::SHIFT | KeyModifiers::ALT) =>
            {
                self.area.insert_newline();
                Edit::Nothing
            }
            KeyCode::Enter => {
                if self.is_empty() {
                    return Edit::Nothing;
                }
                let text = self.text();
                self.history.push(text.clone());
                self.browsing = None;
                self.area = fresh();
                Edit::Submit(text)
            }
            KeyCode::Char('r') if ctrl => Edit::OpenHistory,
            KeyCode::Char('@') if !ctrl => {
                self.area.insert_char('@');
                Edit::OpenFiles
            }
            KeyCode::Char('/') if !ctrl && (row, col) == (0, 0) && self.is_empty() => {
                self.area.insert_char('/');
                Edit::OpenCommands
            }
            KeyCode::Up if row == 0 && !self.history.is_empty() => {
                let last = self.history.len().saturating_sub(1);
                let ix = self.browsing.map_or(last, |i| i.saturating_sub(1));
                self.browse(Some(ix));
                Edit::Nothing
            }
            KeyCode::Down if self.browsing.is_some() && row + 1 == self.line_count() => {
                let next = self
                    .browsing
                    .and_then(|i| (i + 1 < self.history.len()).then_some(i + 1));
                self.browse(next);
                Edit::Nothing
            }
            _ => {
                self.area.input(key);
                Edit::Nothing
            }
        }
    }

    fn browse(&mut self, ix: Option<usize>) {
        self.browsing = ix;
        let text = ix
            .and_then(|i| self.history.get(i))
            .cloned()
            .unwrap_or_default();
        self.set_text(&text);
    }
}
