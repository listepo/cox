//! Status line and todo panel (T5.5, segments T28.1): the one row under the
//! composer in the §1.13 form `sonnet-5 · ctx ▰▰▰▱▱ 41% · $0.83/5 ·
//! workspace-write · 2 tasks · [plan]`, and the panel the `todo` tool's list
//! appears in. Separate from `view` so both are plain text a test can compare
//! without a buffer. Narrow terminals drop segments from the right in the
//! order `docs/getting-started.md` documents (git counts → cache → tasks →
//! effort → model → cost → ctx); `plain_text` is the same segments joined
//! once per turn for `--plain` (T29.1).
//!
//! Plugin `status.left`/`status.right` segments (T33.23, PL§8) sit at either
//! end of the row and drop before any built-in one. Their last good render
//! is cached in `State`; `on_plugin` and `render_requests` are the only
//! places a render is asked for, so drawing the row never reaches a plugin.

use cox_protocol::plugin::{RenderIn, Slot, Widget};
use cox_protocol::types::{PresenceStatus, SandboxMode};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::state::{Cmd, Modal, PluginRequest, PluginUiMsg, State};
use crate::vim::Mode;

/// Columns one plugin segment may fill (PL§8).
pub const SEGMENT_COLS: u16 = 24;
/// `cox_render` misses in a row that stop a slot for the session (PL§8).
pub const MAX_MISSES: u8 = 3;
/// Rows the `panel` slot may fill, above the composer (T33.24, PL§8).
pub const PANEL_ROWS: u16 = 8;

/// One plugin status slot and its last good render.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginSegment {
    pub plugin: String,
    pub slot: Slot,
    /// Kept through misses: a slow render never blanks the segment.
    pub widget: Option<Widget>,
    /// Misses in a row; `MAX_MISSES` stops the slot.
    pub misses: u8,
}

impl PluginSegment {
    fn stopped(&self) -> bool {
        self.misses >= MAX_MISSES
    }
}

/// Folds one `Msg::Plugin` into `state`; a new slot or a redraw asks for
/// renders. A stopped slot ignores even a late answer, so "slow" stays.
pub fn on_plugin(state: &mut State, msg: PluginUiMsg) -> Vec<Cmd> {
    match msg {
        // `commands`/`keys` are folded in `state::update` before this runs
        // (T33.25); only the slots are this module's concern. Every
        // granted slot is registered here, `panel` and `overlay` (T33.24)
        // included — `render_requests`'s `slot_visible` is what actually
        // decides which of them render right now.
        PluginUiMsg::Declare { plugin, slots, .. } => {
            for slot in slots {
                if segment(state, &plugin, slot).is_none() {
                    state.plugin_status.push(PluginSegment {
                        plugin: plugin.clone(),
                        slot,
                        widget: None,
                        misses: 0,
                    });
                }
            }
            render_requests(state, Some(&plugin))
        }
        PluginUiMsg::Redraw { plugin } => render_requests(state, Some(&plugin)),
        PluginUiMsg::Rendered {
            plugin,
            slot,
            widget,
        } => {
            if let Some(seg) = segment(state, &plugin, slot).filter(|s| !s.stopped()) {
                seg.widget = Some(widget);
                seg.misses = 0;
            }
            Vec::new()
        }
        PluginUiMsg::Missed { plugin, slot } => {
            if let Some(seg) = segment(state, &plugin, slot).filter(|s| !s.stopped()) {
                seg.misses += 1;
            }
            Vec::new()
        }
        // `state::update` (T33.25) matches this variant first and never
        // reaches `on_plugin` with it; kept here only so the match stays
        // exhaustive over every `PluginUiMsg`.
        // Likewise `ItemRendered` (T33.26), folded by `item_render`.
        PluginUiMsg::Command { .. } | PluginUiMsg::ItemRendered { .. } => Vec::new(),
    }
}

fn segment<'a>(state: &'a mut State, plugin: &str, slot: Slot) -> Option<&'a mut PluginSegment> {
    state
        .plugin_status
        .iter_mut()
        .find(|s| s.plugin == plugin && s.slot == slot)
}

/// `plugin`'s last good render for `slot` (T33.24, PL§8): `view` reads this
/// for `panel`/`overlay` the same way `plugin_seg` already does for the
/// status row — a stopped or not-yet-rendered slot gives nothing, never a
/// crash.
pub fn widget<'a>(state: &'a State, plugin: &str, slot: Slot) -> Option<&'a Widget> {
    state
        .plugin_status
        .iter()
        .find(|s| s.plugin == plugin && s.slot == slot && !s.stopped())
        .and_then(|s| s.widget.as_ref())
}

