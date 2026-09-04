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

use crate::glyph::Glyphs;
use crate::state::State;

/// Rows the list takes at most; the query narrows it, not scrolling.
const MAX_SHOWN: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Files,
    Commands,
    History,
    /// `/resume`: past sessions as `title · cwd · age · $cost` rows.
    Sessions,
    /// `Tab` on a `git` line (T15.4): a subcommand, a branch or a path.
    Shell,
}

/// Git's porcelain, offered where the subcommand goes.
const GIT_COMMANDS: [&str; 28] = [
    "add",
    "bisect",
    "blame",
    "branch",
    "checkout",
    "cherry-pick",
    "clone",
    "commit",
    "diff",
    "fetch",
    "grep",
    "init",
    "log",
    "merge",
    "mv",
    "pull",
    "push",
    "rebase",
    "reset",
    "restore",
    "revert",
    "rm",
    "show",
    "stash",
    "status",
    "switch",
    "tag",
    "worktree",
];
/// Subcommands whose next word names a branch; every other takes a path.
const BRANCH_COMMANDS: [&str; 5] = ["checkout", "switch", "merge", "rebase", "branch"];

/// What `Tab` completes on a shell line: nothing unless it is a `git` line,
/// then the subcommand, a branch or a path by what stands before the word
/// being typed. Pure: the branches and files were fed into `State`.
pub fn candidates(line: &str, state: &State) -> Vec<String> {
    let Some(rest) = line.strip_prefix("git ") else {
        return Vec::new();
    };
    let mut words = rest.split_whitespace();
    let sub = words.next();
    // The subcommand counts only once the user has moved past it.
    let past_sub = words.next().is_some() || rest.ends_with(char::is_whitespace);
    match sub {
        Some(sub) if past_sub => {
            if BRANCH_COMMANDS.contains(&sub) {
                state.git_branches.clone()
            } else {
                state.files.clone()
            }
        }
        _ => GIT_COMMANDS.iter().map(|s| (*s).to_string()).collect(),
    }
}

/// The word being typed: empty after a space.
pub fn last_word(line: &str) -> &str {
    line.rsplit(char::is_whitespace).next().unwrap_or("")
}

impl Kind {
    fn prefix(self) -> &'static str {
        match self {
            Kind::Files => "@",
            Kind::Commands => "/",
            Kind::History => "history: ",
            Kind::Sessions => "resume: ",
            Kind::Shell => "complete: ",
        }
    }
}

/// One `/resume` row: title (or `untitled`), cwd, coarse age and cost.
pub fn session_entry(title: Option<&str>, cwd: &str, age: &str, cost_usd: f64) -> String {
    format!(
        "{} · {} · {} · ${:.2}",
        title.unwrap_or("untitled"),
        cwd,
        age,
        cost_usd
    )
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

    /// Starts from a query the user already typed (the last word of a
    /// shell line).
    pub fn with_query(mut self, query: &str) -> Self {
        self.query = query.to_string();
        self.refilter();
        self
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

    pub fn lines(&self, g: &Glyphs) -> Vec<Line<'static>> {
        let mut lines = vec![Line::styled(
            format!(" {}{}", self.kind.prefix(), self.query),
            Style::default().add_modifier(Modifier::BOLD),
        )];
        lines.extend(self.matches.iter().enumerate().map(|(i, m)| {
            if i == self.selected {
                Line::styled(
                    format!(" {} {}", g.cursor, crate::text::sanitize(m)),
                    Style::default().fg(Color::Cyan),
                )
            } else {
                Line::raw(format!("   {}", crate::text::sanitize(m)))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_candidates_follow_the_word_before_the_cursor() {
        let mut state = State::new(
            cox_protocol::types::PermissionMode::Default,
            cox_protocol::types::SandboxMode::WorkspaceWrite,
        );
        state.files = vec!["src/lib.rs".into()];
        state.git_branches = vec!["main".into()];
        assert!(candidates("ls ", &state).is_empty());
        assert!(candidates("git ch", &state).contains(&"checkout".to_string()));
        assert_eq!(candidates("git ", &state).len(), GIT_COMMANDS.len());
        assert_eq!(candidates("git checkout ma", &state), ["main"]);
        assert_eq!(candidates("git switch ", &state), ["main"]);
        assert_eq!(candidates("git add sr", &state), ["src/lib.rs"]);
        assert_eq!(last_word("git add sr"), "sr");
        assert_eq!(last_word("git add "), "");
    }

    #[test]
    fn picker_session_entry_lists_title_cwd_age_cost() {
        assert_eq!(
            session_entry(Some("auth work"), "/tmp/work", "3h", 0.83),
            "auth work · /tmp/work · 3h · $0.83"
        );
        assert_eq!(
            session_entry(None, "/tmp/work", "now", 0.0),
            "untitled · /tmp/work · now · $0.00"
        );
        assert_eq!(Kind::Sessions.prefix(), "resume: ");
        // The entry shape ranks under a fuzzy query like any other row.
        let picker = Picker::open(
            Kind::Sessions,
            vec![
                session_entry(Some("auth work"), "/tmp/work", "3h", 0.83),
                session_entry(Some("docs"), "/tmp/other", "9d", 0.01),
            ],
        );
        assert_eq!(picker.matches.len(), 2);
    }
}
