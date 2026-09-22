//! Approval modal (T5.4): what `ApprovalRequired` shows and the keys that
//! decide it — `y` allow, `s` allow for the session, `n` deny, `e` edit a
//! bash command inline and resubmit it as `Decision::Edit`. Separate from
//! `state` so the key table and the drawing sit together and one snapshot
//! covers both. The `/context` modal (T25.7) lives here for the same reason.

use cox_protocol::ids::CallId;
use cox_protocol::types::{Decision, ToolCall, Why};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

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

/// `/context` (T25.7): where the next request's tokens go — one bar per
/// §1.9 segment scaled to `max_context`, numbers right-aligned, and the
/// compaction threshold as a marker. Same `height`/`lines` shape as the
/// sibling modals; bars are ASCII so both glyph sets hold (T14.1).
#[derive(Debug, Clone, PartialEq)]
pub struct ContextBars {
    /// `(label, estimated tokens)` per §1.9 segment, display order — the
    /// numbers arrive as rows because `cox-core`'s `Breakdown` is not
    /// exported across the crate boundary and this modal only displays.
    pub segments: Vec<(&'static str, u32)>,
    pub total: u32,
    pub cached: u32,
    pub max_context: u32,
    pub compact_at: f64,
}

/// Bar geometry, fixed so the rows and the threshold marker align; the
/// label column is 17 so the widest label ("history verbatim") keeps a gap.
const LABEL: usize = 17;
const BAR: usize = 24;

impl ContextBars {
    pub fn height(&self) -> u16 {
        // header + one row per segment + total + cached + threshold caption.
        u16::try_from(self.segments.len() + 4).unwrap_or(u16::MAX)
    }

    pub fn lines(&self, g: &Glyphs, theme: &Theme) -> Vec<Line<'static>> {
        let marker = ((self.compact_at.clamp(0.0, 1.0) * BAR as f64).round() as usize).min(BAR - 1);
        let bar = |tokens: u32| {
            let filled = u64::from(tokens) * BAR as u64 / u64::from(self.max_context.max(1));
            (0..BAR)
                .map(|i| match (i == marker, (i as u64) < filled) {
                    (true, _) => '|',
                    (false, true) => '#',
                    (false, false) => ' ',
                })
                .collect::<String>()
        };
        let row = |label: &str, tokens: u32, style: Style| {
            Line::from(vec![
                Span::styled(
                    format!(" {label:<w$}", w = LABEL),
                    Style::default().fg(theme.dim),
                ),
                Span::styled(bar(tokens), Style::default().fg(theme.accent)),
                Span::styled(format!("  {tokens:>8}"), style),
            ])
        };
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let mut lines = vec![Line::styled(
            format!(
                " context {} {} / {} tokens {} cached {}",
                g.sep, self.total, self.max_context, g.sep, self.cached
            ),
            bold,
        )];
        lines.extend(
            self.segments
                .iter()
                .map(|(label, tokens)| row(label, *tokens, Style::default())),
        );
        lines.push(row("total", self.total, bold));
        lines.push(row(
            "cached",
            self.cached,
            Style::default().add_modifier(Modifier::DIM),
        ));
        let threshold = (self.compact_at.clamp(0.0, 1.0) * f64::from(self.max_context)) as u32;
        lines.push(Line::styled(
            format!(
                " {}^ compact_at = {} = {} tokens",
                " ".repeat(LABEL + marker),
                self.compact_at,
                threshold
            ),
            Style::default().add_modifier(Modifier::DIM),
        ));
        lines
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::widgets::{Paragraph, Widget};

    use super::*;

    /// T25.7 `/context`: bars scaled to `max_context`, right-aligned
    /// numbers, the compaction threshold marked.
    #[test]
    fn context_modal_snapshot() {
        let bars = ContextBars {
            segments: vec![
                ("tools", 21_500),
                ("system", 3_200),
                ("instructions", 120),
                ("skills", 0),
                ("memory", 0),
                ("volatile", 240),
                ("history verbatim", 41_000),
                ("history pointers", 1_250),
                ("summary", 900),
            ],
            total: 68_210,
            cached: 51_000,
            max_context: 200_000,
            compact_at: 0.75,
        };
        let mut term = Terminal::new(TestBackend::new(72, bars.height())).expect("test terminal");
        term.draw(|f| {
            Paragraph::new(bars.lines(&Glyphs::default(), &Theme::dark()))
                .render(f.area(), f.buffer_mut());
        })
        .expect("draw");
        insta::assert_snapshot!(crate::view::buffer_to_string(term.backend().buffer()));
    }
}
