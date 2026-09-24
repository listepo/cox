//! The runtime: crossterm input (polled on a thread) and core `Event`s on
//! one `select!`, an
//! inline viewport, `insert_before` for finished cells so the terminal's own
//! scrollback keeps the transcript, a resize that waits for the size to
//! settle and then rebuilds the viewport where it was (T23.7), and a panic
//! hook that restores the terminal. The only module in the crate that touches a real terminal;
//! everything it decides goes through `state::update`.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use cox_core::Session;
use cox_protocol::errors::CoreError;
use cox_protocol::ids::CallId;
use crossterm::cursor::MoveTo;
use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, EnableBracketedPaste, EnableFocusChange,
    Event as Input, KeyEventKind, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Size;
use ratatui::widgets::{Paragraph, Widget};
use ratatui::{Terminal, TerminalOptions, Viewport};

use crate::cells::cell_lines;
use crate::state::{Ask, Cmd, Msg, State, update};
use crate::view::view;

/// Rows the live viewport keeps below the scrollback; a short terminal
/// gets two fewer than its height so some scrollback stays visible.
const VIEWPORT_ROWS: u16 = 15;

/// A resize arrives as a burst of size changes while a window is dragged;
/// acting on each would stack stale viewports in scrollback, so the
/// viewport is rebuilt only once two size reads this far apart agree.
const SETTLE: Duration = Duration::from_millis(16);

type Term = Terminal<CrosstermBackend<io::Stdout>>;

/// Why the TUI stopped; the binary uses this to quit or start a fresh session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiOutcome {
    Quit,
    Clear,
    /// `/fork [turn]` (T26.3): continue in a child with the history up to `turn`.
    Fork {
        turn: Option<u32>,
    },
    /// `/handoff <objective>` (T26.3): continue in a summary-seeded child.
    Handoff {
        objective: String,
    },
}

