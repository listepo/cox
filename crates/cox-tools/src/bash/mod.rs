//! `bash`: one shell command under the session's sandbox policy on a PTY,
//! with streamed output, an env allowlist and a SIGTERM→SIGKILL timeout
//! (plan.md T3.7, §1.11). Separate from the other tools because it is the
//! only one that spawns a process; `classify` has its own file so the
//! permission engine can rate a command line without running it.

mod classify;

use std::io::Read;
use std::os::fd::{BorrowedFd, RawFd};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use cox_protocol::{
    ArchivePut, Concurrency, Risk, SandboxPolicy, TaskId, Tool, ToolCx, ToolError, ToolOutput,
    ToolSpec,
};
use nix::poll::{PollFd, PollFlags, poll};
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub use classify::classify;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
/// How long a process gets between SIGTERM and SIGKILL.
const TERM_GRACE: Duration = Duration::from_secs(2);
/// How long to wait for the PTY to drain after the shell exited before
/// giving up on grandchildren that still hold it open.
const REAP_GRACE: Duration = Duration::from_millis(500);
/// How long one `poll` on the master waits before re-checking the phase.
const POLL_SLICE_MS: u8 = 50;
/// Reader phases: read while the child runs, read what is left once it
/// exited, stop even if a grandchild keeps writing.
const RUNNING: u8 = 0;
const DRAINING: u8 = 1;
const STOP: u8 = 2;
/// The child inherits only these; everything else (API keys above all)
/// stays in cox's own environment.
const ENV_ALLOWLIST: &[&str] = &[
    "PATH", "HOME", "LANG", "LC_ALL", "LC_CTYPE", "TERM", "TMPDIR", "USER", "SHELL",
];

/// `bash`: runs one command line and returns its stripped output.
pub struct BashTool;

#[derive(Debug, Deserialize, JsonSchema)]
struct BashInput {
    /// The command line, run with `sh -c` in the session's working directory.
    command: String,
    /// Seconds before the command is sent SIGTERM, then SIGKILL (default 120).
    #[serde(default)]
    timeout_s: Option<u64>,
    /// Run detached and return a task id; the output is archived when it finishes.
    #[serde(default)]
    background: bool,
}

#[async_trait]
impl Tool for BashTool {
    fn spec(&self) -> ToolSpec {
        let input_schema = serde_json::to_value(schema_for!(BashInput)).unwrap_or(Value::Null);
        ToolSpec {
            name: "bash".to_string(),
            description: "Runs a shell command line in the workspace and returns its output \
                (stdout and stderr interleaved, ANSI stripped) followed by `[exit <code> in \
                <ms>]`. Output streams while the command runs; a long-running command is \
                stopped after `timeout_s` seconds (default 120). Prefer the dedicated `read`, \
                `grep`, `glob` and `edit` tools for file work; use `bash` for builds, tests, \
                git and anything that needs a process. Pass `background: true` for a server \
                or watcher you do not want to wait for."
                .to_string(),
            input_schema,
            deferred: false,
            risk: Risk::Exec,
            concurrency: Concurrency::Exclusive,
        }
    }