/// Whether `slot` is on screen right now: the status segments always are;
/// `panel`/`overlay` (T33.24) only once `TogglePanel`/`OpenOverlay` shows
/// them. This is the redraw model's "a slot became visible" (PL§8) — a
/// slot renders only when this turns true, never at `Declare` time.
fn slot_visible(state: &State, plugin: &str, slot: Slot) -> bool {
    match slot {
        Slot::StatusLeft | Slot::StatusRight => true,
        Slot::Panel => state.plugin_panel_open.as_deref() == Some(plugin),
        Slot::Overlay => matches!(&state.modal, Some(Modal::Plugin { id }) if id == plugin),
    }
}

/// The area a `cox_render` request offers a slot, so a plugin can lay out
/// its own tree for the space it will actually get (T33.24: "the render
/// request carries the area size").
fn area_for(state: &State, slot: Slot) -> (u16, u16) {
    match slot {
        Slot::StatusLeft | Slot::StatusRight => (SEGMENT_COLS, 1),
        Slot::Panel => (state.term.0, PANEL_ROWS),
        Slot::Overlay => state.term,
    }
}

/// A `cox_render` request for every live, visible segment, or only
/// `plugin`'s.
pub fn render_requests(state: &State, plugin: Option<&str>) -> Vec<Cmd> {
    state
        .plugin_status
        .iter()
        .filter(|s| {
            !s.stopped()
                && plugin.is_none_or(|p| p == s.plugin)
                && slot_visible(state, &s.plugin, s.slot)
        })
        .map(|s| {
            let (width, height) = area_for(state, s.slot);
            Cmd::Plugin(PluginRequest::Render {
                plugin: s.plugin.clone(),
                input: RenderIn {
                    slot: s.slot,
                    width,
                    height,
                },
            })
        })
        .collect()
}

/// How a segment gives way on a narrow row: plugins first, then the
/// droppable built-ins, never the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Keep {
    Always,
    Droppable,
    Plugin,
}

/// A segment's text and, for a plugin, its own spans: a plugin's colours
/// come from its render, never from `spans_for`'s guess at the text.
struct Seg {
    keep: Keep,
    text: String,
    spans: Option<Vec<Span<'static>>>,
}

impl Seg {
    fn built_in(keep: bool, text: String) -> Self {
        let keep = if keep { Keep::Always } else { Keep::Droppable };
        Self {
            keep,
            text,
            spans: None,
        }
    }
}

/// A plugin segment as spans: the cached widget drawn by `plugin_ui` (the
/// one sanitize and theme-token boundary) into a one-row buffer, read back
/// cell by cell. `⚠ <id> slow` once the slot has stopped.
fn plugin_seg(state: &State, seg: &PluginSegment) -> Option<Seg> {
    let spans = if seg.stopped() {
        let text = format!("⚠ {} slow", crate::text::sanitize(&seg.plugin));
        vec![Span::styled(text, Style::default().fg(state.theme.warn))]
    } else {
        let area = Rect::new(0, 0, SEGMENT_COLS, 1);
        let mut buf = Buffer::empty(area);
        crate::plugin_ui::render(
            seg.widget.as_ref()?,
            area,
            &mut buf,
            &state.theme,
            state.marks,
        );
        crate::plugin_ui::row_spans(&buf, 0)
    };
    let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
    (!text.is_empty()).then_some(Seg {
        keep: Keep::Plugin,
        text,
        spans: Some(spans),
    })
}

/// Cells of the `ctx` mini bar; the cached share fills in `theme.accent`.
const CTX_CELLS: usize = 5;

/// One status segment: its text and whether it survives narrowing. The order
/// below is display order; `fit` drops from the right in the documented
/// order (loop countdown → git counts → cache → tasks → effort → sandbox →
/// model → cost → ctx), so the row reads `model · ctx · cost · sandbox ·
/// effort · tasks · cache · loop countdown · mode` at full width, with the
/// plugin `status.left` segments before it and `status.right` after.
fn segments(state: &State) -> Vec<Seg> {
    let plugins = |slot: Slot| {
        state
            .plugin_status
            .iter()
            .filter(move |s| s.slot == slot)
            .filter_map(|s| plugin_seg(state, s))
    };
    let mut out: Vec<Seg> = plugins(Slot::StatusLeft).collect();
    out.extend(
        built_in_segments(state)
            .into_iter()
            .map(|(keep, text)| Seg::built_in(keep, text)),
    );
    out.extend(plugins(Slot::StatusRight));
    out
}

