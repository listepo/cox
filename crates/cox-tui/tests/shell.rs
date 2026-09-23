//! Git-aware completion (T15.4): `Tab` on a `git` line opens the picker over
//! subcommands, branches or paths by position, and a choice replaces the
//! word being typed. Any other line keeps Tab's old meaning.

use cox_protocol::types::{PermissionMode, SandboxMode};
use cox_tui::picker::Kind;
use cox_tui::state::{Modal, Msg, State, update};
use crossterm::event::{KeyCode, KeyEvent};

/// Types `line` into a fresh composer and presses Tab.
fn tab_after(line: &str) -> State {
    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    state.files = vec!["src/lib.rs".into(), "README.md".into()];
    state.git_branches = vec!["main".into(), "feature/x".into()];
    for c in line.chars() {
        update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Char(c))));
    }
    update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Tab)));
    state
}

fn offered(state: &State) -> Vec<String> {
    match &state.modal {
        Some(Modal::Picker(p)) if p.kind == Kind::Shell => p.matches.clone(),
        other => panic!("no shell picker: {other:?}"),
    }
}

#[test]
fn shell_tab_offers_a_subcommand_a_branch_or_a_path_by_position() {
    assert_eq!(offered(&tab_after("git ch"))[0], "checkout");
    assert_eq!(offered(&tab_after("git checkout ma")), ["main"]);
    assert_eq!(offered(&tab_after("git add sr")), ["src/lib.rs"]);
}

#[test]
fn shell_tab_on_another_command_opens_nothing() {
    assert!(tab_after("ls ").modal.is_none());
}

#[test]
fn shell_choice_replaces_the_word_being_typed() {
    let mut state = tab_after("git checkout ma");
    update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Enter)));
    assert!(state.modal.is_none());
    assert_eq!(state.composer.text(), "git checkout main ");
}

/// T23.1: `cox-tui` has no binary of its own to spawn under a PTY (unlike
/// `crates/cox/tests/tui_e2e.rs`, which spawns the real `cox`), so
/// `src/bin/kitty_probe.rs` — which exists purely for these tests — stands
/// in: it drives `cox_tui::app::run` for real and quits itself once its
/// scenario is fed.
mod pty {
    use std::io::{Read, Write};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    use portable_pty::{Child, CommandBuilder, NativePtySystem, PtySize, PtySystem};

    /// Generous on purpose: a slow CI box must never turn into a flake.
    const DEADLINE: Duration = Duration::from_secs(30);

    /// Spawns `kitty_probe` with `env` on a `rows`×`cols` PTY, answers its
    /// `CSI 6n` cursor queries the same way `tui_e2e.rs` does for the real
    /// binary (the inline viewport needs an answer or it stalls), waits for
    /// it to quit itself, and returns every raw byte the PTY saw.
    pub fn run_probe(env: &[(&str, &str)], rows: u16, cols: u16) -> Vec<u8> {
        let pty = NativePtySystem::default()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");
        let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_kitty_probe"));
        for (key, value) in env {
            cmd.env(key, value);
        }
        cmd.env("TERM", "xterm-256color");
        let mut child = pty.slave.spawn_command(cmd).expect("spawn kitty_probe");
        drop(pty.slave);

        let captured = Arc::new(Mutex::new(Vec::new()));
        let mut reader = pty.master.try_clone_reader().expect("clone reader");
        let mut writer = pty.master.take_writer().expect("writer");
        let sink = captured.clone();
        let reading = thread::spawn(move || {
            let mut parser = vt100::Parser::new(rows, cols, 0);
            let mut buf = [0u8; 4096];
            // Carries the tail of the previous read so a query split across
            // two reads is still seen, like `tui_e2e.rs`.
            let mut tail: Vec<u8> = Vec::new();
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                parser.process(&buf[..n]);
                sink.lock().unwrap().extend_from_slice(&buf[..n]);
                tail.extend_from_slice(&buf[..n]);
                let queries = tail.windows(4).filter(|w| *w == b"\x1b[6n").count();
                if queries > 0 {
                    let (row, col) = parser.screen().cursor_position();
                    let reply = format!("\x1b[{};{}R", row + 1, col + 1).repeat(queries);
                    let _ = writer.write_all(reply.as_bytes());
                }
                let keep = tail.len().saturating_sub(3);
                tail.drain(..keep);
            }
        });