/// One `ask_user` call surfaced by the binary (T22.1), mirroring
/// `cox_tools::ask_user::Question` without this crate taking a `cox-tools`
/// dependency (same reason `state::GitStatus` mirrors `cox_tools::git::Status`).
/// `reply` never reaches `State`: a `Modal` must stay `Clone`/`PartialEq` for
/// tests and snapshots, and a `oneshot::Sender` is neither, so `run` keeps
/// it in `pending` and answers it once `update` turns a key into `Cmd::Answer`.
pub struct Question {
    pub call: CallId,
    pub question: String,
    pub options: Vec<String>,
    pub reply: tokio::sync::oneshot::Sender<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum TuiError {
    #[error("terminal: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("the session's event stream was already taken")]
    EventsTaken,
}

/// Runs the TUI until the user quits or the session's stream ends. The
/// caller fills `state.files` (the `@` picker's candidates) from
/// `cox_tools::glob::workspace_files` — this crate never walks the disk.
/// `feed` carries what the runtime learns off-screen (the live sessions of
/// this workspace, T16.3; git counts, T15.2) for the same reason, and
/// `ask` carries what the TUI wants fetched (the diff, T15.3); the answer
/// arrives on `feed`. `questions` carries each `ask_user` call (T22.1); its
/// reply sender is answered from here, never from `state::update`. `persist`
/// carries a `/theme` choice's `(key, value)` (T24.2) out to `config_cmd::set`
/// — this crate has no `toml_edit`-editing path of its own.
pub async fn run(
    session: Session,
    mut state: State,
    mut feed: tokio::sync::mpsc::Receiver<Msg>,
    ask: tokio::sync::mpsc::Sender<Ask>,
    mut questions: tokio::sync::mpsc::Receiver<Question>,
    persist: tokio::sync::mpsc::Sender<(String, String)>,
) -> Result<TuiOutcome, TuiError> {
    let mut rx = session.events().ok_or(TuiError::EventsTaken)?;
    enable_raw_mode()?;
    execute!(io::stdout(), EnableBracketedPaste)?;
    // T23.1: only a terminal `cox_tui::term::Caps::query` already found to
    // report `CSI ?u` support gets the push — everything else keeps the
    // plain `Esc`-prefixed encoding it always had.
    let kitty = state.caps.kitty_keyboard;
    if kitty {
        execute!(
            io::stdout(),
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            )
        )?;
    }
    // T23.5: focus reports decide whether `tui.notify = auto` rings.
    let focus = state.caps.focus;
    if focus {
        execute!(io::stdout(), EnableFocusChange)?;
    }
    let vte = crate::term::is_vte(&|k| std::env::var(k).ok());
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore(kitty, focus);
        hook(info);
    }));
    let mut terminal = inline_terminal(crossterm::terminal::size()?.1)?;
    let mut built_for = terminal.size()?;
    // The last size read while a resize settles, and when it was taken.
    let mut settling: Option<(Size, Instant)> = None;
    // The cursor's row inside the viewport after the last draw: after a
    // resize the terminal has moved the cursor along with its line, so this
    // is how far above it the old viewport starts.
    let mut cursor_row = 0;
    let stop = Arc::new(AtomicBool::new(false));
    let mut input = spawn_input(stop.clone());
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    // The reply sender for whichever `ask_user` call the modal shows now;
    // `Cmd::Answer` looks it up here instead of carrying it through `State`.
    let mut pending: Option<(CallId, tokio::sync::oneshot::Sender<String>)> = None;
    let result = async {
        loop {
            let msg = tokio::select! {
                Some(ev) = input.recv() => match ev? {
                    Input::Key(k) if k.kind != KeyEventKind::Release => Msg::Key(k),
                    Input::Paste(text) => Msg::Paste(text),
                    Input::Resize(w, h) => Msg::Resize(w, h),
                    Input::FocusGained => Msg::Focus(true),
                    Input::FocusLost => Msg::Focus(false),
                    _ => continue,
                },
                ev = rx.recv() => match ev {
                    Some(ev) => Msg::Event(ev),
                    None => return Ok(TuiOutcome::Quit),
                },
                _ = tick.tick() => Msg::Tick,
                Some(msg) = feed.recv() => msg,
                Some(q) = questions.recv() => {
                    pending = Some((q.call, q.reply));
                    Msg::Question {
                        call: q.call,
                        question: q.question,
                        options: q.options,
                    }
                }
            };
            let ticked = matches!(msg, Msg::Tick);
            for cmd in update(&mut state, msg) {
                match cmd {
                    // Same reason as the headless loop: `submit` runs a whole
                    // turn, and an approval must be answerable meanwhile.
                    Cmd::Submit(sub) => {
                        let session = session.clone();
                        tokio::spawn(async move {
                            // Failures surface as `Event::Error` on the stream.
                            let _ = session.submit(sub).await;
                        });
                    }
                    Cmd::Quit => return Ok(TuiOutcome::Quit),
                    Cmd::Clear => return Ok(TuiOutcome::Clear),
                    Cmd::Fork(turn) => return Ok(TuiOutcome::Fork { turn }),
                    Cmd::Handoff(objective) => return Ok(TuiOutcome::Handoff { objective }),
                    // Clipboard lands with the transcript cells (T5.3).
                    Cmd::Copy(_) => {}
                    // A request the runtime has not answered yet is still
                    // pending, so a repeat is dropped rather than awaited.
                    Cmd::Ask(what) => {
                        let _ = ask.try_send(what);
                    }
                    // `None` (Esc) drops `reply` instead of sending it, so
                    // `ask_user` sees the call as dismissed, not answered
                    // with empty text.
                    Cmd::Answer(call, answer) => {
                        if let Some((pending_call, reply)) = pending.take() {
                            if pending_call == call {
                                if let Some(text) = answer {
                                    let _ = reply.send(text);
                                }
                            } else {
                                pending = Some((pending_call, reply));
                            }
                        }
                    }
                    // Best-effort: a full channel or a closed receiver just
                    // means this one preview is not persisted; the picker
                    // already applied it to `state` either way.
                    Cmd::PersistConfig { key, value } => {
                        let _ = persist.try_send((key, value));
                    }
                    Cmd::Notify { title, body } => {
                        use std::io::Write;
                        let bytes = crate::term::notification(&state.caps, vte, &title, &body);
                        let mut out = io::stdout();
                        out.write_all(bytes.as_bytes())?;
                        out.flush()?;
                    }
                }
            }
            // While a resize settles nothing is drawn or inserted: finished
            // cells wait in `state`, and ratatui's own `autoresize` — which
            // clears the whole screen when the width shrinks — never runs.
            let size = terminal.size()?;
            if size != built_for || settling.is_some() {
                settling = match settling {
                    Some((last, at)) if last == size && ticked && at.elapsed() >= SETTLE => {
                        terminal = rebuild(&mut terminal, cursor_row, size.height)?;
                        built_for = size;
                        None
                    }
                    Some((last, at)) if last == size => Some((last, at)),
                    _ => Some((size, Instant::now())),
                };
                if settling.is_some() {
                    continue;
                }
            }
            let look = state.look(size.width);
            let depth = state.depth;
            for cell in state.take_finished() {
                let lines = cell_lines(&cell, &look);
                let height = u16::try_from(lines.len()).unwrap_or(u16::MAX);
                // Scrollback goes through the same colour mapping as the
                // viewport; it is written straight to the terminal.
                terminal.insert_before(height, |buf| {
                    Paragraph::new(lines).render(buf.area, buf);
                    crate::link::apply(buf, &state.cwd, state.caps.osc8);
                    crate::color::map_buffer(buf, depth);
                })?;
            }
            let mut drawn = None;
            terminal.draw(|frame| {
                let area = frame.area();
                let pos = view(&state, area, frame.buffer_mut());
                if let Some(pos) = pos {
                    frame.set_cursor_position(pos);
                }
                drawn = Some((area, pos));
            })?;
            match drawn {
                Some((area, Some(pos))) => cursor_row = pos.y.saturating_sub(area.y),
                // A hidden cursor stays wherever the diff ended; parking it
                // on the viewport's first row keeps `cursor_row` true.
                Some((area, None)) => {
                    terminal.set_cursor_position(area.as_position())?;
                    cursor_row = 0;
                }
                None => {}
            }
        }
    }
    .await;
    stop.store(true, Ordering::Relaxed);
    restore(kitty, focus);
    result
}

