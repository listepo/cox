//! `view`: `State` → screen. Pure over the state and a `Buffer`, so the
//! live viewport and the test harness (`render`) draw through the same
//! function and a snapshot is the real screen. Cells print through
//! `cells::cell_lines`, shared with the runtime's `insert_before`, so
//! scrollback and viewport agree.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};

use crate::cells::cell_lines;
use crate::state::{Modal, State};
use crate::status;

/// Draws `state` into `area`; returns where the cursor goes.
pub fn view(state: &State, area: Rect, buf: &mut Buffer) -> Option<Position> {
    let banner = u16::from(state.banner.is_some());
    let modal = match &state.modal {
        Some(Modal::Approval(a)) => a.height(),
        Some(Modal::Picker(p)) => p.height(),
        None => 0,
    };
    let composer_rows = u16::try_from(state.composer.line_count().clamp(1, 5)).unwrap_or(5);
    let todo_rows = if state.show_todo {
        u16::try_from(state.todo.len() + 1).unwrap_or(u16::MAX)
    } else {
        0
    };
    let [
        banner_area,
        transcript,
        todo_area,
        modal_area,
        composer,
        status,
    ] = Layout::vertical([
        Constraint::Length(banner),
        Constraint::Min(1),
        Constraint::Length(todo_rows),
        Constraint::Length(modal),
        Constraint::Length(composer_rows),
        Constraint::Length(1),
    ])
    .areas(area);

    if let Some(b) = &state.banner {
        b.line().render(banner_area, buf);
    }
    let look = state.look(transcript.width);
    let lines: Vec<Line<'static>> = state
        .transcript
        .iter()
        .flat_map(|c| cell_lines(c, &look))
        .collect();
    let offset = lines
        .len()
        .saturating_sub(usize::from(transcript.height) + state.scroll);
    Paragraph::new(lines)
        .scroll((u16::try_from(offset).unwrap_or(u16::MAX), 0))
        .render(transcript, buf);

    if state.show_todo {
        Paragraph::new(status::todo_lines(state)).render(todo_area, buf);
    }
    match &state.modal {
        Some(Modal::Approval(a)) => Paragraph::new(a.lines(&state.glyphs)).render(modal_area, buf),
        Some(Modal::Picker(p)) => Paragraph::new(p.lines(&state.glyphs)).render(modal_area, buf),
        None => {}
    }

    let [prompt, text] =
        Layout::horizontal([Constraint::Length(2), Constraint::Min(1)]).areas(composer);
    Line::raw(">").render(prompt, buf);
    state.composer.widget().render(text, buf);
    status::line(state).render(status, buf);

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
