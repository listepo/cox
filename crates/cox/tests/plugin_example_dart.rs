//! T33.38 Check: the Dart MCP-server example (`plugins/examples/dart`) end
//! to end. `#[ignore]`d because it needs the Dart SDK — `just
//! plugin-examples dart` builds `build/example_dart` first (this test only
//! stages/installs the package, it never compiles Dart) and then runs this
//! test through `--run-ignored only`; the `plugin-examples` CI job does
//! both, and a missing toolchain there fails the job.
//!
//! KNOWN GAP (see the T33.38 report): `crates/cox/src/session.rs`'s
//! `load_plugins` always reads `manifest.wasm` and only registers a
//! plugin's `[[mcp]]` servers once that read succeeds; `wasm` is a
//! required, non-optional field on `cox_plugin_api::PluginManifest` with
//! no wasm-less path today. The Dart example ships no `plugin.wasm` by
//! design (Dart cannot emit one extism can load, research.md §4.3.5 P44),
//! so as of this commit `cox plugin install`/`enable` stage and grant it,
//! but the session never starts `bin/server.dart`: `cox run` itself exits
//! non-zero with `"stop":{"type":"error"}` once the scripted turn tries the
//! `count` tool (confirmed locally with Dart 3.13.4 installed and the
//! example built — see the T33.38 report), so this test fails before it
//! even reaches the tool-result assertions below. Not because Dart or the
//! example are broken, but because the host has no wasm-less load path yet.
//! A follow-up card must add one before this test can pass for real.

#![cfg(feature = "plugins")]

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::Value;

const SCENARIO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/scenarios/dart_count_twice.toml"
);

/// The cox repository root, from this test binary's own manifest dir
/// (`crates/cox`).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn example_dir() -> PathBuf {
    repo_root().join("plugins/examples/dart")
}

fn cox(home: &Path, cwd: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cox"));
    cmd.current_dir(cwd)
        .env("COX_HOME", home)
        .env("HOME", home)
        .args(["--cwd", cwd.to_str().unwrap()]);
    cmd
}

fn stdout(cmd: &mut Command) -> Vec<u8> {
    cmd.assert().success().get_output().stdout.clone()
}

/// `tool_call_done` events from the rollout, `(ok, visible)` in order.
fn tool_results(home: &Path, session: &str) -> Vec<(bool, String)> {
    let path = home.join("sessions").join(format!("{session}.jsonl"));
    let text = std::fs::read_to_string(&path).unwrap();
    text.lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap()["event"].clone())
        .filter(|e| e["type"] == "tool_call_done")
        .map(|e| {
            (
                e["result"]["ok"].as_bool().unwrap_or(false),
                e["result"]["visible"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect()
}

#[test]
#[ignore = "needs dart: run `just plugin-examples dart`"]
fn plugin_example_dart() {
    let dir = example_dir();
    let exe = dir.join("build/example_dart");
    assert!(
        exe.is_file(),
        "build the example first (just plugin-examples dart): {} is missing",
        exe.display()
    );

    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let (home, cwd) = (home.path(), cwd.path());

    stdout(cox(home, cwd).args(["plugin", "install", dir.to_str().unwrap(), "--yes"]));
    stdout(cox(home, cwd).args(["plugin", "enable", "example-dart", "--yes"]));

    let list: Value =
        serde_json::from_slice(&stdout(cox(home, cwd).args(["plugin", "list", "--json"]))).unwrap();
    let row = list["plugins"]
        .as_array()
        .and_then(|plugins| plugins.iter().find(|p| p["id"] == "example-dart"))
        .unwrap_or_else(|| panic!("example-dart not in `cox plugin list --json`: {list}"));
    assert_eq!(row["loaded"], true, "not granted: {list}");

    let out: Value = serde_json::from_slice(&stdout(
        cox(home, cwd)
            .env("COX_PROVIDER", "scripted")
            .env("COX_SCENARIO", SCENARIO)
            .args(["run", "-p", "count twice", "--output-format", "json"]),
    ))
    .unwrap();
    let session = out["session"].as_str().unwrap();

    // The `[[mcp]]` server runs wrapped by `sandboxed_argv`
    // (`crates/cox/src/session.rs`'s `plugin_mcp`) unconditionally — the
    // same guard every plugin-shipped `[[mcp]]` server gets — so a
    // successful call here already proves it ran under the sandbox.
    assert_eq!(
        tool_results(home, session),
        [(true, "1".to_string()), (true, "2".to_string())],
        "the count tool, called twice in one turn, should answer 1 then 2"
    );
}
