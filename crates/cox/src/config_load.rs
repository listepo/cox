//! The binary's side of config loading (T32.16): `cox-config` owns the
//! layering, guards, provenance and editing, and this module keeps only what
//! needs `crates/cox`'s own types or crates `cox-config` must not depend on:
//! the CLI-flag layer built from clap's `Cli`, the `.claude/settings.json`
//! reader (`cox-ext`), the keymap for `cox-tui`, and the stderr warnings.
//! Everything else is re-exported from `cox_config::load` at this old path,
//! so callers keep writing `config_load::...`.

use std::collections::HashMap;
use std::path::Path;

use cox_ext::claude_settings;
use cox_protocol::CoreError;
use serde_json::Value as JsonValue;

#[cfg(test)]
pub(crate) use cox_config::load::temp_env;
pub use cox_config::load::{LoadedConfig, cox_home, find_git_root, home_dir};

use crate::cli::Cli;

/// Loads and layers config for `cwd` with `cli`'s flags and the Claude
/// settings import, and warns on stderr about each project-config guard
/// violation (`cox_config::load::load` itself prints nothing).
pub fn load(cwd: &Path, cli: &Cli) -> Result<LoadedConfig, CoreError> {
    let loaded = cox_config::load::load(cwd, &flag_overrides(cli), claude_layer)?;
    for v in &loaded.violations {
        eprintln!(
            "cox: warning: project config ignores {} = {} (guard); using {}",
            v.key, v.project_value, v.reverted_to
        );
    }
    Ok(loaded)
}

/// T25.5: `<cox_home>/keybindings.toml` over whatever `<claude_home>/
/// keybindings.json` binds that cox also has. Shared by the TUI session and
/// `doctor` so both see the same map; a missing file is just the defaults.
pub fn keymap(cox_home: &Path, claude_home: &Path) -> cox_tui::keymap::Loaded {
    let toml = std::fs::read_to_string(cox_home.join("keybindings.toml")).ok();
    let claude = claude_settings::keybindings(claude_home);
    let mut loaded = cox_tui::keymap::load(toml.as_deref(), &claude.bindings);
    loaded.warnings.extend(claude.notices);
    loaded
}

/// Maps a clap long-flag name (without the leading `--`) to the dotted
/// config key it conceptually overrides (plan.md §1.12: "every flag maps to
/// a config key"). A key under `runtime.` is not a real `Config` field — it
/// documents that the flag is a per-invocation parameter, not persisted
/// config (the prompt text, `--continue`, ...); `apply_flags` below only
/// writes the ones that *are* real `Config` fields into the flag layer.
pub fn flag_key_map() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        // Global (plan.md §1.12 "Global:" row).
        ("provider", "tiers.code.provider"),
        ("model", "tiers.code.model"),
        ("tier", "tiers.<tier>.model"),
        ("sandbox", "sandbox.mode"),
        ("permission-mode", "permissions.mode"),
        ("approve", "permissions.approval"),
        ("budget", "budget.session_usd"),
        ("profile", "core.profile"),
        ("cwd", "core.workspace_roots"),
        ("add-dir", "core.workspace_roots"),
        // T27.3: resolved once in `main` into `--cwd`/`--add-dir`.
        ("worktree", "runtime.worktree"),
        ("home", "core.home"),
        ("verbose", "core.log_level"),
        ("no-hooks", "hooks.enabled"),
        ("no-mcp", "mcp.enabled"),
        ("no-plugins", "plugins.enabled"),
        ("plain", "tui.screen_reader"),
        // `cox init` (plan.md T25.6) headlessly.
        ("force", "runtime.force"),
        ("prompt", "runtime.prompt"),
        ("output-format", "runtime.output_format"),
        ("max-turns", "core.max_turns"),
        ("allowed-tools", "permissions.allow"),
        ("answer", "runtime.answer"),
        ("continue", "runtime.continue"),
        ("resume", "runtime.resume"),
        ("deep", "runtime.deep"),
        // T27.6: `cox run --loop`/`--max-iterations` are invocation
        // parameters (the loop's own state), not persisted config.
        ("loop", "runtime.loop"),
        ("max-iterations", "runtime.max_iterations"),
        // `cox stats --project` (T28.2): a read-only scope flag, not config.
        ("project", "runtime.project"),
    ])
}

/// Sets `root[dotted.path] = value`, creating intermediate objects as needed.
fn set_dotted(root: &mut JsonValue, dotted: &str, value: JsonValue) {
    let parts: Vec<&str> = dotted.split('.').collect();
    set_path(root, &parts, value);
}

