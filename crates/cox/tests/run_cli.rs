//! T6.1: `cox run -p` against the real binary with the scripted provider —
//! the three output shapes and the exit codes a script relies on.

use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;

fn cox(work: &Path, home: &Path, scenario: &str) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cox"));
    cmd.current_dir(work)
        .env("COX_HOME", home)
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
