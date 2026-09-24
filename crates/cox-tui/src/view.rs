//! `view`: `State` → screen. Pure over the state and a `Buffer`, so the
//! live viewport and the test harness (`render`) draw through the same
//! function and a snapshot is the real screen. Cells print through
//! `cells::cell_lines`, shared with the runtime's `insert_before`, so
//! scrollback and viewport agree. An empty composer shows `KEYMAP` hints
//! for the current context (T24.6).

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};

use crate::cells::cell_lines;
use crate::commands;
use crate::state::{Cell, Modal, State};
use crate::status;

/// Queued messages (T25.1) shown above at once before they collapse to a
/// `+n` summary — enough to see what is coming without pushing the
/// composer off screen.
const QUEUE_SHOWN: usize = 3;

/// One line per queued message (T25.1), oldest first, dim and prefixed
/// `⏸`; past `QUEUE_SHOWN` the rest collapse into one `+n` line. Each
/// message shows only its first line — same reasoning as a picker row, a
/// queued message is a label here, not a place to read the whole thing —
/// and goes through `text::sanitize` like other user text a render site
/// did not type itself.
fn queue_lines(state: &State) -> Vec<Line<'static>> {
    let style = Style::default().fg(state.theme.dim);
    let total = state.queue.len();
    let mut lines: Vec<Line<'static>> = state
        .queue
        .iter()
        .take(QUEUE_SHOWN)
        .map(|text| {
            let first = crate::text::sanitize(text.lines().next().unwrap_or(""));
            Line::styled(format!(" ⏸ {first}"), style)
        })
        .collect();
    if total > QUEUE_SHOWN {
        lines.push(Line::styled(format!(" ⏸ +{}", total - QUEUE_SHOWN), style));
    }
    lines
}

/// The empty composer's line (T24.6): the first keymap rows of the context
/// the keys are in, one per action (T25.5: as bound now), dim, in place of
/// a fixed placeholder — up to five, fewer when they would not fit in
/// `width` (never under three).
fn hints(state: &State, width: u16) -> Line<'static> {
    let sep = format!(" {} ", state.glyphs.sep);
    let mut rows = state.keymap.rows(state.context());
    rows.dedup_by_key(|(_, action)| *action);
    let mut text = String::new();
    for (n, (key, action)) in rows.into_iter().take(5).enumerate() {
        let hint = format!("{key} {}", commands::label(action));
        let next = if n == 0 { hint } else { format!("{sep}{hint}") };
        if n >= 3 && text.len() + next.len() > usize::from(width) {
            break;
        }
        text.push_str(&next);
    }
    Line::styled(text, Style::default().fg(state.theme.dim))
}