/// Walks `parts`, replacing any non-object on the way with an empty object.
fn set_path(node: &mut JsonValue, parts: &[&str], value: JsonValue) {
    if !node.is_object() {
        *node = JsonValue::Object(Default::default());
    }
    let JsonValue::Object(map) = node else {
        return;
    };
    match parts {
        [] => {}
        [leaf] => {
            map.insert((*leaf).to_string(), value);
        }
        [head, rest @ ..] => {
            let child = map
                .entry((*head).to_string())
                .or_insert_with(|| JsonValue::Object(Default::default()));
            set_path(child, rest, value);
        }
    }
}

/// Builds the sparse CLI-flag override tree (only fields the user actually
/// passed), applying only entries that name a real `Config` field — the
/// `runtime.*`-mapped flags in [`flag_key_map`] are invocation parameters,
/// not config, and are left for the caller (T2.x) to read off `Cli` directly.
///
/// Looks each key up in [`flag_key_map`] (rather than repeating the dotted
/// strings inline) so the map stays the single source of truth for "which
/// key does this flag override" — `every_flag_has_a_config_key` checks the
/// map is complete; this checks the map is actually load-bearing.
pub fn flag_overrides(cli: &Cli) -> JsonValue {
    let keys = flag_key_map();
    let mut root = JsonValue::Object(Default::default());
    if let Some(provider) = &cli.provider {
        set_dotted(
            &mut root,
            keys["provider"],
            JsonValue::from(provider.clone()),
        );
    }
    if let Some(model) = &cli.model {
        set_dotted(&mut root, keys["model"], JsonValue::from(model.clone()));
    }
    for pair in &cli.tier {
        if let Some((tier, model)) = pair.split_once('=') {
            set_dotted(
                &mut root,
                &format!("tiers.{tier}.model"),
                JsonValue::from(model.to_string()),
            );
        }
    }
    if let Some(sandbox) = &cli.sandbox {
        set_dotted(&mut root, keys["sandbox"], JsonValue::from(sandbox.clone()));
    }
    if let Some(mode) = &cli.permission_mode {
        set_dotted(
            &mut root,
            keys["permission-mode"],
            JsonValue::from(mode.clone()),
        );
    }
    if let Some(approve) = &cli.approve {
        set_dotted(&mut root, keys["approve"], JsonValue::from(approve.clone()));
    }
    if let Some(budget) = cli.budget {
        set_dotted(&mut root, keys["budget"], JsonValue::from(budget));
    }
    if let Some(profile) = &cli.profile {
        set_dotted(&mut root, keys["profile"], JsonValue::from(profile.clone()));
    }
    if !cli.add_dir.is_empty() || cli.cwd.is_some() {
        let mut roots: Vec<JsonValue> = cli
            .add_dir
            .iter()
            .map(|p| JsonValue::from(p.display().to_string()))
            .collect();
        if let Some(cwd) = &cli.cwd {
            roots.push(JsonValue::from(cwd.display().to_string()));
        }
        set_dotted(&mut root, keys["add-dir"], JsonValue::Array(roots));
    }
    if let Some(home) = &cli.home {
        set_dotted(
            &mut root,
            keys["home"],
            JsonValue::from(home.display().to_string()),
        );
    }
    if cli.verbose > 0 {
        let level = if cli.verbose >= 2 { "trace" } else { "debug" };
        set_dotted(&mut root, keys["verbose"], JsonValue::from(level));
    }
    if cli.no_hooks {
        set_dotted(&mut root, keys["no-hooks"], JsonValue::from(false));
    }
    if cli.no_mcp {
        set_dotted(&mut root, keys["no-mcp"], JsonValue::from(false));
    }
    if cli.no_plugins {
        set_dotted(&mut root, keys["no-plugins"], JsonValue::from(false));
    }
    if cli.plain {
        set_dotted(&mut root, keys["plain"], JsonValue::from(true));
    }
    root
}

/// The imported `.claude/settings.json` files for `cwd`, if any exist.
/// Broken files are warned about and skipped (D14).
fn claude_layer(cwd: &Path) -> Option<JsonValue> {
    let claude_home = home_dir().join(".claude");
    let project = find_git_root(cwd);
    let paths = claude_settings::paths(Some(&claude_home), project.as_deref());
    let settings = claude_settings::load(&paths);
    for notice in &settings.notices {
        eprintln!("cox: warning: {notice}");
    }
    (!settings.is_empty()).then(|| settings.to_layer())
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn config_every_flag_has_a_config_key() {
        let map = flag_key_map();
        let excluded = ["help", "version", "json", "verbose"];
        let mut missing = Vec::new();

        let cmd = Cli::command();
        for arg in cmd.get_arguments() {
            if arg.is_positional() {
                continue;
            }
            if let Some(long) = arg.get_long()
                && !excluded.contains(&long)
                && !map.contains_key(long)
            {
                missing.push(long.to_string());
            }
        }
        let run = cmd.find_subcommand("run").expect("run subcommand exists");
        for arg in run.get_arguments() {
            if arg.is_positional() {
                continue;
            }
            if let Some(long) = arg.get_long()
                && !excluded.contains(&long)
                && !map.contains_key(long)
            {
                missing.push(long.to_string());
            }
        }
        // T25.6: `cox init --force` registers its flag like `run`'s own.
        let init = cmd.find_subcommand("init").expect("init subcommand exists");
        for arg in init.get_arguments() {
            if arg.is_positional() {
                continue;
            }
            if let Some(long) = arg.get_long()
                && !excluded.contains(&long)
                && !map.contains_key(long)
            {
                missing.push(long.to_string());
            }
        }
        assert!(
            missing.is_empty(),
            "flags missing a config-key mapping: {missing:?}"
        );
    }
}