    fn subject(&self, input: &Value) -> String {
        input
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    fn risk(&self, input: &Value) -> Risk {
        match input.get("command").and_then(Value::as_str) {
            Some(command) => classify(command),
            None => Risk::Exec,
        }
    }

    async fn call(&self, input: Value, cx: &ToolCx) -> Result<ToolOutput, ToolError> {
        let input: BashInput = serde_json::from_value(input).map_err(|e| ToolError::Denied {
            why: format!("invalid bash input: {e}"),
        })?;
        let timeout = input
            .timeout_s
            .filter(|s| *s > 0)
            .map_or(DEFAULT_TIMEOUT, Duration::from_secs);
        if input.background {
            return Ok(background(input.command, timeout, cx));
        }
        let run = run(
            &input.command,
            &cx.cwd,
            &cx.sandbox,
            &cx.cancel,
            &cx.output,
            timeout,
        )
        .await?;
        Ok(ToolOutput {
            is_error: run.ended.is_some() || run.code != Some(0),
            text: run.render(),
            diff: None,
            structured: None,
        })
    }
}

/// Spawns the command detached from the turn: it outlives cancellation and
/// its full output lands in the archive under this call. `TaskCreated`/
/// `TaskCompleted` and a way to fetch the row by task id arrive with T9.2.
fn background(command: String, timeout: Duration, cx: &ToolCx) -> ToolOutput {
    let task = TaskId::new();
    let (cwd, sandbox, archive) = (cx.cwd.clone(), cx.sandbox.clone(), cx.archive.clone());
    let (session, call) = (cx.session, cx.call);
    let subject = command.clone();
    tokio::spawn(async move {
        // The turn's output channel closes when this call returns, so the
        // background run streams into a sink nobody reads.
        let (sink, _) = mpsc::channel(1);
        if let Ok(run) = run(
            &command,
            &cwd,
            &sandbox,
            &CancellationToken::new(),
            &sink,
            timeout,
        )
        .await
        {
            let _ = archive
                .put(ArchivePut {
                    session,
                    call,
                    tool: "bash".into(),
                    subject: Some(command),
                    bytes: run.render().into_bytes(),
                })
                .await;
        }
    });
    ToolOutput {
        text: format!(
            "background task {task} started: {subject}\n\
             its output is archived under call {call} when it finishes"
        ),
        is_error: false,
        diff: None,
        structured: None,
    }
}

struct Run {
    raw: Vec<u8>,
    code: Option<u32>,
    /// Why the command was stopped early, if it was.
    ended: Option<&'static str>,
    elapsed: Duration,
}

impl Run {
    fn render(&self) -> String {
        let ms = self.elapsed.as_millis();
        let tail = match (self.ended, self.code) {
            (Some(why), _) => format!("[{why} after {ms}ms; killed]"),
            (None, Some(code)) => format!("[exit {code} in {ms}ms]"),
            (None, None) => format!("[exit unknown in {ms}ms]"),
        };
        let body = strip_ansi(&self.raw);
        if body.is_empty() {
            tail
        } else {
            format!("{}\n{tail}", body.trim_end_matches('\n'))
        }
    }
}

/// Builds the child. The sandbox policy is threaded through here so P4 can
/// wrap the command (Seatbelt/Landlock/bwrap) in one place; until then every
/// mode runs the command as-is.
fn command_for(command: &str, cwd: &Path, _sandbox: &SandboxPolicy) -> CommandBuilder {
    let mut cmd = CommandBuilder::new("/bin/sh");
    cmd.arg("-c");
    cmd.arg(command);
    cmd.cwd(cwd);
    cmd.env_clear();
    for key in ENV_ALLOWLIST {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
    // Fewer escape sequences to strip, and no pager waiting on a TTY.
    cmd.env("NO_COLOR", "1");
    cmd.env("PAGER", "cat");
    cmd.env("GIT_PAGER", "cat");
    cmd
}

fn signal(pid: Option<u32>, sig: Signal) {
    // The child is its own session leader (portable-pty calls setsid), so
    // its pid is the process group of everything it started.
    if let Some(pid) = pid {
        let _ = killpg(Pid::from_raw(pid as i32), sig);
    }
}

async fn run(
    command: &str,
    cwd: &Path,
    sandbox: &SandboxPolicy,
    cancel: &CancellationToken,
    output: &mpsc::Sender<String>,
    timeout: Duration,
) -> Result<Run, ToolError> {
    let start = Instant::now();
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 40,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|_| ToolError::Io)?;
    let mut child = pair
        .slave
        .spawn_command(command_for(command, cwd, sandbox))
        .map_err(|_| ToolError::Io)?;
    let pid = child.process_id();
    let mut reader = pair.master.try_clone_reader().map_err(|_| ToolError::Io)?;
    let fd = pair.master.as_raw_fd();
    let master = pair.master;
    // Our slave stays open until the reader is done: macOS throws away
    // whatever the master has not read yet when the last slave closes, so
    // a quick `echo ok` would otherwise exit before its output arrived.
    // The reader therefore stops on the exit status plus a drain, not EOF.
    let slave = pair.slave;

    let phase = Arc::new(AtomicU8::new(RUNNING));
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(64);
    let reader_phase = phase.clone();
    tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 8192];
        loop {
            let phase = reader_phase.load(Ordering::Relaxed);
            if phase == STOP {
                break;
            }
            if fd.is_none_or(|fd| readable(fd, POLL_SLICE_MS)) {
                match reader.read(&mut buf) {
                    Ok(n) if n > 0 => {
                        if tx.blocking_send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    _ => break,
                }
            } else if phase == DRAINING {
                break;
            }
        }
        drop(slave);
        drop(master);
    });
    let mut wait = tokio::task::spawn_blocking(move || child.wait());

    let timer = tokio::time::sleep(timeout);
    tokio::pin!(timer);
    let mut run = Run {
        raw: Vec::new(),
        code: None,
        ended: None,
        elapsed: Duration::ZERO,
    };
    let mut exited = false;
    let mut drained = false;
    let mut killed = false;
    loop {
        tokio::select! {
            chunk = rx.recv(), if !drained => match chunk {
                Some(bytes) => {
                    let _ = output.send(strip_ansi(&bytes)).await;
                    run.raw.extend_from_slice(&bytes);
                }
                None => {
                    drained = true;
                    if exited {
                        break;
                    }
                }
            },
            status = &mut wait, if !exited => {
                exited = true;
                run.code = status.ok().and_then(Result::ok).map(|s| s.exit_code());
                phase.store(DRAINING, Ordering::Relaxed);
                if drained {
                    break;
                }
                timer.as_mut().reset(tokio::time::Instant::now() + REAP_GRACE);
            }
            _ = cancel.cancelled(), if run.ended.is_none() => {
                run.ended = Some("cancelled");
                signal(pid, Signal::SIGTERM);
                timer.as_mut().reset(tokio::time::Instant::now() + TERM_GRACE);
            }
            _ = &mut timer => {
                if exited {
                    // The shell is gone but something it started still writes.
                    signal(pid, Signal::SIGKILL);
                    break;
                }
                if run.ended.is_none() {
                    run.ended = Some("timed out");
                    signal(pid, Signal::SIGTERM);
                } else if !killed {
                    killed = true;
                    signal(pid, Signal::SIGKILL);
                } else {
                    break;
                }
                timer.as_mut().reset(tokio::time::Instant::now() + TERM_GRACE);
            }
        }
    }
    phase.store(STOP, Ordering::Relaxed);
    if !exited {
        signal(pid, Signal::SIGKILL);
    }
    run.elapsed = start.elapsed();
    Ok(run)
}

