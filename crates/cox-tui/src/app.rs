//! The runtime: crossterm input and core `Event`s on one `select!`, an
//! inline viewport, `insert_before` for finished cells so the terminal's own
//! scrollback keeps the transcript, and a panic hook that restores the
//! terminal. The only module in the crate that touches a real terminal;
//! everything it decides goes through `state::update`.

use std::io;
use std::time::Duration;

use cox_core::Session;
use cox_protocol::errors::CoreError;
use crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, Event as Input, EventStream, KeyEventKind,
};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::widgets::{Paragraph, Widget};
use ratatui::{Terminal, TerminalOptions, Viewport};

use crate::state::{Cmd, Msg, State, update};
use crate::view::{cell_lines, view};

/// Rows the live viewport keeps below the scrollback.
const VIEWPORT_ROWS: u16 = 15;

#[derive(Debug, thiserror::Error)]
pub enum TuiError {
    #[error("terminal: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("the session's event stream was already taken")]
    EventsTaken,
}

/// Runs the TUI until the user quits or the session's stream ends.
pub async fn run(session: Session, mut state: State) -> Result<(), TuiError> {
    let mut rx = session.events().ok_or(TuiError::EventsTaken)?;
    enable_raw_mode()?;
    execute!(io::stdout(), EnableBracketedPaste)?;
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        hook(info);
    }));
    let mut terminal = Terminal::with_options(
        CrosstermBackend::new(io::stdout()),
        TerminalOptions {
            viewport: Viewport::Inline(VIEWPORT_ROWS),
        },
    )?;
    let mut input = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let result = async {
        loop {
            let msg = tokio::select! {
                Some(ev) = input.next() => match ev? {
                    Input::Key(k) if k.kind != KeyEventKind::Release => Msg::Key(k),
                    Input::Paste(text) => Msg::Paste(text),
                    Input::Resize(w, h) => Msg::Resize(w, h),
                    _ => continue,
                },
                ev = rx.recv() => match ev {
                    Some(ev) => Msg::Event(ev),
                    None => return Ok(()),
                },
                _ = tick.tick() => Msg::Tick,
            };
            for cmd in update(&mut state, msg) {
                match cmd {
                    Cmd::Submit(sub) => session.submit(sub).await?,
                    Cmd::Quit => return Ok(()),
                    // Clipboard lands with the transcript cells (T5.3).
                    Cmd::Copy(_) => {}
                }
            }
            for cell in state.take_finished() {
                let lines = cell_lines(&cell);
                let height = u16::try_from(lines.len()).unwrap_or(u16::MAX);
                terminal
                    .insert_before(height, |buf| Paragraph::new(lines).render(buf.area, buf))?;
            }
            terminal.draw(|frame| {
                if let Some(pos) = view(&state, frame.area(), frame.buffer_mut()) {
                    frame.set_cursor_position(pos);
                }
            })?;
        }
    }
    .await;
    restore();
    result
}

/// Leaves the terminal usable whatever happened; safe to call twice.
fn restore() {
    let _ = execute!(io::stdout(), DisableBracketedPaste);
    let _ = disable_raw_mode();
}