        wait_exit(child.as_mut());
        // The master reports EOF/EIO once the last slave fd closes, so the
        // reader ends by itself after draining what the probe wrote last
        // (`restore()` writes right as the process exits).
        let start = Instant::now();
        while !reading.is_finished() {
            assert!(start.elapsed() < DEADLINE, "PTY reader did not drain");
            thread::sleep(Duration::from_millis(10));
        }
        drop(pty.master);
        captured.lock().unwrap().clone()
    }

    fn wait_exit(child: &mut (dyn Child + Send + Sync)) {
        let start = Instant::now();
        while child.try_wait().expect("try_wait").is_none() {
            assert!(start.elapsed() < DEADLINE, "kitty_probe did not exit");
            thread::sleep(Duration::from_millis(20));
        }
    }
}

#[test]
fn pty_pops_keyboard_flags_on_exit() {
    let with = pty::run_probe(&[("COX_KITTY_PROBE_KITTY", "1")], 24, 80);
    assert!(
        with.windows(5).any(|w| w == b"\x1b[>3u"),
        "push flags missing: {:?}",
        String::from_utf8_lossy(&with)
    );
    assert!(
        with.windows(5).any(|w| w == b"\x1b[<1u"),
        "pop flags missing: {:?}",
        String::from_utf8_lossy(&with)
    );

    let without = pty::run_probe(&[("COX_KITTY_PROBE_KITTY", "0")], 24, 80);
    assert!(
        !without.windows(5).any(|w| w == b"\x1b[>3u"),
        "push flags present without the capability: {:?}",
        String::from_utf8_lossy(&without)
    );
    assert!(
        !without.windows(5).any(|w| w == b"\x1b[<1u"),
        "pop flags present without the capability: {:?}",
        String::from_utf8_lossy(&without)
    );
}

/// Rows `app.rs` keeps for the live viewport (`VIEWPORT_ROWS`).
const VIEWPORT_ROWS: usize = 15;

/// A sentinel no cox frame draws, so a cell still holding it was not
/// touched by the frame under test.
const SENTINEL: &str = "\u{a4}";

/// Replays `raw` offline and counts the frames that rewrote the whole
/// viewport. ratatui ends every `draw` by showing or hiding the cursor and
/// `insert_before` never does, so each `?25h`/`?25l` closes one frame (the
/// inserts that preceded that draw included). Before each frame every
/// screen cell is overwritten with `SENTINEL` in a copy of the screen; a
/// row with no sentinel left afterwards was erased or fully redrawn, and a
/// frame that did that to at least `VIEWPORT_ROWS` rows repainted the
/// viewport rather than diffing it.
fn full_repaints(raw: &[u8], rows: u16, cols: u16) -> usize {
    let mut live = vt100::Parser::new(rows, cols, 0);
    let mut seed = b"\x1b7".to_vec();
    for row in 1..=rows {
        seed.extend(format!("\x1b[{row};1H{}", SENTINEL.repeat(cols.into())).bytes());
    }
    seed.extend(b"\x1b8");
    let mut repaints = 0;
    let mut rest = raw;
    while !rest.is_empty() {
        let end = rest
            .windows(6)
            .position(|w| w == b"\x1b[?25h" || w == b"\x1b[?25l")
            .map_or(rest.len(), |at| at + 6);
        let (frame, next) = rest.split_at(end);
        rest = next;
        let mut copy = vt100::Parser::new(rows, cols, 0);
        copy.process(&live.screen().state_formatted());
        copy.process(&seed);
        copy.process(frame);
        live.process(frame);
        let screen = copy.screen();
        let rewritten = (0..rows)
            .filter(|&r| {
                (0..cols).all(|c| screen.cell(r, c).is_none_or(|x| x.contents() != SENTINEL))
            })
            .count();
        if rewritten >= VIEWPORT_ROWS {
            repaints += 1;
        }
    }
    repaints
}

/// T23.2: 40 finished cells stream into scrollback on an 80×24 PTY.
/// Without `scrolling-regions` every `insert_before` clears the viewport
/// and the next draw repaints all of it (measured: 40 repaints for 40
/// cells); with the feature the region above the viewport scrolls and the
/// viewport is only diffed (measured: 0). The bound is the card's "at most
/// once per cell" tightened to one for the whole run, so the test fails if
/// the feature is ever dropped.
#[test]
fn pty_insert_before_repaints_at_most_once_per_cell() {
    let raw = pty::run_probe(&[("COX_PROBE_SCENARIO", "cells")], 24, 80);
    let shown = String::from_utf8_lossy(&raw);
    assert!(
        shown.contains("cell-40"),
        "the last cell never reached the screen"
    );
    let repaints = full_repaints(&raw, 24, 80);
    assert!(
        repaints <= 1,
        "{repaints} full-viewport repaints for 40 inserted cells"
    );
}