#[cfg(test)]
mod claude_settings_tests {
    use clap::Parser;
    use cox_core::permission::{Engine, Outcome};
    use cox_protocol::Config;
    use cox_protocol::ids::CallId;
    use cox_protocol::types::{ApprovalPolicy, PermissionMode, Risk, SandboxMode, ToolCall};
    use std::fs;
    use tempfile::tempdir;

    use super::*;
    use crate::cli::Cli;

    fn rm_call() -> ToolCall {
        ToolCall {
            id: CallId::new(),
            name: "bash".into(),
            input: serde_json::json!({ "command": "rm -rf build" }),
            risk: Risk::Exec,
            subject: "rm -rf build".into(),
            segments: None,
        }
    }

    fn deny_of(cfg: &Config, cwd: &Path) -> Option<String> {
        let engine = Engine::compile(&cfg.permissions, None, cwd).expect("engine");
        match engine.decide(
            &rm_call(),
            PermissionMode::Auto,
            ApprovalPolicy::Never,
            SandboxMode::WorkspaceWrite,
            &[],
        ) {
            Outcome::Deny { reason, .. } => Some(reason),
            _ => None,
        }
    }

    /// T7.5 step 4: the fixture yields the same decision as native rules,
    /// adds to (not replaces) the project's own list, and is labelled.
    #[test]
    fn config_claude_settings_import_matches_native_rules() {
        let home = tempdir().expect("tempdir");
        let git_root = tempdir().expect("tempdir");
        fs::create_dir_all(git_root.path().join(".git")).expect("mkdir .git");
        fs::create_dir_all(git_root.path().join(".cox")).expect("mkdir .cox");
        fs::create_dir_all(git_root.path().join(".claude")).expect("mkdir .claude");
        fs::write(
            git_root.path().join(".cox/config.toml"),
            "[permissions]\ndeny = [\"Bash(curl *)\"]\n",
        )
        .expect("write project config");
        fs::write(
            git_root.path().join(".claude/settings.json"),
            r#"{"permissions":{"deny":["Bash(rm -rf *)"]},"hooks":{"Stop":[{"hooks":[{"type":"command","command":"say done"}]}]}}"#,
        )
        .expect("write settings");
        let native_dir = tempdir().expect("tempdir");
        fs::create_dir_all(native_dir.path().join(".git")).expect("mkdir .git");
        fs::create_dir_all(native_dir.path().join(".cox")).expect("mkdir .cox");
        fs::write(
            native_dir.path().join(".cox/config.toml"),
            "[permissions]\ndeny = [\"Bash(curl *)\", \"Bash(rm -rf *)\"]\n",
        )
        .expect("write native config");

        temp_env(
            &[
                ("COX_HOME", Some(home.path().to_str().unwrap())),
                ("HOME", Some(home.path().to_str().unwrap())),
            ],
            || {
                let cli = Cli::parse_from(["cox"]);
                let imported = load(git_root.path(), &cli).expect("load imported");
                let native = load(native_dir.path(), &cli).expect("load native");
                assert_eq!(
                    imported.config.permissions.deny,
                    native.config.permissions.deny
                );
                let denied = deny_of(&imported.config, git_root.path());
                assert!(denied.is_some(), "rm must be denied");
                assert_eq!(denied, deny_of(&native.config, native_dir.path()));
                // A list both layers feed keeps the first layer's label
                // (figment `adjoin`); a key only Claude sets is labelled.
                assert_eq!(imported.source_of("permissions.deny"), "project");
                assert_eq!(imported.source_of("hooks.Stop"), "claude-settings");
                assert_eq!(native.source_of("permissions.deny"), "project");
                assert_eq!(imported.config.hooks.events["Stop"][0].command, "say done");

                // The import is opt-out.
                fs::write(
                    git_root.path().join(".cox/config.toml"),
                    "[permissions]\ndeny = [\"Bash(curl *)\"]\nimport_claude_settings = false\n",
                )
                .expect("rewrite project config");
                let off = load(git_root.path(), &cli).expect("load opt-out");
                assert_eq!(off.config.permissions.deny, ["Bash(curl *)"]);
                assert!(off.config.hooks.events.is_empty());
            },
        );
    }
}