/// Whether the master has bytes to read within `timeout`, so the reader
/// can notice `DRAINING`/`STOP` instead of blocking forever on a PTY that
/// a grandchild still holds open.
fn readable(fd: RawFd, timeout_ms: u8) -> bool {
    // SAFETY: `fd` is the master's descriptor and the master is owned by the
    // reader thread that calls this, so it stays open for the whole call.
    let fd = unsafe { BorrowedFd::borrow_raw(fd) };
    let mut fds = [PollFd::new(fd, PollFlags::POLLIN)];
    matches!(poll(&mut fds, timeout_ms), Ok(n) if n > 0)
}

/// Drops CSI/OSC escape sequences, carriage returns and other control
/// bytes so the model sees plain text; `cox-tui` sanitises again for
/// display, this is only about not paying tokens for colour codes.
fn strip_ansi(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => match chars.next() {
                Some('[') => {
                    // Parameters and intermediates end at the first final byte.
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    // OSC ends with BEL or ESC \.
                    while let Some(c) = chars.next() {
                        if c == '\u{7}' {
                            break;
                        }
                        if c == '\u{1b}' {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\n' | '\t' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_drops_colour_osc_and_carriage_returns() {
        let raw = b"\x1b[1;32mok\x1b[0m\r\n\x1b]0;title\x07done\r\n";
        assert_eq!(strip_ansi(raw), "ok\ndone\n");
    }

    #[test]
    fn render_reports_exit_code_or_why_it_was_killed() {
        let done = Run {
            raw: b"hi\r\n".to_vec(),
            code: Some(0),
            ended: None,
            elapsed: Duration::from_millis(5),
        };
        assert_eq!(done.render(), "hi\n[exit 0 in 5ms]");
        let killed = Run {
            raw: Vec::new(),
            code: None,
            ended: Some("timed out"),
            elapsed: Duration::from_millis(1000),
        };
        assert_eq!(killed.render(), "[timed out after 1000ms; killed]");
    }

    #[test]
    fn bash_risk_comes_from_the_command_line() {
        let tool = BashTool;
        assert_eq!(
            tool.risk(&serde_json::json!({"command": "ls"})),
            Risk::ReadOnly
        );
        assert_eq!(
            tool.risk(&serde_json::json!({"command": "rm -rf x"})),
            Risk::Destructive
        );
        assert_eq!(tool.risk(&serde_json::json!({})), Risk::Exec);
        assert_eq!(tool.subject(&serde_json::json!({"command": "ls"})), "ls");
    }
}