/// Draws `state` into `area`; returns where the cursor goes.
pub fn view(state: &State, area: Rect, buf: &mut Buffer) -> Option<Position> {
    let banner = u16::from(state.banner.is_some());
    // The transcript spans the full width, so one `look` serves it and the
    // approval modal, whose height is its own line count (an edit's diff).
    let look = state.look(area.width);
    let approval = match &state.modal {
        Some(Modal::Approval(a)) => a.lines(&look),
        _ => Vec::new(),
    };
    let modal = match &state.modal {
        Some(Modal::Approval(_)) => u16::try_from(approval.len()).unwrap_or(u16::MAX),
        Some(Modal::Question(q)) => q.height(),
        Some(Modal::Picker(p)) => p.height(),
        // The diff view takes the transcript's rows, not a band of its own.
        Some(Modal::Diff { .. } | Modal::Help) | None => 0,
    };
    let composer_rows = u16::try_from(state.composer.line_count().clamp(1, 5)).unwrap_or(5);
    let todo_rows = if state.show_todo {
        u16::try_from(state.todo.len() + 1).unwrap_or(u16::MAX)
    } else {
        0
    };
    let queue = queue_lines(state);
    let queue_rows = u16::try_from(queue.len()).unwrap_or(u16::MAX);
    let [
        banner_area,
        transcript,
        todo_area,
        modal_area,
        queue_area,
        composer,
        status,
    ] = Layout::vertical([
        Constraint::Length(banner),
        Constraint::Min(1),
        Constraint::Length(todo_rows),
        Constraint::Length(modal),
        Constraint::Length(queue_rows),
        Constraint::Length(composer_rows),
        Constraint::Length(1),
    ])
    .areas(area);

    if let Some(b) = &state.banner {
        b.line(&state.theme).render(banner_area, buf);
    }
    let rows = usize::from(transcript.height);
    let (lines, offset): (Vec<Line<'static>>, usize) = match &state.modal {
        // The transcript scrolls from its end; the diff view from its start.
        Some(Modal::Diff { text, scroll }) => {
            let lines = crate::diff::view_lines(text, &look);
            let offset = (*scroll).min(lines.len().saturating_sub(rows));
            (lines, offset)
        }
        Some(Modal::Help) => (
            crate::modal::help_lines(&state.glyphs, &state.theme, &state.keymap, area.width),
            0,
        ),
        _ => {
            // `Ctrl+E` (T24.4) can only reach the last tool cell still in
            // the viewport; every other cell renders with the plain `look`.
            let last_tool = state
                .transcript
                .iter()
                .rposition(|c| matches!(c, Cell::Tool { .. }));
            let lines: Vec<Line<'static>> = state
                .transcript
                .iter()
                .enumerate()
                .flat_map(|(i, c)| {
                    let mut look = look;
                    if Some(i) == last_tool {
                        look.expand_last = Some(state.expanded_last);
                    }
                    cell_lines(c, &look)
                })
                .collect();
            let offset = lines.len().saturating_sub(rows + state.scroll);
            (lines, offset)
        }
    };
    Paragraph::new(lines)
        .scroll((u16::try_from(offset).unwrap_or(u16::MAX), 0))
        .render(transcript, buf);

    if state.show_todo {
        Paragraph::new(status::todo_lines(state)).render(todo_area, buf);
    }
    match &state.modal {
        Some(Modal::Approval(_)) => Paragraph::new(approval).render(modal_area, buf),
        Some(Modal::Question(q)) => {
            Paragraph::new(q.lines(&state.glyphs, &state.theme)).render(modal_area, buf)
        }
        Some(Modal::Picker(p)) => {
            Paragraph::new(p.lines(&state.glyphs, &state.theme)).render(modal_area, buf)
        }
        Some(Modal::Diff { .. } | Modal::Help) | None => {}
    }
    if !queue.is_empty() {
        Paragraph::new(queue).render(queue_area, buf);
    }

    let [prompt, text] =
        Layout::horizontal([Constraint::Length(2), Constraint::Min(1)]).areas(composer);
    Line::styled(
        state.glyphs.mode(state.mode),
        Style::default().fg(state.theme.mode(state.mode)),
    )
    .render(prompt, buf);
    if state.composer.text().is_empty() {
        hints(state, text.width).render(text, buf);
    } else {
        state.composer.widget().render(text, buf);
    }
    status::line_at(state, status.width).render(status, buf);

    // T23.3: marks become hyperlinks (or nothing) before the frame leaves.
    crate::link::apply(buf, &state.cwd, state.caps.osc8);
    // One place for every colour on the screen, the composer widget and the
    // syntect spans included.
    crate::color::map_buffer(buf, state.depth);

    let (row, col) = state.composer.cursor();
    let x = text.x + u16::try_from(col).unwrap_or(u16::MAX);
    let y = text.y + u16::try_from(row).unwrap_or(u16::MAX);
    Some(Position::new(
        x.min(text.right().saturating_sub(1)),
        y.min(text.bottom().saturating_sub(1)),
    ))
}

/// Test harness: the screen `view` would draw at `width`×`height`.
pub fn render(state: &State, width: u16, height: u16) -> Buffer {
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    view(state, area, &mut buf);
    buf
}

/// Rows of a buffer as text, trailing spaces trimmed (snapshot form).
pub fn buffer_to_string(buf: &Buffer) -> String {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use cox_protocol::types::{PermissionMode, SandboxMode};

    /// T25.1 step 2/"Done when": two queued messages render above the
    /// composer, dim and prefixed `⏸`, oldest first.
    #[test]
    fn queue_renders_above_composer() {
        let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
        state.status.busy = true;
        state.queue.push_back("first message".to_string());
        state.queue.push_back("second message".to_string());
        let buf = render(&state, 40, 8);
        insta::assert_snapshot!(buffer_to_string(&buf));
    }

    /// T24.6 step 2: the empty composer's hints are the first `KEYMAP` rows
    /// of the context — send/mode/@/`/`/? when idle, interrupt and friends
    /// while a turn runs.
    #[test]
    fn placeholder_hints_follow_context() {
        let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
        let idle = buffer_to_string(&render(&state, 90, 4));
        state.status.busy = true;
        let running = buffer_to_string(&render(&state, 90, 4));
        assert!(idle.contains("Enter send · Shift+Tab mode cycle · @ file · / command · ? help"));
        assert!(running.contains("Esc interrupt · Ctrl+B background · Ctrl+O transcript"));
        insta::assert_snapshot!(format!("{idle}\n---\n{running}"));
    }

    /// T24.6 step 3: `?` on an empty composer draws `KEYMAP` grouped by
    /// context over the transcript; `Esc` closes it, and `?` typed after
    /// text stays a character.
    #[test]
    fn help_overlay_snapshot() {
        use crate::state::{Msg, update};
        use crossterm::event::{KeyCode, KeyEvent};
        let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
        let key = |c| Msg::Key(KeyEvent::from(c));
        update(&mut state, key(KeyCode::Char('?')));
        assert_eq!(state.modal, Some(Modal::Help));
        insta::assert_snapshot!(buffer_to_string(&render(&state, 80, 15)));
        update(&mut state, key(KeyCode::Esc));
        assert_eq!(state.modal, None);
        update(&mut state, key(KeyCode::Char('a')));
        update(&mut state, key(KeyCode::Char('?')));
        assert_eq!(
            (state.modal.clone(), state.composer.text()),
            (None, "a?".into())
        );
    }
}
