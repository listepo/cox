//! Status line and todo panel (T5.5, segments T28.1): the one row under the
//! composer in the §1.13 form `sonnet-5 · ctx ▰▰▰▱▱ 41% · $0.83/5 ·
//! workspace-write · 2 tasks · [plan]`, and the panel the `todo` tool's list
//! appears in. Separate from `view` so both are plain text a test can compare
//! without a buffer. Narrow terminals drop segments from the right in the
//! order `docs/getting-started.md` documents (git counts → cache → tasks →
//! effort → model → cost → ctx); `plain_text` is the same segments joined
//! once per turn for `--plain` (T29.1).

use cox_protocol::types::{PresenceStatus, SandboxMode};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::state::{Modal, State};
use crate::vim::Mode;

/// Cells of the `ctx` mini bar; the cached share fills in `theme.accent`.
const CTX_CELLS: usize = 5;

/// One status segment: its text and whether it survives narrowing. The order
/// below is display order; `fit` drops from the right in the documented
/// order (git counts → cache → tasks → effort → sandbox → model → cost →
/// ctx), so the row reads `model · ctx · cost · sandbox · effort · tasks ·
/// cache · mode` at full width.
fn segments(state: &State) -> Vec<(bool, String)> {
    let s = &state.status;
    let sep = state.glyphs.sep;
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
    let filled = ((s.cache_ratio.clamp(0.0, 1.0) * CTX_CELLS as f64).round() as usize)
        .min(CTX_CELLS);
    let bar: String = "▰".repeat(filled) + &"▱".repeat(CTX_CELLS - filled);
    let cache_pct = (s.cache_ratio * 100.0).round() as u64;
    // Inside a repository the line starts with `⎇ main +12 −3`; outside one
    // it is exactly what it was (T15.2). The branch is git's text: sanitised.
    // `⧉ t42` right after the branch when the session runs in a worktree
    // (T27.3); both names came from outside, so both are sanitised like any
    // other untrusted string.
    let git = state.git.as_ref().map_or(String::new(), |g| {
        format!(
            "{} {} +{} {}{} {sep} ",
            state.glyphs.branch,
            crate::text::sanitize(&g.branch),
            g.added,
            state.glyphs.minus,
            g.removed
        )
    });
    let worktree = state.worktree.as_ref().map_or(String::new(), |name| {
        format!(
            "{} {} {sep} ",
            state.glyphs.worktree,
            crate::text::sanitize(name)
        )
    });
    let head = format!("{git}{worktree}");
    let ctx = format!("ctx {bar} {pct}%");
    let cost = format!("${:.2}/{:.0}", s.cost_usd, s.budget_cap_usd);
    let effort = s.effort.map(|e| format!("effort:{}", e.name()));
    // `ask_user`'s modal takes over the mode slot (T22.1): there is
    // nothing to permission-check while it is open.
    let mode = match &state.modal {
        Some(Modal::Question(_)) => "question".to_string(),
        _ => format!("{:?}", state.mode).to_lowercase(),
    };
    let tasks = format!("{} tasks", state.tasks.len());
    let cache = format!("cache {cache_pct}%");
    let tail = match (s.busy, state.ctrl_c_armed) {
        (true, _) => format!(" {sep} working"),
        (false, true) => format!(" {sep} Ctrl+C again to quit"),
        (false, false) => String::new(),
    };
    let vim = match state.composer.vim_mode() {
        Some(Mode::Normal) => format!(" {sep} -- NORMAL --"),
        Some(Mode::Insert) => format!(" {sep} -- INSERT --"),
        Some(Mode::Visual) => format!(" {sep} -- VISUAL --"),
        Some(Mode::VisualLine) => format!(" {sep} -- VISUAL LINE --"),
        None => String::new(),
    };
    // Only when there are any, so a lone session's line is unchanged; `!`
    // when one of them is waiting for its user (T16.3).
    let agents = match state.agents.len() {
        0 => String::new(),
        n => {
            let plural = if n == 1 { "" } else { "s" };
            let waiting = state
                .agents
                .iter()
                .any(|a| a.status == PresenceStatus::Waiting);
            let bang = if waiting { "!" } else { "" };
            format!(" {sep} {n} agent{plural}{bang}")
        }
    };
    let suffix = format!("{agents}{tail}{vim}");
    let mut out = vec![(true, model.to_string()), (true, ctx)];
    out.push((true, cost));
    out.push((false, sandbox.to_string()));
    if let Some(effort) = effort {
        out.push((false, effort));
    }
    out.push((false, tasks));
    out.push((false, cache));
    out.push((true, format!("{head}[{mode}]{suffix}")));
    out
}

