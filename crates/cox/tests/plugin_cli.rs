//! T33.4 Check: `cox plugin list` against a scratch `COX_HOME`, discovering
//! WAT fixture plugins the way installed/enabled plugins would sit on disk.
//! `list` must never compile or run a plugin's module (PL§1 line 47/445: a
//! project plugin is untrusted until granted, T33.6), so a project plugin
//! whose `cox_init` would trap is still listed, unharmed, as `discovered`.
//! T33.6 Check: a headless run loads only granted plugins and warns once per
//! ungranted one, naming the command to run.

#![cfg(feature = "plugins")]

use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;

fn cox(home: &Path, cwd: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cox"));
    cmd.current_dir(cwd)
        .env("COX_HOME", home)
        .env("HOME", home)
        .args(["--cwd", cwd.to_str().unwrap(), "plugin", "list", "--json"]);
    cmd
}

fn manifest(id: &str) -> String {
    format!(
        "api = 1\nid = \"{id}\"\nversion = \"0.1.0\"\nname = \"Demo\"\nwasm = \"plugin.wasm\"\n"
    )
}

/// Mirrors `cox-plugin`'s own host tests: a `cox_init` that echoes its
/// input, so any JSON round-trips and the host reads back an empty
/// (default) `InitOut`. Only used to prove discovery reports a real module;
/// `list` never calls it.
const ECHO_INIT: &str = r#"(module
      (import "extism:host/env" "input_length" (func $input_length (result i64)))
      (import "extism:host/env" "input_load_u8" (func $load (param i64) (result i32)))
      (import "extism:host/env" "alloc" (func $alloc (param i64) (result i64)))
      (import "extism:host/env" "store_u8" (func $store (param i64 i32)))
      (import "extism:host/env" "output_set" (func $output_set (param i64 i64)))
      (func $echo (result i32) (local $n i64) (local $off i64) (local $i i64)
        (local.set $n (call $input_length))
        (local.set $off (call $alloc (local.get $n)))
        (block $done (loop $copy
          (br_if $done (i64.ge_u (local.get $i) (local.get $n)))
          (call $store (i64.add (local.get $off) (local.get $i)) (call $load (local.get $i)))
          (local.set $i (i64.add (local.get $i) (i64.const 1)))
          (br $copy)))
        (call $output_set (local.get $off) (local.get $n))
        (i32.const 0))
      (export "cox_init" (func $echo)))"#;

/// A `cox_init` that traps immediately if ever called. `list` must not
/// execute it, so discovery must still succeed and show it as `discovered`.
const TRAPPING_INIT: &str = r#"(module
      (func $boom (result i32) unreachable)
      (export "cox_init" (func $boom)))"#;

#[test]
fn cox_plugin_list_reports_a_wat_fixture_plugin() {
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();

    let staged = home.path().join("plugins/demo/versions/staged");
    std::fs::create_dir_all(&staged).unwrap();
    std::fs::write(staged.join("plugin.toml"), manifest("demo")).unwrap();
    std::fs::write(staged.join("plugin.wasm"), ECHO_INIT).unwrap();

    let digest = cox_plugin::package_digest(&staged).expect("digest the staged tree");
    let digest12 = &digest[..12];
    let versioned = staged.with_file_name(digest12);
    std::fs::rename(&staged, &versioned).unwrap();
    std::fs::write(home.path().join("plugins/demo/current"), digest12).unwrap();

    let out = cox(home.path(), cwd.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&out).expect("json output");
    let plugins = v["plugins"].as_array().expect("plugins array");
    assert_eq!(plugins.len(), 1, "{v}");
    assert_eq!(plugins[0]["id"], "demo");
    assert_eq!(plugins[0]["source"], "user");
    assert_eq!(plugins[0]["state"], "discovered", "{v}");
    assert_eq!(plugins[0]["grant"], "unknown");
}

