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
/// `src/bin/kitty_probe.rs` — which exists purely for this test — stands
/// in: it drives `cox_tui::app::run` for real and quits itself a moment
/// after start.
mod kitty_pty {
    use std::io::{Read, Write};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};

    /// Spawns `kitty_probe` with `COX_KITTY_PROBE_KITTY` set, answers its
    /// `CSI 6n` cursor queries the same way `tui_e2e.rs` does for the real
    /// binary (the inline viewport needs an answer or it stalls), waits for
    /// it to quit itself, and returns every raw byte the PTY saw.
    pub fn run_probe(kitty: bool) -> Vec<u8> {
        let pty = NativePtySystem::default()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");
        let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_kitty_probe"));
        cmd.env("COX_KITTY_PROBE_KITTY", if kitty { "1" } else { "0" });
        cmd.env("TERM", "xterm-256color");
        let mut child = pty.slave.spawn_command(cmd).expect("spawn kitty_probe");
        drop(pty.slave);

        let captured = Arc::new(Mutex::new(Vec::new()));
        let mut reader = pty.master.try_clone_reader().expect("clone reader");
        let writer = Arc::new(Mutex::new(pty.master.take_writer().expect("writer")));
        let sink = captured.clone();
        let replier = writer.clone();
        thread::spawn(move || {
            let mut parser = vt100::Parser::new(24, 80, 0);
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
                    let _ = replier.lock().unwrap().write_all(reply.as_bytes());
                }
                let keep = tail.len().saturating_sub(3);
                tail.drain(..keep);
            }
        });

        let start = Instant::now();
        loop {
            if child.try_wait().expect("try_wait").is_some() {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "kitty_probe did not exit"
            );
            thread::sleep(Duration::from_millis(20));
        }
        // `restore()` writes the pop sequence right as the process exits;
        // give the reader thread a moment to drain it.
        thread::sleep(Duration::from_millis(100));
        captured.lock().unwrap().clone()
    }
}

#[test]
fn pty_pops_keyboard_flags_on_exit() {
    let with = kitty_pty::run_probe(true);
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

    let without = kitty_pty::run_probe(false);
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
