//! T33.4 Check: `cox plugin list` against a scratch `COX_HOME`, discovering
//! WAT fixture plugins the way installed/enabled plugins would sit on disk.
//! `list` must never compile or run a plugin's module (PL§1 line 47/445: a
//! project plugin is untrusted until granted, T33.6), so a project plugin
//! whose `cox_init` would trap is still listed, unharmed, as `discovered`.
//! T33.6 Check: a headless run loads only granted plugins and warns once per
//! ungranted one, naming the command to run.
//! T33.7 Check: `install` → `enable --yes` → `list` reports `loaded`;
//! `disable` → `list` reports `not loaded`; a project plugin needs
//! `--project` to be granted at all (`project_plugin_needs_project_grant`).
//! T33.31 Check: `update --check` shows the diff, `update` needs a
//! re-grant, `--rollback` restores the old digest without asking, and a
//! headless `update` keeps `current` and warns.

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

/// `cox plugin <args...>` against a scratch `COX_HOME`/`cwd`. The test
/// process's own stdin is not a terminal (`cargo nextest run` redirects
/// it), so a prompt-less run (no `--yes`) reads EOF on `read_line` and
/// declines deterministically, same as `assert_cmd`'s other users of
/// `std::io::stdin()` in this workspace.
fn cox_plugin(home: &Path, cwd: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cox"));
    cmd.current_dir(cwd)
        .env("COX_HOME", home)
        .env("HOME", home)
        .args(["--cwd", cwd.to_str().unwrap(), "plugin"])
        .args(args);
    cmd
}

fn list_json(home: &Path, cwd: &Path) -> Value {
    let out = cox(home, cwd)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&out).expect("json output")
}