#[test]
fn cox_plugin_list_never_runs_a_project_plugins_module() {
    let home = tempfile::tempdir().unwrap();
    // The project root doubles as `cwd` so `find_git_root` sees it: a real
    // `.git` directory is enough, no `git init` needed for discovery.
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join(".git")).unwrap();

    let plugin_dir = repo.path().join(".cox/plugins/trap");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(plugin_dir.join("plugin.toml"), manifest("trap")).unwrap();
    std::fs::write(plugin_dir.join("plugin.wasm"), TRAPPING_INIT).unwrap();

    let out = cox(home.path(), repo.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&out).expect("json output");
    let plugins = v["plugins"].as_array().expect("plugins array");
    assert_eq!(plugins.len(), 1, "{v}");
    assert_eq!(plugins[0]["id"], "trap");
    assert_eq!(plugins[0]["source"], "project");
    assert_eq!(plugins[0]["state"], "discovered", "{v}");
    assert_eq!(plugins[0]["grant"], "unknown");
}

/// The package bytes are not wasm, so loading one is a visible "failed to
/// load" warning: that warning is the proof a plugin was loaded at all.
const NOT_WASM: &str = "not a wasm module";

const TEXT_ONLY: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../cox-core/tests/scenarios/text_only.toml"
);

fn headless(home: &Path, cwd: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cox"));
    cmd.current_dir(cwd)
        .env("COX_HOME", home)
        .env("HOME", home)
        .env("COX_PROVIDER", "scripted")
        .env("COX_SCENARIO", TEXT_ONLY)
        .args(["--cwd", cwd.to_str().unwrap(), "run", "-p", "hi"])
        .args(["--output-format", "stream-json"]);
    cmd
}

fn stdout_of(cmd: &mut Command) -> String {
    String::from_utf8(cmd.assert().success().get_output().stdout.clone()).unwrap()
}

#[test]
fn headless_never_loads_ungranted_plugin() {
    use cox_protocol::{GrantScope, PluginGrant, PluginStore as _, Store as _};

    let home = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join(".git")).unwrap();

    // A user plugin granted for its exact digest.
    let staged = home.path().join("plugins/good/versions/staged");
    std::fs::create_dir_all(&staged).unwrap();
    std::fs::write(staged.join("plugin.toml"), manifest("good")).unwrap();
    std::fs::write(staged.join("plugin.wasm"), NOT_WASM).unwrap();
    let digest = cox_plugin::package_digest(&staged).unwrap();
    std::fs::rename(&staged, staged.with_file_name(&digest[..12])).unwrap();
    std::fs::write(home.path().join("plugins/good/current"), &digest[..12]).unwrap();
    let store = cox_store::Store::open(home.path()).unwrap();
    store
        .grant_put(&PluginGrant {
            plugin_id: "good".into(),
            scope: GrantScope::User,
            digest,
            capabilities: serde_json::json!([]),
            enabled: true,
            source: serde_json::json!({}),
            decided_at: "2026-09-26T00:00:00Z".into(),
        })
        .unwrap();
    drop(store);

    // A project plugin nobody granted.
    let trap = repo.path().join(".cox/plugins/trap");
    std::fs::create_dir_all(&trap).unwrap();
    std::fs::write(trap.join("plugin.toml"), manifest("trap")).unwrap();
    std::fs::write(trap.join("plugin.wasm"), NOT_WASM).unwrap();

    let out = stdout_of(&mut headless(home.path(), repo.path()));
    assert!(out.contains("plugin good failed to load"), "{out}");
    assert!(!out.contains("plugin trap failed to load"), "{out}");
    assert_eq!(
        out.matches("run `cox plugin enable trap --project`")
            .count(),
        1,
        "{out}"
    );

    let off = stdout_of(headless(home.path(), repo.path()).arg("--no-plugins"));
    assert!(
        !off.contains("plugin good") && !off.contains("plugin trap"),
        "{off}"
    );
}
