//! T23.1: a tiny binary that exists only for `tests/shell.rs`'s
//! `pty_pops_keyboard_flags_on_exit`. `cox-tui` is a library with no binary
//! of its own (`crates/cox` owns the real one) and has no `[[bin]]`-worthy
//! reason to grow one — but a PTY e2e needs a real process whose controlling
//! terminal is the PTY slave, the way `crates/cox/tests/tui_e2e.rs` spawns
//! the actual `cox` binary; this is that process for `cox_tui::app::run`
//! alone. `NullProvider` stands in for `cox-provider`'s real backends: a
//! `[[bin]]` target only ever gets `[dependencies]` (never
//! `[dev-dependencies]`), and a regular `cox-provider` dependency would
//! break `crates/cox/tests/deps.rs`'s crate-direction rule (`cox-tui` may
//! only depend on `cox-core`/`cox-protocol` among workspace crates); this
//! probe never submits a turn, so nothing here is ever actually called. It
//! quits itself shortly after start by feeding two synthetic `Ctrl+C` on
//! the same channel the real binary uses for off-screen events — the same
//! path an idle-then-repeated `Ctrl+C` from a real keyboard takes.
//!
//! `COX_PROBE_SCENARIO=cells` (T23.2) first feeds 40 finished `Notice`
//! cells on that channel, so `shell.rs` can count what each `insert_before`
//! costs on screen; core events are the same `Msg::Event`s a session would
//! deliver, so no provider has to run.

use std::sync::Arc;
use std::time::Duration;

use cox_core::{MemoryStore, Session};
use cox_protocol::Config;
use cox_protocol::errors::ProviderError;
use cox_protocol::traits::Provider;
use cox_protocol::types::{Caps, Event, Level, ProviderEvent, ProviderId, Request, Usage};
use cox_protocol::types::{PermissionMode, SandboxMode};
use cox_tui::state::{Msg, State};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Never actually called: this probe never submits a turn.
struct NullProvider;

#[async_trait::async_trait]
impl Provider for NullProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Local
    }

    fn capabilities(&self) -> Caps {
        Caps {
            cache: false,
            thinking: false,
            server_tools: false,
            count_tokens: false,
            max_context: 0,
        }
    }

    async fn stream(
        &self,
        _req: Request,
        _sink: mpsc::Sender<ProviderEvent>,
        _cancel: CancellationToken,
    ) -> Result<Usage, ProviderError> {
        Err(ProviderError::Overloaded)
    }

    async fn count_tokens(&self, _req: &Request) -> Result<u32, ProviderError> {
        Err(ProviderError::Overloaded)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let kitty = std::env::var("COX_KITTY_PROBE_KITTY").as_deref() == Ok("1");
    let cwd = std::env::temp_dir();

    let mut config = Config::default();
    config.core.workspace_roots = vec![cwd.clone()];
    let store = Arc::new(MemoryStore::new());
    let session = Session::new(
        config,
        Arc::new(NullProvider),
        Vec::new(),
        store.clone(),
        store,
        cwd,
    )
    .expect("session opens");

    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    state.caps.kitty_keyboard = kitty;

    let (feed_tx, feed_rx) = mpsc::channel::<Msg>(4);
    let (ask_tx, _ask_rx) = mpsc::channel::<cox_tui::state::Ask>(4);
    let (_question_tx, question_rx) = mpsc::channel::<cox_tui::app::Question>(1);
    let (persist_tx, _persist_rx) = mpsc::channel::<(String, String)>(4);

    let scenario = std::env::var("COX_PROBE_SCENARIO").unwrap_or_default();
    tokio::spawn(async move {
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        if scenario == "cells" {
            for n in 1..=40 {
                let _ = feed_tx.send(notice(n)).await;
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
        let _ = feed_tx.send(Msg::Key(ctrl_c)).await;
        // Idle, so the first arms quit rather than interrupting a turn; the
        // second (still idle) turns into `Cmd::Quit`.
        tokio::time::sleep(Duration::from_millis(150)).await;
        let _ = feed_tx.send(Msg::Key(ctrl_c)).await;
    });

    let _ = cox_tui::app::run(session, state, feed_rx, ask_tx, question_rx, persist_tx).await;
}

/// A one-line finished cell whose text the PTY test can find again.
fn notice(n: u32) -> Msg {
    Msg::Event(Event::Notice {
        level: Level::Info,
        text: format!("cell-{n:02}"),
    })
}
