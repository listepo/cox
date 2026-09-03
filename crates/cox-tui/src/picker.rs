//! The picker (T5.2): one nucleo-ranked list behind `@` files, `/` commands
//! and `Ctrl+R` history. Candidates come from the caller (the workspace walk
//! is I/O and belongs to the runtime); ranking is nucleo with the same
//! path-aware config as the `glob` tool, so the user and the model find "the
//! auth handler, wherever it lives" the same way. The scorer is repeated here
//! rather than imported because plan.md §1.1 forbids `cox-tui` → `cox-tools`.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nucleo::pattern::{CaseMatching, Normalization, Pattern};
use nucleo::{Config, Matcher, Utf32String};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;

/// Rows the list takes at most; the query narrows it, not scrolling.
const MAX_SHOWN: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Files,
    Commands,
    History,
}

impl Kind {
    fn prefix(self) -> &'static str {
        match self {
            Kind::Files => "@",
            Kind::Commands => "/",
            Kind::History => "history: ",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker {
    pub kind: Kind,
    pub query: String,
    all: Vec<String>,
    pub matches: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    Nothing,
    Chosen(String),
    Closed,
}

impl Picker {
    pub fn open(kind: Kind, candidates: Vec<String>) -> Self {
        let mut picker = Self {
            kind,
            query: String::new(),
            all: candidates,
            matches: Vec::new(),
            selected: 0,
        };
        picker.refilter();
        picker
    }

    fn refilter(&mut self) {
        let mut matches = self.all.clone();
        if !self.query.is_empty() {
            rank_by_query(&mut matches, &self.query);
        }
        matches.truncate(MAX_SHOWN);
        self.matches = matches;
        self.selected = 0;
    }

    pub fn key(&mut self, key: KeyEvent) -> Pick {
        match key.code {
            KeyCode::Esc => Pick::Closed,
            // Backspace past the start of the query is "never mind".
            KeyCode::Backspace => {
                if self.query.pop().is_none() {
                    return Pick::Closed;
                }
                self.refilter();
                Pick::Nothing
            }
            KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                Pick::Nothing
            }
            KeyCode::Down => {
                if self.selected + 1 < self.matches.len() {
                    self.selected += 1;
                }
                Pick::Nothing
            }
            KeyCode::Tab | KeyCode::Enter => match self.matches.get(self.selected) {
                Some(m) => Pick::Chosen(m.clone()),
                None => Pick::Closed,
            },
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.query.push(c);
                self.refilter();
                Pick::Nothing
            }
            _ => Pick::Nothing,
        }
    }

    pub fn height(&self) -> u16 {
        u16::try_from(1 + self.matches.len()).unwrap_or(u16::MAX)
    }

    pub fn lines(&self) -> Vec<Line<'static>> {
        let mut lines = vec![Line::styled(
            format!(" {}{}", self.kind.prefix(), self.query),
            Style::default().add_modifier(Modifier::BOLD),
        )];
        lines.extend(self.matches.iter().enumerate().map(|(i, m)| {
            if i == self.selected {
                Line::styled(format!(" ▸ {m}"), Style::default().fg(Color::Cyan))
            } else {
                Line::raw(format!("   {m}"))
            }
        }));
        lines
    }
}

/// Reorders `found` by nucleo's fuzzy score, best first, dropping what the
/// query does not match; ties broken by path so the order is stable.
fn rank_by_query(found: &mut Vec<String>, query: &str) {
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut scored: Vec<(u32, String)> = std::mem::take(found)
        .into_iter()
        .filter_map(|s| {
            let haystack = Utf32String::from(s.as_str());
            pattern
                .score(haystack.slice(..), &mut matcher)
                .map(|score| (score, s))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    *found = scored.into_iter().map(|(_, s)| s).collect();
}
