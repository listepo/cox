//! T6.1: `cox run -p` against the real binary with the scripted provider —
//! the three output shapes and the exit codes a script relies on.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Stdio};

use assert_cmd::Command;
use cox_protocol::ids::TaskId;
use cox_protocol::types::Event;
use serde_json::Value;

fn cox(work: &Path, home: &Path, scenario: &str) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cox"));
    cmd.current_dir(work)
        .env("COX_HOME", home)
        .env("HOME", home)
        .env("COX_PROVIDER", "scripted")
        .env("COX_SCENARIO", scenario)
        .args(["--cwd", work.to_str().unwrap(), "run", "-p", "hi"]);
    cmd
}

const TEXT_ONLY: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../cox-core/tests/scenarios/text_only.toml"
);
const WRITE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/scenarios/write_then_done.toml"
);

#[test]
fn text_format_prints_the_final_assistant_text() {
    let (work, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    cox(work.path(), home.path(), TEXT_ONLY)
        .assert()
        .success()
        .stdout("hello from scripted\n");
}

#[test]
fn json_format_reports_result_usage_cost_and_stop() {
    let (work, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let out = cox(work.path(), home.path(), TEXT_ONLY)
        .args(["--output-format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["result"], "hello from scripted");
    assert_eq!(v["stop"]["type"], "end_turn");
    assert_eq!(v["turns"], 1);
    assert_eq!(v["cost_usd"], 0.0);
    assert!(v["session"].as_str().unwrap().len() == 26, "{v}");
    assert!(v["usage"]["input_tokens"].is_number(), "{v}");
}

#[test]
fn stream_json_lists_every_event_and_the_claude_aliases() {
    let (work, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let out = cox(work.path(), home.path(), TEXT_ONLY)
        .args(["--output-format", "stream-json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let types: Vec<String> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| {
            serde_json::from_str::<Value>(l).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(types.first().unwrap(), "session_started");
    assert!(types.contains(&"turn_started".into()), "{types:?}");
    assert!(types.contains(&"text_delta".into()), "{types:?}");
    assert!(types.contains(&"assistant".into()), "{types:?}");
    let done = types.iter().position(|t| t == "turn_done").unwrap();
    assert_eq!(&types[done + 1..], ["result"]);
}

/// T34.7/SM§6: `stream-json`'s writer (`run.rs`) is
/// `serde_json::to_string(&cox_core::redact::scrub_event(&ev))` for every
/// `Event`, generically (D2) — `scrub_event` special-cases only
/// `TextDelta`/`ToolCallOutput`/`ToolCallDone`. `send_message` has since
/// landed (T34.6) and `subagent_messaging.rs` now proves a real
/// `TaskMessage` reaching stream-json end to end through the binary; this
/// test stays alongside it as the narrower, no-binary-spawn check of the
/// two functions the writer calls at the serializer level: the event
/// survives `scrub_event` untouched and round-trips through JSON with
/// every field intact.
#[test]
fn stream_json_passes_task_message_through_unchanged() {
    let ev = Event::TaskMessage {
        task: TaskId::new(),
        from: Some(TaskId::new()),
        hop: 2,
        text: "hi from a sibling".into(),
    };
    let scrubbed = cox_core::redact::scrub_event(&ev);
    assert_eq!(*scrubbed, ev, "scrub_event touched a TaskMessage");
    let line = serde_json::to_string(&scrubbed).unwrap();
    let v: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["type"], "task_message");
    assert_eq!(v["hop"], 2);
    assert_eq!(v["text"], "hi from a sibling");
    assert!(v["from"].is_string());
    // Round-trips to the same event: nothing dropped, renamed or reordered.
    let back: Event = serde_json::from_str(&line).unwrap();
    assert_eq!(back, ev);
}

#[test]
fn a_denied_write_exits_2_and_the_file_is_not_written() {
    let (work, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    cox(work.path(), home.path(), WRITE)
        .args(["--output-format", "json"])
        .assert()
        .code(2)
        .stdout(predicates_str_contains("\"denied\":1"));
    assert!(!work.path().join("a.txt").exists());
}

const GIT_THEN_TOUCH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/scenarios/bash_git_then_touch.toml"
);

/// T36.1: `Bash(git:*)` covers the `git` command, not the `touch` chained
/// after it, so the line asks, and headless turns the ask into a deny; with
/// a rule for each command the same line runs without asking.
#[test]
fn a_prefix_rule_does_not_allow_a_command_chained_after_it() {
    let (work, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let rules = |allow: &str| format!("[permissions]\nallow = [{allow}]\n");
    std::fs::write(home.path().join("config.toml"), rules(r#""Bash(git:*)""#)).unwrap();
    cox(work.path(), home.path(), GIT_THEN_TOUCH)
        .args(["--output-format", "json"])
        .assert()
        .code(2)
        .stdout(predicates_str_contains("\"denied\":1"));
    assert!(!work.path().join("chained").exists());

    let both = r#""Bash(git:*)", "Bash(touch:*)""#;
    std::fs::write(home.path().join("config.toml"), rules(both)).unwrap();
    cox(work.path(), home.path(), GIT_THEN_TOUCH)
        .assert()
        .success()
        .stdout("done\n");
    assert!(work.path().join("chained").exists());
}

#[test]
fn auto_mode_writes_the_file_and_exits_0() {
    let (work, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    cox(work.path(), home.path(), WRITE)
        .args(["--permission-mode", "auto"])
        .assert()
        .success()
        .stdout("done\n");
    assert_eq!(
        std::fs::read_to_string(work.path().join("a.txt")).unwrap(),
        "x"
    );
}

#[test]
fn unknown_output_format_is_an_error() {
    let (work, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    cox(work.path(), home.path(), TEXT_ONLY)
        .args(["--output-format", "yaml"])
        .assert()
        .code(1);
}

fn predicates_str_contains(needle: &'static str) -> impl predicates::Predicate<[u8]> {
    predicates::function::function(move |out: &[u8]| {
        std::str::from_utf8(out).is_ok_and(|s| s.contains(needle))
    })
}

/// The real binary with piped stdio, for the driver protocol (T6.3).
fn interactive(work: &Path, home: &Path, extra: &[&str]) -> Child {
    std::process::Command::new(env!("CARGO_BIN_EXE_cox"))
        .current_dir(work)
        .env("COX_HOME", home)
        .env("HOME", home)
        .env("COX_PROVIDER", "scripted")
        .env("COX_SCENARIO", WRITE)
        .args([
            "--cwd",
            work.to_str().unwrap(),
            "run",
            "-p",
            "hi",
            "--approve",
            "on-request",
        ])
        .args(extra)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap()
}

/// Reads stream-json lines until the ask, answers it with `reply(call_id)`,
/// and returns the exit code.
fn answer_ask(child: &mut Child, reply: impl FnOnce(&str) -> String) -> i32 {
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut stdin = child.stdin.take().unwrap();
    let mut line = String::new();
    loop {
        line.clear();
        assert!(
            stdout.read_line(&mut line).unwrap() > 0,
            "no ask before EOF"
        );
        let v: Value = serde_json::from_str(&line).unwrap();
        if v["type"] == "approval_required" {
            writeln!(stdin, "{}", reply(v["call"]["id"].as_str().unwrap())).unwrap();
            break;
        }
    }
    let mut rest = String::new();
    std::io::Read::read_to_string(&mut stdout, &mut rest).unwrap();
    child.wait().unwrap().code().unwrap()
}

#[test]
fn approve_line_on_stdin_lets_the_write_run() {
    let (work, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let mut child = interactive(
        work.path(),
        home.path(),
        &["--output-format", "stream-json"],
    );
    let code = answer_ask(&mut child, |id| format!(r#"{{"approve":"{id}"}}"#));
    assert_eq!(code, 0);
    assert_eq!(
        std::fs::read_to_string(work.path().join("a.txt")).unwrap(),
        "x"
    );
}

#[test]
fn approve_deny_line_exits_2_without_writing() {
    let (work, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let mut child = interactive(
        work.path(),
        home.path(),
        &["--output-format", "stream-json"],
    );
    let code = answer_ask(&mut child, |id| {
        format!(r#"{{"deny":"{id}","reason":"nope"}}"#)
    });
    assert_eq!(code, 2);
    assert!(!work.path().join("a.txt").exists());
}

#[test]
fn approve_silence_times_out_into_a_denial() {
    let (work, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::write(home.path().join("config.toml"), "[hooks]\ntimeout_s = 1\n").unwrap();
    let mut child = interactive(work.path(), home.path(), &["--output-format", "json"]);
    let _stdin = child.stdin.take(); // held open and silent
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["denied"], 1, "{v}");
    assert!(!work.path().join("a.txt").exists());
}

#[test]
fn ext_lists_commands_and_agents_from_the_project_tree() {
    let work = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(work.path().join(".git")).unwrap();
    std::fs::create_dir_all(work.path().join(".claude/commands")).unwrap();
    std::fs::create_dir_all(work.path().join(".cox/agents")).unwrap();
    std::fs::write(
        work.path().join(".claude/commands/review.md"),
        "Review $ARGUMENTS",
    )
    .unwrap();
    std::fs::write(
        work.path().join(".cox/agents/scout.md"),
        "---\nname: scout\ndescription: looks around\n---\nbody",
    )
    .unwrap();
    std::fs::write(work.path().join("AGENTS.md"), "be nice").unwrap();
    let out = assert_cmd::Command::cargo_bin("cox")
        .unwrap()
        .args(["--cwd", work.path().to_str().unwrap(), "ext"])
        .env("COX_HOME", home.path())
        .env("HOME", home.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.starts_with("instructions:\n") && text.contains("AGENTS.md\n"),
        "{text}"
    );
    assert!(text.contains("commands:\n  review"), "{text}");
    // Embedded explore/shell first, the project tree's scout appended.
    assert!(
        text.contains("agents:\n  explore\n  shell\n  scout"),
        "{text}"
    );
    assert!(text.contains("notices: none"), "{text}");
}

#[test]
fn ext_list_marks_a_disabled_agent_but_still_shows_it() {
    // T34.10: `disabled: true` hides a def from the `agent` tool's own
    // description (crates/cox-core/src/subagent.rs), but `cox ext list`
    // still reports it, marked, since it exists on disk either way.
    let work = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(work.path().join(".cox/agents")).unwrap();
    std::fs::write(
        work.path().join(".cox/agents/blocked.md"),
        "---\nname: blocked\ndescription: not for the model\ndisabled: true\n---\nbody",
    )
    .unwrap();
    let out = assert_cmd::Command::cargo_bin("cox")
        .unwrap()
        .args(["--cwd", work.path().to_str().unwrap(), "ext", "list"])
        .env("COX_HOME", home.path())
        .env("HOME", home.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("blocked (disabled)"), "{text}");
}

/// `cox stats` reads the store under `COX_HOME`, not one it creates in the
/// working directory — the latter made every session look unbilled.
#[test]
fn stats_reads_the_cox_home_store_not_the_working_directory() {
    let (work, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    cox(work.path(), home.path(), TEXT_ONLY).assert().success();
    let out = assert_cmd::Command::cargo_bin("cox")
        .unwrap()
        .current_dir(work.path())
        .args(["stats", "--csv"])
        .env("COX_HOME", home.path())
        .env("HOME", home.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("main"), "{text}");
    assert!(
        !work.path().join("cox.db").exists(),
        "stats created a store in the working directory"
    );
}

#[test]
fn sessions_grep_finds_a_scripted_run() {
    let (work, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    cox(work.path(), home.path(), TEXT_ONLY).assert().success();
    let out = assert_cmd::Command::cargo_bin("cox")
        .unwrap()
        .args([
            "--home",
            home.path().to_str().unwrap(),
            "sessions",
            "--grep",
            "hello",
        ])
        .env("COX_HOME", home.path())
        .env("HOME", home.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("hello from scripted"), "{text}");
}

#[test]
fn resume_followup_prompt_reuses_session_id() {
    let (work, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let out1 = cox(work.path(), home.path(), TEXT_ONLY)
        .args(["--output-format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v1: Value = serde_json::from_slice(&out1).unwrap();
    let session_id = v1["session"].as_str().unwrap();
    let out2 = assert_cmd::Command::new(env!("CARGO_BIN_EXE_cox"))
        .current_dir(work.path())
        .env("COX_HOME", home.path())
        .env("HOME", home.path())
        .env("COX_PROVIDER", "scripted")
        .env("COX_SCENARIO", TEXT_ONLY)
        .args([
            "--cwd",
            work.path().to_str().unwrap(),
            "run",
            "-p",
            "followup",
            "--resume",
            session_id,
            "--output-format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v2: Value = serde_json::from_slice(&out2).unwrap();
    assert_eq!(v2["session"].as_str(), Some(session_id));
}