/// An inline terminal whose viewport fits a screen `height` rows tall.
fn inline_terminal(height: u16) -> io::Result<Term> {
    let rows = VIEWPORT_ROWS.min(height.saturating_sub(2)).max(1);
    Terminal::with_options(
        CrosstermBackend::new(io::stdout()),
        TerminalOptions {
            viewport: Viewport::Inline(rows),
        },
    )
}

/// Clears the old viewport where the resized terminal now shows it and
/// builds a fresh one there, so its stale frame is neither left on screen
/// nor scrolled into scrollback, and the lines above it stay untouched.
fn rebuild(terminal: &mut Term, cursor_row: u16, height: u16) -> io::Result<Term> {
    let cursor = terminal.get_cursor_position()?;
    let top = cursor.y.saturating_sub(cursor_row);
    execute!(
        io::stdout(),
        MoveTo(0, top),
        Clear(ClearType::FromCursorDown)
    )?;
    inline_terminal(height)
}

/// crossterm's `EventStream` holds the input-reader lock while it waits, and
/// ratatui's inline `insert_before` needs that same lock to ask the terminal
/// for the cursor position — the query times out under a stream. Polling
/// with a short timeout on a thread releases the lock between polls.
fn spawn_input(stop: Arc<AtomicBool>) -> tokio::sync::mpsc::Receiver<io::Result<Input>> {
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let ev = match crossterm::event::poll(Duration::from_millis(50)) {
                Ok(true) => crossterm::event::read(),
                Ok(false) => continue,
                Err(e) => Err(e),
            };
            let failed = ev.is_err();
            if tx.blocking_send(ev).is_err() || failed {
                return;
            }
        }
    });
    rx
}

/// Leaves the terminal usable whatever happened; safe to call twice. `kitty`
/// pops the Kitty keyboard protocol flags first — popping when nothing was
/// pushed is a no-op on every terminal that implements the spec, but `run`
/// only pays for the round trip when its own push actually happened; `focus`
/// likewise turns off the focus reports only `run` turned on.
fn restore(kitty: bool, focus: bool) {
    if kitty {
        let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
    }
    if focus {
        let _ = execute!(io::stdout(), DisableFocusChange);
    }
    let _ = execute!(io::stdout(), DisableBracketedPaste);
    let _ = disable_raw_mode();
}
