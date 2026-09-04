//! Status line and todo panel (T5.5): the one row under the composer in the
//! §1.13 form `sonnet-5 · ctx 41% · $0.83 · workspace-write · 2 tasks ·
//! [plan]`, and the panel the `todo` tool's list appears in. Separate from
//! `view` so both are plain text a test can compare without a buffer.

use cox_protocol::types::SandboxMode;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;

use crate::state::State;
use crate::vim::Mode;

pub fn line(state: &State) -> Line<'static> {
    let s = &state.status;
    let model = match s.model.strip_prefix("claude-").unwrap_or(&s.model) {
        "" => "-",
        m => m,
    };
    let sandbox = match s.sandbox {
        SandboxMode::ReadOnly => "read-only",
        SandboxMode::WorkspaceWrite => "workspace-write",
        SandboxMode::DangerFullAccess => "danger-full-access",
    };
    let pct = u64::from(s.context_tokens) * 100 / u64::from(s.context_window.max(1));
    let cache = (s.cache_ratio * 100.0).round() as u64;
    let sep = state.glyphs.sep;
    let tail = match (s.busy, state.ctrl_c_armed) {
        (true, _) => format!(" {sep} working"),
        (false, true) => format!(" {sep} Ctrl+C again to quit"),
        (false, false) => String::new(),
    };
    let vim = match state.composer.vim_mode() {
        Some(Mode::Normal) => format!(" {sep} NORMAL"),
        Some(Mode::Insert) => format!(" {sep} INSERT"),
        None => String::new(),
    };
    Line::styled(
        format!(
            " {model} {sep} ctx {pct}% {sep} cache {cache}% {sep} ${:.2} {sep} {sandbox} {sep} {} tasks {sep} [{}]{tail}{vim}",
            s.cost_usd,
            state.tasks.len(),
            format!("{:?}", state.mode).to_lowercase(),
        ),
        Style::default().add_modifier(Modifier::DIM),
    )
}

/// The `todo` tool's rendered list (`[x] id: text` per line) as
/// `(mark, text)` pairs; `structured` does not cross the event boundary, so
/// the panel reads what the model saw.
pub fn parse_todo(visible: &str) -> Vec<(String, String)> {
    visible
        .lines()
        .filter_map(|l| {
            let (mark, rest) = l.strip_prefix('[')?.split_once("] ")?;
            let (_, text) = rest.split_once(": ")?;
            Some((mark.to_string(), text.to_string()))
        })
        .collect()
}

/// The panel: a header and one row per item; done dim, in progress bold.
pub fn todo_lines(state: &State) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(
        " todo",
        Style::default().add_modifier(Modifier::BOLD),
    )];
    lines.extend(state.todo.iter().map(|(mark, text)| {
        let style = match mark.as_str() {
            "x" => Style::default().add_modifier(Modifier::DIM),
            "~" => Style::default().add_modifier(Modifier::BOLD),
            _ => Style::default(),
        };
        Line::styled(format!(" [{mark}] {text}"), style)
    }));
    lines
}
