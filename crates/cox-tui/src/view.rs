//! `view`: `State` → screen. Pure over the state and a `Buffer`, so the
//! live viewport and the test harness (`render`) draw through the same
//! function and a snapshot is the real screen. `cell_lines` is shared with
//! the runtime's `insert_before`, so scrollback and viewport agree.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};

use crate::state::{Cell, Modal, State};

/// Draws `state` into `area`; returns where the cursor goes.
pub fn view(state: &State, area: Rect, buf: &mut Buffer) -> Option<Position> {
    let banner = u16::from(state.banner.is_some());
    let modal = match &state.modal {
        Some(Modal::Approval { .. }) => 3,
        Some(Modal::Picker(p)) => p.height(),
        None => 0,
    };
    let composer_rows = u16::try_from(state.composer.line_count().clamp(1, 5)).unwrap_or(5);
    let [banner_area, transcript, modal_area, composer, status] = Layout::vertical([
        Constraint::Length(banner),
        Constraint::Min(1),
        Constraint::Length(modal),
        Constraint::Length(composer_rows),
        Constraint::Length(1),
    ])
    .areas(area);

    if let Some(b) = &state.banner {
        b.line().render(banner_area, buf);
    }
    let lines: Vec<Line<'static>> = state.transcript.iter().flat_map(cell_lines).collect();
    let offset = lines
        .len()
        .saturating_sub(usize::from(transcript.height) + state.scroll);
    Paragraph::new(lines)
        .scroll((u16::try_from(offset).unwrap_or(u16::MAX), 0))
        .render(transcript, buf);

    match &state.modal {
        Some(Modal::Approval { call, why }) => Paragraph::new(vec![
            Line::styled(
                format!(" approve {} {}?", call.name, call.subject),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::raw(format!(" {why:?}")),
            Line::raw(" [y]es  [a]lways this session  [n]o"),
        ])
        .render(modal_area, buf),
        Some(Modal::Picker(p)) => Paragraph::new(p.lines()).render(modal_area, buf),
        None => {}
    }

    let [prompt, text] =
        Layout::horizontal([Constraint::Length(2), Constraint::Min(1)]).areas(composer);
    Line::raw(">").render(prompt, buf);
    state.composer.widget().render(text, buf);
    let s = &state.status;
    Line::styled(
        format!(
            " {} · ctx {} · ${:.2} · {:?} · {:?} · {} tasks{}",
            if s.model.is_empty() { "-" } else { &s.model },
            s.context_tokens,
            s.cost_usd,
            s.sandbox,
            state.mode,
            state.tasks.len(),
            match (s.busy, state.ctrl_c_armed) {
                (true, _) => " · working",
                (false, true) => " · Ctrl+C again to quit",
                (false, false) => "",
            }
        ),
        Style::default().add_modifier(Modifier::DIM),
    )
    .render(status, buf);

    let (row, col) = state.composer.cursor();
    let x = text.x + u16::try_from(col).unwrap_or(u16::MAX);
    let y = text.y + u16::try_from(row).unwrap_or(u16::MAX);
    Some(Position::new(
        x.min(text.right().saturating_sub(1)),
        y.min(text.bottom().saturating_sub(1)),
    ))
}

/// How one cell prints, in the viewport and in scrollback.
pub fn cell_lines(cell: &Cell) -> Vec<Line<'static>> {
    match cell {
        Cell::User { text } => vec![Line::styled(
            format!("› {text}"),
            Style::default().add_modifier(Modifier::BOLD),
        )],
        Cell::Assistant { text, .. } => text.lines().map(|l| Line::raw(l.to_string())).collect(),
        Cell::Thinking { text, .. } => text
            .lines()
            .map(|l| {
                Line::styled(
                    format!("∴ {l}"),
                    Style::default().add_modifier(Modifier::DIM),
                )
            })
            .collect(),
        Cell::Tool {
            call,
            output,
            result,
        } => {
            let mut lines = vec![Line::styled(
                format!("⚙ {} {}", call.name, call.subject),
                Style::default().fg(Color::Cyan),
            )];
            lines.extend(output.lines().map(|l| Line::raw(format!("  {l}"))));
            if let Some(r) = result {
                let mark = if r.ok { "✓" } else { "✗" };
                lines.push(Line::styled(
                    format!("  {mark} {}B {}ms", r.bytes, r.duration_ms),
                    Style::default().add_modifier(Modifier::DIM),
                ));
            }
            lines
        }
        Cell::Notice { level, text } => vec![Line::styled(
            format!("[{}] {text}", format!("{level:?}").to_lowercase()),
            Style::default().fg(Color::Yellow),
        )],
    }
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
