//! T5.8: the real `cox` binary under a PTY renders a scripted turn, shows
//! the model and cost in the status line, and exits cleanly on Ctrl+C ×2.
//! Everything below the binary is the same path a user gets; only the
//! provider is a scripted double.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};

type Screen = Arc<Mutex<vt100::Parser>>;
type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

const ROWS: u16 = 30;
const COLS: u16 = 100;

/// Polls the screen until `ok` holds; a snapshot can land between
/// `insert_before` and the next draw, so callers check whole states.
///
/// The deadline is generous on purpose: a passing run never waits for it,
/// and the debug binary's first paint (SQLite, syntax themes, the workspace
/// walk) takes well over a second on a box that is also running two other
/// cargo builds — 5 s made this the one flaky test in the suite.
fn wait_until(screen: &Screen, what: &str, ok: impl Fn(&str) -> bool) -> String {
    let start = Instant::now();
    loop {
        let text = screen.lock().unwrap().screen().contents();
        if ok(&text) {
            return text;
        }
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "{what} never appeared within 30s; screen was:\n{text}"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn tui_renders_scripted_turn_and_exits_on_double_ctrl_c() {
    let home = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let scenario = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../cox-core/tests/scenarios/text_only.toml"
    );

    let pty = NativePtySystem::default()
        .openpty(PtySize {
            rows: ROWS,
            cols: COLS,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_cox"));
    cmd.args([
        "--cwd",
        work.path().to_str().unwrap(),
        "--model",
        "scripted",
    ]);
    cmd.cwd(work.path());
    cmd.env("COX_HOME", home.path());
    // The real `~/.claude.json` / `~/.claude/settings.json` must not leak
    // MCP servers or hooks into the test (T7.5/T7.6 read `$HOME`).
    cmd.env("HOME", home.path());
    cmd.env("COX_PROVIDER", "scripted");
    cmd.env("COX_SCENARIO", scenario);
    cmd.env("TERM", "xterm-256color");
    let mut child = pty.slave.spawn_command(cmd).unwrap();
    drop(pty.slave);

    let screen: Screen = Arc::new(Mutex::new(vt100::Parser::new(ROWS, COLS, 0)));
    let mut reader = pty.master.try_clone_reader().unwrap();
    let writer: Writer = Arc::new(Mutex::new(pty.master.take_writer().unwrap()));
    let (sink, replier) = (screen.clone(), writer.clone());
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        // Carries the tail of the previous read so a query split across two
        // reads is still seen.
        let mut raw: Vec<u8> = Vec::new();
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            let mut parser = sink.lock().unwrap();
            parser.process(&buf[..n]);
            raw.extend_from_slice(&buf[..n]);
            // The inline viewport queries the cursor (CSI 6n) at start and on
            // every insert_before; a real terminal answers, so this one must
            // too or the binary times out.
            let queries = raw.windows(4).filter(|w| *w == b"\x1b[6n").count();
            if queries > 0 {
                let (row, col) = parser.screen().cursor_position();
                let reply = format!("\x1b[{};{}R", row + 1, col + 1).repeat(queries);
                let _ = replier.lock().unwrap().write_all(reply.as_bytes());
            }
            let keep = raw.len().saturating_sub(3);
            raw.drain(..keep);
        }
    });
    let send = |bytes: &[u8]| {
        let mut w = writer.lock().unwrap();
        w.write_all(bytes).unwrap();
        w.flush().unwrap();
    };

    // The status line is the first thing the TUI paints.
    wait_until(&screen, "status line", |t| t.contains("$0.00"));
    send(b"hello\r");
    // Reply rendered, turn finished (Ctrl+C while `working` would interrupt
    // instead of arming quit), status shows the model and the cost.
    wait_until(&screen, "finished scripted turn", |t| {
        t.contains("hello from scripted")
            && !t.contains("working")
            && t.contains("scripted · ")
            && t.contains("$0.00")
    });

    send(b"\x03");
    wait_until(&screen, "quit prompt", |t| t.contains("again to quit"));
    send(b"\x03");

    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(start.elapsed() < Duration::from_secs(5), "cox did not exit");
        thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success(), "exit status {status:?}");
}
