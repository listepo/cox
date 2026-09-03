//! Approval modal (T5.4): what `ApprovalRequired` shows and the keys that
//! decide it — `y` allow, `s` allow for the session, `n` deny, `e` edit a
//! bash command inline and resubmit it as `Decision::Edit`. Separate from
//! `state` so the key table and the drawing sit together and one snapshot
//! covers both.

use cox_protocol::types::{Decision, ToolCall, Why};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;

use crate::text::sanitize;

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

    pub fn lines(&self) -> Vec<Line<'static>> {
        let why = match &self.why {
            Why::RuleAsk { rule } => format!("rule {rule} asks"),
            Why::Risk { risk } => format!("{risk:?} risk needs approval").to_lowercase(),
            Why::SandboxDenied { detail } => format!("sandbox denied: {}", sanitize(detail)),
            Why::Policy { policy } => format!("approval policy {policy:?}").to_lowercase(),
        };
        let keys = match &self.editing {
            Some(text) => format!(" edit> {text}▏   Enter runs · Esc cancels"),
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
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::styled(
                format!(" {why}"),
                Style::default().add_modifier(Modifier::DIM),
            ),
            Line::raw(keys),
        ]
    }
}
