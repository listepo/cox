//! Approval modal (T5.4): what `ApprovalRequired` shows and the keys that
//! decide it — `y` allow, `s` allow for the session, `n` deny, `e` edit a
//! bash command inline and resubmit it as `Decision::Edit`. Separate from
//! `state` so the key table and the drawing sit together and one snapshot
//! covers both.

use cox_protocol::ids::CallId;
use cox_protocol::types::{Decision, ToolCall, Why};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;

use crate::glyph::Glyphs;
use crate::text::sanitize;
use crate::theme::Theme;

/// The bash tool's input field the `e` key rewrites.
const COMMAND_FIELD: &str = "command";

#[derive(Debug, Clone, PartialEq)]
pub struct Approval {
    pub call: ToolCall,
    pub why: Why,
    /// `e`: the command as edited so far; the cursor sits at its end.
    pub editing: Option<String>,
}

impl Approval {
    pub fn new(call: ToolCall, why: Why) -> Self {
        Self {
            call,
            why,
            editing: None,
        }
    }

    fn editable(&self) -> bool {
        self.call.name == "bash"
    }

    /// `Some` once a key decided the call; `None` keeps the modal open.
    pub fn key(&mut self, key: KeyEvent) -> Option<Decision> {
        if let Some(text) = &mut self.editing {
            match key.code {
                KeyCode::Enter => {
                    let edited = std::mem::take(text);
                    self.editing = None;
                    if self.command() == edited {
                        return Some(Decision::Allow);
                    }
                    let mut input = self.call.input.clone();
                    if let Some(obj) = input.as_object_mut() {
                        obj.insert(COMMAND_FIELD.into(), edited.into());
                    }
                    return Some(Decision::Edit { input });
                }
                KeyCode::Esc => self.editing = None,
                KeyCode::Backspace => {
                    text.pop();
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => text.push(c),
                _ => {}
            }
            return None;
        }
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => Some(Decision::Allow),
            KeyCode::Char('s') => Some(Decision::AllowForSession),
            KeyCode::Char('n') | KeyCode::Esc => Some(Decision::Deny {
                reason: "denied by user".into(),
            }),
            KeyCode::Char('e') if self.editable() => {
                self.editing = Some(self.command());
                None
            }
            _ => None,
        }
    }

    /// The command as the model wrote it; the subject is the fallback for a
    /// call whose input does not carry one.
    fn command(&self) -> String {
        self.call.input[COMMAND_FIELD]
            .as_str()
            .map_or_else(|| self.call.subject.clone(), str::to_string)
    }

    pub fn height(&self) -> u16 {
        3
    }

    pub fn lines(&self, g: &Glyphs, theme: &Theme) -> Vec<Line<'static>> {
        let why = match &self.why {
            Why::RuleAsk { rule } => format!("rule {rule} asks"),
            Why::Risk { risk } => format!("{risk:?} risk needs approval").to_lowercase(),
            Why::SandboxDenied { detail } => format!("sandbox denied: {}", sanitize(detail)),
            Why::Policy { policy } => format!("approval policy {policy:?}").to_lowercase(),
        };
        let keys = match &self.editing {
            Some(text) => format!(
                " edit> {text}{}   Enter runs {} Esc cancels",
                g.caret, g.sep
            ),
            None if self.editable() => " [y]es  [s]ession  [n]o  [e]dit".to_string(),
            None => " [y]es  [s]ession  [n]o".to_string(),
        };
        vec![
            Line::styled(
                format!(
                    " approve {} {}?",
                    sanitize(&self.call.name),
                    sanitize(&self.call.subject)
                ),
                Style::default().fg(theme.warn).add_modifier(Modifier::BOLD),
            ),
            Line::styled(
                format!(" {why}"),
                Style::default().add_modifier(Modifier::DIM),
            ),
            Line::raw(keys),
        ]
    }
}

/// `ask_user`'s answer, once a key decides it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestionAnswer {
    /// A `1`-`9` option pick, or whatever `Enter` sent from the free-text row.
    Text(String),
    /// `Esc`: the reply channel is dropped rather than sent, so the tool
    /// call fails ("dismissed without an answer") instead of succeeding
    /// with an empty answer.
    Dismissed,
}

/// `ask_user` modal (T22.1): the model's question, its suggested options
/// picked by digit, and a free-text row `Enter` sends. Shares `Approval`'s
/// shape — a `key`/`height`/`lines` triple — so `state.rs` and `view.rs`
/// drive both the same way.
#[derive(Debug, Clone, PartialEq)]
pub struct Question {
    pub call: CallId,
    question: String,
    options: Vec<String>,
    /// What the user has typed so far; sent verbatim on `Enter`.
    input: String,
}

impl Question {
    pub fn new(call: CallId, question: String, options: Vec<String>) -> Self {
        Self {
            call,
            question,
            options,
            input: String::new(),
        }
    }

    /// `Some` once a key decided the answer; `None` keeps the modal open.
    pub fn key(&mut self, key: KeyEvent) -> Option<QuestionAnswer> {
        match key.code {
            KeyCode::Esc => Some(QuestionAnswer::Dismissed),
            KeyCode::Enter => Some(QuestionAnswer::Text(std::mem::take(&mut self.input))),
            KeyCode::Backspace => {
                self.input.pop();
                None
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                // A bare digit picks an option, but only as the first
                // keystroke — once the free-text row holds anything, a
                // digit joins it like any other character.
                if self.input.is_empty()
                    && let Some(n) = c.to_digit(10).filter(|n| (1..=9).contains(n))
                    && let Some(opt) = self.options.get(n as usize - 1)
                {
                    return Some(QuestionAnswer::Text(opt.clone()));
                }
                self.input.push(c);
                None
            }
            _ => None,
        }
    }

    pub fn height(&self) -> u16 {
        3
    }

    pub fn lines(&self, g: &Glyphs, theme: &Theme) -> Vec<Line<'static>> {
        let options = if self.options.is_empty() {
            " type an answer".to_string()
        } else {
            self.options
                .iter()
                .take(9)
                .enumerate()
                .map(|(i, o)| format!(" {}) {}", i + 1, sanitize(o)))
                .collect::<Vec<_>>()
                .join(" ")
        };
        vec![
            Line::styled(
                format!(" ask_user {}", sanitize(&self.question)),
                Style::default().fg(theme.warn).add_modifier(Modifier::BOLD),
            ),
            Line::styled(options, Style::default().add_modifier(Modifier::DIM)),
            Line::raw(format!(
                " > {}{}   Enter sends {} Esc dismisses",
                self.input, g.caret, g.sep
            )),
        ]
    }
}