fn built_in_segments(state: &State) -> Vec<(bool, String)> {
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
    let filled =
        ((s.cache_ratio.clamp(0.0, 1.0) * CTX_CELLS as f64).round() as usize).min(CTX_CELLS);
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
    // T27.7: time left until `/loop`'s next turn, e.g. `↻ 4m12s`; `cells.rs`
    // already formats a running tool call's elapsed time as `{secs}.{tenths}s`
    // (no minutes), which does not fit a countdown that can run for hours, so
    // this is its own small formatter rather than a shared one.
    let loop_countdown = state.active_loop.as_ref().map(|lp| {
        let secs = lp.next_at.saturating_sub(state.tick) / 10;
        format!("↻ {}m{}s", secs / 60, secs % 60)
    });
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
    // Right-most droppable: `fit` removes the last `false` segment first, so
    // the countdown is the first thing to go on a narrow line.
    if let Some(seg) = loop_countdown {
        out.push((false, seg));
    }
    out.push((true, format!("{head}[{mode}]{suffix}")));
    out
}

/// The same segments `--plain` (T29.1) prints once per turn: joined text, no
/// colours, no width fitting — one place builds both surfaces.
pub fn plain_text(state: &State) -> String {
    segments(state)
        .into_iter()
        .map(|seg| seg.text)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(&format!(" {} ", state.glyphs.sep))
}

/// Which segments fit `width`: segments joined in display order, plugin
/// ones removed from the right first, then the droppable built-ins (cache,
/// tasks, effort), until the joined length fits. The git head is fused to
/// the mode slot so it never separates from the row it prefixes.
fn fit(state: &State, width: u16) -> Vec<Seg> {
    let segs = segments(state);
    let sep = format!(" {} ", state.glyphs.sep);
    let mut kept: Vec<Seg> = segs.into_iter().filter(|s| !s.text.is_empty()).collect();
    loop {
        let joined = kept
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(&sep);
        if joined.len() <= width as usize {
            break;
        }
        let last = |keep| kept.iter().rposition(|s| s.keep == keep);
        let Some(pos) = last(Keep::Plugin).or_else(|| last(Keep::Droppable)) else {
            break;
        };
        kept.remove(pos);
    }
    kept
}

pub fn line(state: &State) -> Line<'static> {
    line_at(state, u16::MAX)
}

