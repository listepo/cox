//! T33.28 Check: the Rust reference plugin (`plugins/examples/rust`, built by
//! `cox-plugin-fixtures`) end to end. The real binary installs it into a
//! scratch `COX_HOME`, `enable --yes` grants it, and two `run -p` turns with
//! the scripted provider each fail one tool call: the rollout carries the
//! hook's notice with the count kept in the plugin's kv store across both
//! processes, and `cox plugin list --json` shows what the manifest
//! contributes. The subscription, status segment and `/example:reset` are
//! driven through the host API, since a headless run has no status row.

#![cfg(feature = "plugins")]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use assert_cmd::Command;
use cox_plugin::{Lane, LivePlugins, PluginHost};
use cox_plugin_api::{CommandIn, CommandOut, EventBatch, RenderIn, Slot, Widget};
use cox_plugin_fixtures::EXAMPLE_DIR;
use cox_protocol::config::PluginsConfig;
use cox_protocol::ids::SessionId;
use cox_protocol::traits::{PluginStore, Store as _};
use serde_json::{Value, json};

const SCENARIO: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/scenarios/read_missing_then_done.toml"
);

fn cox(home: &Path, cwd: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cox"));
    cmd.current_dir(cwd)
        .env("COX_HOME", home)
        .env("HOME", home)
        .env("COX_PROVIDER", "scripted")
        .env("COX_SCENARIO", SCENARIO)
        .args(["--cwd", cwd.to_str().unwrap()]);
    cmd
}

fn stdout(cmd: &mut Command) -> Vec<u8> {
    cmd.assert().success().get_output().stdout.clone()
}

/// The notices the plugin's hook left in the session's rollout.
fn hook_notices(home: &Path, session: &str) -> Vec<String> {
    let path = home.join("sessions").join(format!("{session}.jsonl"));
    let text = std::fs::read_to_string(&path).unwrap();
    text.lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap()["event"].clone())
        .filter(|e| e["type"] == "notice")
        .filter_map(|e| e["text"].as_str().map(str::to_owned))
        .filter(|t| t.starts_with("plugin example: failed tool calls"))
        .collect()
}

#[test]
fn example_plugin_counts_failures_across_a_two_turn_headless_run() {
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let (home, cwd) = (home.path(), cwd.path());

    stdout(cox(home, cwd).args(["plugin", "install", EXAMPLE_DIR, "--yes"]));
    stdout(cox(home, cwd).args(["plugin", "enable", "example", "--yes"]));

    let first: Value = serde_json::from_slice(&stdout(cox(home, cwd).args([
        "run",
        "-p",
        "read it",
        "--output-format",
        "json",
    ])))
    .unwrap();
    let session = first["session"].as_str().unwrap().to_string();
    stdout(cox(home, cwd).args(["run", "-p", "again", "--resume", &session]));

    assert_eq!(
        hook_notices(home, &session),
        [
            "plugin example: failed tool calls: 1",
            "plugin example: failed tool calls: 2"
        ]
    );

    let list: Value =
        serde_json::from_slice(&stdout(cox(home, cwd).args(["plugin", "list", "--json"]))).unwrap();
    let row = &list["plugins"][0];
    assert_eq!(row["id"], "example", "{list}");
    assert_eq!(row["loaded"], true, "{list}");
    let declared = &row["declared"];
    assert_eq!(declared["events"], json!(["turn_started"]), "{list}");
    assert_eq!(
        declared["hooks"],
        json!(["PostToolUse", "PostToolUseFailure"]),
        "{list}"
    );
    assert_eq!(declared["kv"], true, "{list}");
    assert_eq!(declared["ui"]["status"], true, "{list}");
    assert_eq!(declared["ui"]["commands"], true, "{list}");
}

fn call<O: serde::de::DeserializeOwned>(
    host: &PluginHost,
    export: &str,
    input: &impl serde::Serialize,
) -> O {
    host.call(Lane::Control, export, input, Duration::from_secs(5))
        .unwrap()
        .unwrap()
}

fn status_text(host: &PluginHost) -> String {
    let render = RenderIn {
        slot: Slot::StatusRight,
        width: 80,
        height: 1,
    };
    match call(host, "cox_render", &render) {
        Widget::Text(lines) => lines[0][0].text.clone(),
        other => panic!("a text segment, got {other:?}"),
    }
}

#[test]
fn example_plugin_counts_turns_in_its_status_and_resets_them() {
    let home = tempfile::tempdir().unwrap();
    let dir = Path::new(EXAMPLE_DIR);
    let (manifest, _) =
        cox_plugin::discover::load_manifest(dir, &dir.join("plugin.toml"), None).unwrap();
    let wasm = std::fs::read(dir.join(&manifest.wasm)).unwrap();
    let store: Arc<dyn PluginStore> = Arc::new(cox_store::Store::open(home.path()).unwrap());
    let mut live = LivePlugins::default();
    live.load(&manifest, &wasm, store).unwrap();
    let warnings = live.start(&PluginsConfig::default(), SessionId::new(), home.path());
    assert!(warnings.is_empty(), "{warnings:?}");

    let plugin = &live.plugins()[0];
    let init = plugin.init_out().unwrap();
    assert_eq!(init.subscribe, ["turn_started"]);
    assert_eq!(plugin.granted_status(), [Slot::StatusRight]);
    assert_eq!(plugin.granted_commands()[0].name, "reset");

    let host = plugin.host();
    let batch = EventBatch {
        first_seq: 1,
        dropped: 0,
        events: vec![
            json!({ "type": "turn_started", "seq": 1 }),
            json!({ "type": "turn_started", "seq": 2 }),
        ],
    };
    let effects: cox_plugin_api::Effects = call(host, "cox_on_event", &batch);
    assert!(effects.redraw);
    assert_eq!(status_text(host), "turns 2 · failed tools 0");

    let reset = CommandIn {
        name: "reset".into(),
        args: String::new(),
    };
    let out: CommandOut = call(host, "cox_command", &reset);
    assert!(matches!(out, CommandOut::Notice(n) if n.text == "counters reset"));
    assert_eq!(status_text(host), "turns 0 · failed tools 0");
}
