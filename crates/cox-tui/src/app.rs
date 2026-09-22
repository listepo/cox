//! The runtime: crossterm input (polled on a thread) and core `Event`s on
//! one `select!`, an
//! inline viewport, `insert_before` for finished cells so the terminal's own
//! scrollback keeps the transcript, and a panic hook that restores the
//! terminal. The only module in the crate that touches a real terminal;
//! everything it decides goes through `state::update`.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use cox_core::Session;
use cox_protocol::errors::CoreError;
use cox_protocol::ids::CallId;
use crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, Event as Input, KeyEventKind,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::backend::CrosstermBackend;
use ratatui::widgets::{Paragraph, Widget};
use ratatui::{Terminal, TerminalOptions, Viewport};

use crate::cells::cell_lines;
use crate::state::{Ask, Cmd, Msg, State, update};
use crate::view::view;

/// Rows the live viewport keeps below the scrollback.
const VIEWPORT_ROWS: u16 = 15;

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
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore(kitty);
        hook(info);
    }));
    let mut terminal = Terminal::with_options(
        CrosstermBackend::new(io::stdout()),
        TerminalOptions {
            viewport: Viewport::Inline(VIEWPORT_ROWS),
        },
    )?;
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
                }
            }
            let look = state.look(terminal.size()?.width);
            let depth = state.depth;
            for cell in state.take_finished() {
                let lines = cell_lines(&cell, &look);
                let height = u16::try_from(lines.len()).unwrap_or(u16::MAX);
                // Scrollback goes through the same colour mapping as the
                // viewport; it is written straight to the terminal.
                terminal.insert_before(height, |buf| {
                    Paragraph::new(lines).render(buf.area, buf);
                    crate::color::map_buffer(buf, depth);
                })?;
            }
            terminal.draw(|frame| {
                if let Some(pos) = view(&state, frame.area(), frame.buffer_mut()) {
                    frame.set_cursor_position(pos);
                }
            })?;
        }
    }
    .await;
    stop.store(true, Ordering::Relaxed);
    restore(kitty);
    result
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
/// only pays for the round trip when its own push actually happened.
fn restore(kitty: bool) {
    if kitty {
        let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
    }
    let _ = execute!(io::stdout(), DisableBracketedPaste);
    let _ = disable_raw_mode();
}