/// The status row fitted to `width` columns: `ctx` is the mini bar with the
/// cached share in `theme.accent`; `$` names the spend over the session cap
/// and turns `theme.warn` past `warn_at`.
pub fn line_at(state: &State, width: u16) -> Line<'static> {
    let segs = fit(state, width);
    let sep = Span::styled(
        format!(" {} ", state.glyphs.sep),
        Style::default().add_modifier(Modifier::DIM),
    );
    let mut spans = Vec::new();
    for (i, seg) in segs.into_iter().enumerate() {
        if i > 0 {
            spans.push(sep.clone());
        }
        match seg.spans {
            Some(own) => spans.extend(own),
            None => spans.extend(spans_for(state, &seg.text)),
        }
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
            return vec![Span::styled(
                text.to_string(),
                Style::default().fg(theme.warn),
            )];
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Msg, update};
    use crate::view::render;
    use cox_protocol::plugin::ui;
    use cox_protocol::types::{PermissionMode, SandboxMode};
    use unicode_width::UnicodeWidthStr;

    fn text(t: &str) -> Widget {
        Widget::Text(vec![vec![ui::Span {
            text: t.into(),
            ..ui::Span::default()
        }]])
    }

    fn plugin(state: &mut State, msg: PluginUiMsg) -> Vec<Cmd> {
        update(state, Msg::Plugin(msg))
    }

    fn declare(state: &mut State, id: &str, slots: Vec<Slot>) -> Vec<Cmd> {
        plugin(
            state,
            PluginUiMsg::Declare {
                plugin: id.into(),
                slots,
                commands: Vec::new(),
                keys: Vec::new(),
                renderers: Vec::new(),
            },
        )
    }

    /// Stands in for `crates/cox`: answers every `Cmd::Plugin` render with
    /// a widget and counts the calls a real plugin would have taken.
    #[derive(Default)]
    struct CountingBus {
        calls: usize,
    }

    impl CountingBus {
        fn serve(&mut self, state: &mut State, cmds: Vec<Cmd>) {
            for cmd in cmds {
                if let Cmd::Plugin(PluginRequest::Render { plugin, input }) = cmd {
                    self.calls += 1;
                    let widget = text(&format!("{plugin} #{}", self.calls));
                    let msg = PluginUiMsg::Rendered {
                        plugin,
                        slot: input.slot,
                        widget,
                    };
                    let more = update(state, Msg::Plugin(msg));
                    self.serve(state, more);
                }
            }
        }
    }

    fn two_plugins() -> State {
        let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
        state.status.model = "sonnet-5".into();
        let mut bus = CountingBus::default();
        let cmds = declare(&mut state, "left", vec![Slot::StatusLeft, Slot::Panel]);
        bus.serve(&mut state, cmds);
        let cmds = declare(&mut state, "right", vec![Slot::StatusRight]);
        bus.serve(&mut state, cmds);
        state
    }

    #[test]
    fn plugin_segments_wide() {
        let state = two_plugins();
        insta::assert_snapshot!(line_at(&state, 200).to_string());
    }

    /// Both plugin segments go before the droppable built-ins (tasks, cache).
    #[test]
    fn plugin_segments_drop_first_when_narrow() {
        let state = two_plugins();
        let narrow = line_at(&state, 90).to_string();
        assert!(!narrow.contains("left #") && !narrow.contains("right #"));
        assert!(narrow.contains("tasks"), "{narrow}");
        insta::assert_snapshot!(narrow);
    }

    #[test]
    fn view_never_calls_plugin() {
        let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
        let mut bus = CountingBus::default();
        let cmds = declare(&mut state, "acme", vec![Slot::StatusLeft]);
        bus.serve(&mut state, cmds);
        assert_eq!(bus.calls, 1);
        for width in [20, 60, 120, 200] {
            for _ in 0..10 {
                let screen = crate::view::buffer_to_string(&render(&state, width, 6));
                assert!(width < 120 || screen.contains("acme #1"), "{screen}");
            }
        }
        for _ in 0..10 {
            let cmds = update(&mut state, Msg::Tick);
            bus.serve(&mut state, cmds);
        }
        assert_eq!(bus.calls, 1, "drawing and ticking asked for no render");
        let cmds = plugin(
            &mut state,
            PluginUiMsg::Redraw {
                plugin: "acme".into(),
            },
        );
        bus.serve(&mut state, cmds);
        let cmds = update(&mut state, Msg::Resize(100, 30));
        bus.serve(&mut state, cmds);
        assert_eq!(bus.calls, 3, "one render per redraw and per resize");
    }

    #[test]
    fn slow_render_keeps_last_good_segment() {
        let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
        declare(&mut state, "acme", vec![Slot::StatusRight]);
        let slot = Slot::StatusRight;
        let id = || "acme".to_string();
        plugin(
            &mut state,
            PluginUiMsg::Rendered {
                plugin: id(),
                slot,
                widget: text("12 turns"),
            },
        );
        for _ in 0..MAX_MISSES - 1 {
            plugin(&mut state, PluginUiMsg::Missed { plugin: id(), slot });
            assert!(line(&state).to_string().contains("12 turns"));
        }
        plugin(&mut state, PluginUiMsg::Missed { plugin: id(), slot });
        let row = line(&state).to_string();
        assert!(
            row.contains("⚠ acme slow") && !row.contains("12 turns"),
            "{row}"
        );
        // Stopped for the session: no more renders, and a late answer is ignored.
        assert!(plugin(&mut state, PluginUiMsg::Redraw { plugin: id() }).is_empty());
        assert!(update(&mut state, Msg::Resize(80, 24)).is_empty());
        plugin(
            &mut state,
            PluginUiMsg::Rendered {
                plugin: id(),
                slot,
                widget: text("late"),
            },
        );
        assert!(line(&state).to_string().contains("⚠ acme slow"));
    }

    #[test]
    fn plugin_segment_text_is_sanitized_and_capped() {
        let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
        declare(&mut state, "acme", vec![Slot::StatusLeft]);
        let long = format!("\u{1b}[31mred\u{202e} {}", "x".repeat(40));
        plugin(
            &mut state,
            PluginUiMsg::Rendered {
                plugin: "acme".into(),
                slot: Slot::StatusLeft,
                widget: text(&long),
            },
        );
        let seg = segments(&state).remove(0);
        assert!(seg.text.starts_with("red x"), "{}", seg.text);
        assert_eq!(seg.text.width(), usize::from(SEGMENT_COLS));
    }
}