/// The same segments `--plain` (T29.1) prints once per turn: joined text, no
/// colours, no width fitting — one place builds both surfaces.
pub fn plain_text(state: &State) -> String {
    segments(state)
        .into_iter()
        .map(|(_, text)| text)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(&format!(" {} ", state.glyphs.sep))
}

/// Which segments fit `width`: segments joined in display order, droppable
/// ones (the `false` flag: cache, tasks, effort) removed from the right
/// until the joined length fits. The git head is fused to the mode slot so
/// it never separates from the row it prefixes.
fn fit(state: &State, width: u16) -> Vec<String> {
    let segs = segments(state);
    let sep = format!(" {} ", state.glyphs.sep);
    let mut kept: Vec<(bool, String)> = segs
        .into_iter()
        .filter(|(_, text)| !text.is_empty())
        .collect();
    loop {
        let joined = kept
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>()
            .join(&sep);
        if joined.len() <= width as usize {
            break;
        }
        let Some(pos) = kept.iter().rposition(|(keep, _)| !keep) else {
            break;
        };
        kept.remove(pos);
    }
    kept.into_iter().map(|(_, text)| text).collect()
}

pub fn line(state: &State) -> Line<'static> {
    line_at(state, u16::MAX)
}

/// The status row fitted to `width` columns: `ctx` is the mini bar with the
/// cached share in `theme.accent`; `$` names the spend over the session cap
/// and turns `theme.warn` past `warn_at`.
pub fn line_at(state: &State, width: u16) -> Line<'static> {
    let texts = fit(state, width);
    let sep = Span::styled(
        format!(" {} ", state.glyphs.sep),
        Style::default().add_modifier(Modifier::DIM),
    );
    let mut spans = Vec::new();
    for (i, text) in texts.iter().enumerate() {
        if i > 0 {
            spans.push(sep.clone());
        }
        spans.extend(spans_for(state, text));
    }
    let mut line = Line::from(spans);
    line.style = Style::default().add_modifier(Modifier::DIM);
    line
}

/// One segment's style: the `ctx` bar splits cached (`theme.accent`) from
/// uncached (`theme.text`); the cost turns `theme.warn` past `warn_at`; the
/// rest is the line's own dim. A `Line` cannot nest, so the bar renders as
/// one styled span per share — the caller splices both into the row.
fn spans_for(state: &State, text: &str) -> Vec<Span<'static>> {
    let theme = state.theme;
    if let Some(bar) = text.strip_prefix("ctx ") {
        let cached_cells = bar.chars().take_while(|c| *c == '▰').count();
        let rest: String = bar.chars().skip(cached_cells).collect();
        let mut out = vec![Span::raw("ctx ".to_string())];
        if cached_cells > 0 {
            out.push(Span::styled(
                "▰".repeat(cached_cells),
                Style::default().fg(theme.accent),
            ));
        }
        if !rest.is_empty() {
            out.push(Span::styled(rest, Style::default().fg(theme.text)));
        }
        return out;
    }
    if text.starts_with('$') {
        let s = &state.status;
        if s.cost_usd >= s.budget_cap_usd.max(0.0) * s.budget_warn_at.max(0.0) {
            return vec![Span::styled(text.to_string(), Style::default().fg(theme.warn))];
        }
    }
    vec![Span::raw(text.to_string())]
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