/// The human-readable `cox plugin list` (no `--json`), for the literal
/// `loaded`/`not loaded` words the T33.7 Check names.
fn list_text(home: &Path, cwd: &Path) -> String {
    let out = cox_plugin(home, cwd, &["list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    String::from_utf8(out).unwrap()
}

fn write_plugin(dir: &Path, id: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("plugin.toml"), manifest(id)).unwrap();
    std::fs::write(dir.join("plugin.wasm"), "dummy wasm bytes").unwrap();
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
    // T33.7: no grant on file, so `grant::check` reports `NeedsApproval`,
    // not the old `"unknown"` placeholder.
    assert_eq!(plugins[0]["grant"], "needs_approval", "{v}");
    assert_eq!(plugins[0]["loaded"], false, "{v}");
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
    assert_eq!(plugins[0]["grant"], "needs_approval", "{v}");
    assert_eq!(plugins[0]["loaded"], false, "{v}");
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

/// T33.7 Check: `install` (declined by default: stdin is closed, so the
/// prompt reads EOF and the grant is not written) → `enable --yes` grants
/// it → `list` reports `loaded` → `disable` → `list` reports `not loaded`.
#[test]
fn install_then_enable_yes_then_list_shows_loaded_then_disable_shows_not_loaded() {
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    write_plugin(src.path(), "demo");

    // `install` alone: no `--yes` and no stdin, so the approval prompt
    // declines and no grant is written — only the files and `current` land.
    cox_plugin(
        home.path(),
        cwd.path(),
        &["install", src.path().to_str().unwrap()],
    )
    .assert()
    .success();
    assert!(
        home.path().join("plugins/demo/current").exists(),
        "install must write `current` even when the grant is declined"
    );

    let before = list_json(home.path(), cwd.path());
    assert_eq!(before["plugins"][0]["grant"], "needs_approval", "{before}");
    assert_eq!(before["plugins"][0]["loaded"], false, "{before}");

    // A separate `enable --yes` grants it.
    cox_plugin(home.path(), cwd.path(), &["enable", "demo", "--yes"])
        .assert()
        .success();

    let enabled = list_json(home.path(), cwd.path());
    assert_eq!(enabled["plugins"][0]["grant"], "granted", "{enabled}");
    assert_eq!(enabled["plugins"][0]["loaded"], true, "{enabled}");

    let text = list_text(home.path(), cwd.path());
    assert!(text.contains("loaded"), "{text}");
    assert!(!text.contains("not loaded"), "{text}");

    // `disable` clears `enabled`; the files and grant row stay.
    cox_plugin(home.path(), cwd.path(), &["disable", "demo"])
        .assert()
        .success();

    let disabled = list_json(home.path(), cwd.path());
    assert_eq!(disabled["plugins"][0]["grant"], "disabled", "{disabled}");
    assert_eq!(disabled["plugins"][0]["loaded"], false, "{disabled}");

    let text_after = list_text(home.path(), cwd.path());
    assert!(text_after.contains("not loaded"), "{text_after}");
}

/// T33.7 Check `project_plugin_needs_project_grant`: a project plugin is
/// repository content (PL§1), so `cox plugin enable <id>` without
/// `--project` must not find it — discovery never looks under
/// `.cox/plugins/` unless `--project` says so — and the grant is scoped to
/// that repository root, not to every repository.
#[test]
fn project_plugin_needs_project_grant() {
    let home = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join(".git")).unwrap();
    write_plugin(&repo.path().join(".cox/plugins/proj"), "proj");

    // Without `--project`, discovery never looks at the repository, so
    // there is nothing to grant.
    let out = cox_plugin(home.path(), repo.path(), &["enable", "proj", "--yes"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("not found"), "{out}");
    assert!(out.contains("--project"), "{out}");

    let still_ungranted = list_json(home.path(), repo.path());
    assert_eq!(
        still_ungranted["plugins"][0]["grant"], "needs_approval",
        "{still_ungranted}"
    );

    // With `--project`, it is found, shown in words and granted.
    cox_plugin(
        home.path(),
        repo.path(),
        &["enable", "proj", "--project", "--yes"],
    )
    .assert()
    .success();

    let granted = list_json(home.path(), repo.path());
    assert_eq!(granted["plugins"][0]["grant"], "granted", "{granted}");
    assert_eq!(granted["plugins"][0]["loaded"], true, "{granted}");

    // The grant is scoped to this repository root, not to every one.
    let other_repo = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(other_repo.path().join(".git")).unwrap();
    write_plugin(&other_repo.path().join(".cox/plugins/proj"), "proj");
    let elsewhere = list_json(home.path(), other_repo.path());
    assert_eq!(
        elsewhere["plugins"][0]["grant"], "needs_approval",
        "a project grant must not cover a different repository: {elsewhere}"
    );
}

fn stdout_ok(cmd: &mut Command) -> String {
    String::from_utf8(cmd.assert().success().get_output().stdout.clone()).unwrap()
}

fn current_of(home: &Path, id: &str) -> String {
    std::fs::read_to_string(home.join("plugins").join(id).join("current")).unwrap()
}

/// Installs `demo` from `src` with `--yes`, returning its `current`.
fn installed(home: &Path, cwd: &Path, src: &Path) -> String {
    write_plugin(src, "demo");
    cox_plugin(home, cwd, &["install", src.to_str().unwrap(), "--yes"])
        .assert()
        .success();
    current_of(home, "demo")
}

/// T33.31 Check: install → rebuild with changed bytes and a widened
/// manifest → `update --check` shows the diff and changes nothing →
/// `update` without `--yes` keeps `current` → `update --yes` re-grants and
/// switches → `--rollback` restores the old digest without asking (stdin
/// is not a terminal, so any prompt would have declined).
#[test]
fn update_check_regrant_then_rollback_restores_old_digest() {
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let old = installed(home.path(), cwd.path(), src.path());

    let up_to_date = stdout_ok(&mut cox_plugin(
        home.path(),
        cwd.path(),
        &["update", "demo"],
    ));
    assert!(up_to_date.contains("up to date"), "{up_to_date}");

    std::fs::write(src.path().join("plugin.wasm"), "rebuilt wasm bytes").unwrap();
    let widened = format!("{}\n[capabilities]\nkv = true\n", manifest("demo"));
    std::fs::write(src.path().join("plugin.toml"), widened).unwrap();

    let check = stdout_ok(&mut cox_plugin(
        home.path(),
        cwd.path(),
        &["update", "demo", "--check"],
    ));
    assert!(check.contains(&format!("{old} -> ")), "{check}");
    assert!(check.contains("+ kv (new)"), "{check}");
    assert_eq!(
        current_of(home.path(), "demo"),
        old,
        "--check changes nothing"
    );
    assert_eq!(
        std::fs::read_dir(home.path().join("plugins/demo/versions"))
            .unwrap()
            .count(),
        1,
        "--check stages nothing"
    );

    cox_plugin(home.path(), cwd.path(), &["update", "demo"])
        .assert()
        .success();
    assert_eq!(
        current_of(home.path(), "demo"),
        old,
        "no re-grant, no switch"
    );

    let granted = stdout_ok(&mut cox_plugin(
        home.path(),
        cwd.path(),
        &["update", "demo", "--yes"],
    ));
    let new = current_of(home.path(), "demo");
    assert_ne!(new, old, "{granted}");
    let listed = list_json(home.path(), cwd.path());
    assert_eq!(listed["plugins"][0]["digest12"], new.as_str(), "{listed}");
    assert_eq!(listed["plugins"][0]["grant"], "granted", "{listed}");

    let back = stdout_ok(&mut cox_plugin(
        home.path(),
        cwd.path(),
        &["update", "demo", "--rollback"],
    ));
    assert_eq!(current_of(home.path(), "demo"), old, "{back}");
    assert!(!back.contains("waits for approval"), "{back}");
    let listed = list_json(home.path(), cwd.path());
    assert_eq!(listed["plugins"][0]["grant"], "granted", "{listed}");
    assert!(
        home.path()
            .join("plugins/demo/versions")
            .join(&new)
            .is_dir(),
        "the rolled-back-from version is kept as previous"
    );
}

/// PL§1b: headless never approves. With no terminal and no `--yes`, a new
/// digest is staged but `current` stays, and the warning names the
/// command to run. The same holds for a rollback whose grant was revoked.
#[test]
fn update_in_headless_keeps_current_and_warns() {
    let home = tempfile::tempdir().unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let old = installed(home.path(), cwd.path(), src.path());
    std::fs::write(src.path().join("plugin.wasm"), "rebuilt wasm bytes").unwrap();

    // Piped stdin could say "y"; headless must not read it as approval.
    let out =
        stdout_ok(cox_plugin(home.path(), cwd.path(), &["update", "demo"]).write_stdin("y\n"));
    assert!(
        out.contains("update for demo waits for approval: run `cox plugin update demo`"),
        "{out}"
    );
    assert_eq!(current_of(home.path(), "demo"), old);
    let listed = list_json(home.path(), cwd.path());
    assert_eq!(listed["plugins"][0]["grant"], "granted", "{listed}");

    // Revoke the old version's grant, then switch with `--yes`: a headless
    // rollback to the revoked digest must ask, so it keeps the new one.
    cox_plugin(home.path(), cwd.path(), &["disable", "demo"])
        .assert()
        .success();
    cox_plugin(home.path(), cwd.path(), &["update", "demo", "--yes"])
        .assert()
        .success();
    let new = current_of(home.path(), "demo");
    assert_ne!(new, old);
    let out = stdout_ok(&mut cox_plugin(
        home.path(),
        cwd.path(),
        &["update", "demo", "--rollback"],
    ));
    assert!(
        out.contains("run `cox plugin update demo --rollback`"),
        "{out}"
    );
    assert_eq!(current_of(home.path(), "demo"), new);
}
