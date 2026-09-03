//! Config loading and provenance (plan.md §1.6/D13/T0.3): layers
//! `config/default.toml` < `~/.cox/config.toml` < `<git root>/.cox/config.toml`
//! < `COX_<SECTION>_<KEY>` env vars < CLI flags via `figment`, enforces the
//! project-config guard list, and records which layer last set each key so
//! `cox config show --sources` can print it.
//!
//! `.claude/settings.json` import is T7.5 (out of scope here, plan.md §1.6
//! "Out of scope").

use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

use cox_protocol::config::DEFAULT_CONFIG_TOML;
use cox_protocol::{Config, CoreError, PermissionMode, SandboxMode};
use figment::providers::{Env, Format, Serialized, Toml};
use figment::value::{Dict, Map as FigMap};
use figment::{Figment, Metadata, Profile, Provider};
use serde_json::Value as JsonValue;

use crate::cli::Cli;

/// A `Provider` adapter that reports a fixed layer name as its `Metadata`,
/// so `Figment::find_metadata` tells us which layer produced a value —
/// `figment`'s own provider names (`"TOML file"`, `"environment
/// variable(s)"`, ...) aren't the `default|user|project|env|flag` labels
/// `cox config show --sources` needs.
struct Named<P> {
    name: &'static str,
    inner: P,
}

impl<P: Provider> Provider for Named<P> {
    fn metadata(&self) -> Metadata {
        Metadata::named(self.name)
    }

    fn data(&self) -> Result<FigMap<Profile, Dict>, figment::Error> {
        self.inner.data()
    }

    fn profile(&self) -> Option<Profile> {
        self.inner.profile()
    }
}

fn named<P: Provider>(name: &'static str, inner: P) -> Named<P> {
    Named { name, inner }
}

/// Where `cox` looks for its home directory. `COX_HOME` overrides `~/.cox`
/// (plan.md §1.6 `core.home` comment) for the whole `~/.cox` tree, not just
/// the `core.home` config value — this is what every task's `COX_HOME=...`
/// scratch-tree invocation relies on.
pub fn cox_home() -> PathBuf {
    match env::var_os("COX_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => home_dir().join(".cox"),
    }
}

pub fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The user config file: `<cox_home>/config.toml`.
pub fn user_config_path() -> PathBuf {
    cox_home().join("config.toml")
}

/// Walks up from `start` looking for a `.git` entry (a directory for a
/// normal clone, a file for a worktree), returning the first ancestor that
/// has one.
pub fn find_git_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    loop {
        if dir.join(".git").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// The project config file, if `cwd` is inside a git checkout:
/// `<git root>/.cox/config.toml`.
pub fn project_config_path(cwd: &Path) -> Option<PathBuf> {
    find_git_root(cwd).map(|root| root.join(".cox").join("config.toml"))
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
        ("cwd", "core.workspace_roots"),
        ("add-dir", "core.workspace_roots"),
        ("home", "core.home"),
        ("verbose", "core.log_level"),
        ("no-hooks", "hooks.enabled"),
        ("no-mcp", "mcp.enabled"),
        // `cox run` (plan.md §1.12 `cox run` row).
        ("prompt", "runtime.prompt"),
        ("output-format", "runtime.output_format"),
        ("max-turns", "core.max_turns"),
        ("allowed-tools", "permissions.allow"),
        ("answer", "runtime.answer"),
        ("continue", "runtime.continue"),
        ("resume", "runtime.resume"),
        ("deep", "runtime.deep"),
    ])
}

/// Sets `root[dotted.path] = value`, creating intermediate objects as needed.
fn set_dotted(root: &mut JsonValue, dotted: &str, value: JsonValue) {
    let mut cur = root;
    let parts: Vec<&str> = dotted.split('.').collect();
    for part in &parts[..parts.len() - 1] {
        if !cur.is_object() {
            *cur = JsonValue::Object(Default::default());
        }
        cur = cur
            .as_object_mut()
            .expect("just ensured object")
            .entry(part.to_string())
            .or_insert_with(|| JsonValue::Object(Default::default()));
    }
    if !cur.is_object() {
        *cur = JsonValue::Object(Default::default());
    }
    cur.as_object_mut()
        .expect("just ensured object")
        .insert(parts[parts.len() - 1].to_string(), value);
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
    root
}

/// One project-config guard violation (plan.md §1.6): the project layer set
/// a key it isn't allowed to, so the loader reverted it and is reporting why.
#[derive(Debug, Clone, PartialEq)]
pub struct GuardViolation {
    /// The dotted key the project layer tried to set.
    pub key: &'static str,
    /// What the project layer set it to.
    pub project_value: String,
    /// What it was reverted to (the value without the project layer).
    pub reverted_to: String,
}

/// Reverts any guarded key `full` set relative to `without_project` back to
/// `without_project`'s value, returning what it reverted (plan.md §1.6:
/// "budget.* may not be raised above user/default, permissions.mode =
/// \"bypass\", sandbox.mode = \"danger-full-access\" or tiers.think.confirm
/// = false are ignored from the project file with a warning to stderr").
fn apply_project_guards(full: &mut Config, without_project: &Config) -> Vec<GuardViolation> {
    let mut violations = Vec::new();

    let mut check_budget_raise = |key, full_v: &mut f64, base_v: f64| {
        if *full_v > base_v {
            violations.push(GuardViolation {
                key,
                project_value: full_v.to_string(),
                reverted_to: base_v.to_string(),
            });
            *full_v = base_v;
        }
    };
    check_budget_raise(
        "budget.session_usd",
        &mut full.budget.session_usd,
        without_project.budget.session_usd,
    );
    check_budget_raise(
        "budget.monthly_usd",
        &mut full.budget.monthly_usd,
        without_project.budget.monthly_usd,
    );
    check_budget_raise(
        "budget.warn_at",
        &mut full.budget.warn_at,
        without_project.budget.warn_at,
    );

    if full.permissions.mode == PermissionMode::Bypass
        && without_project.permissions.mode != PermissionMode::Bypass
    {
        violations.push(GuardViolation {
            key: "permissions.mode",
            project_value: "bypass".to_string(),
            reverted_to: format!("{:?}", without_project.permissions.mode).to_lowercase(),
        });
        full.permissions.mode = without_project.permissions.mode;
    }

    if full.sandbox.mode == SandboxMode::DangerFullAccess
        && without_project.sandbox.mode != SandboxMode::DangerFullAccess
    {
        violations.push(GuardViolation {
            key: "sandbox.mode",
            project_value: "danger-full-access".to_string(),
            reverted_to: format!("{:?}", without_project.sandbox.mode).to_lowercase(),
        });
        full.sandbox.mode = without_project.sandbox.mode;
    }

    if !full.tiers.think.confirm && without_project.tiers.think.confirm {
        violations.push(GuardViolation {
            key: "tiers.think.confirm",
            project_value: "false".to_string(),
            reverted_to: "true".to_string(),
        });
        full.tiers.think.confirm = without_project.tiers.think.confirm;
    }

    violations
}

/// Dotted keys the project-config guard list can revert (plan.md §1.6);
/// used only to pick which figment (with or without the project layer) a
/// reverted key's provenance is looked up in.
const GUARDED_KEYS: [&str; 6] = [
    "budget.session_usd",
    "budget.monthly_usd",
    "budget.warn_at",
    "permissions.mode",
    "sandbox.mode",
    "tiers.think.confirm",
];

/// The result of [`load`]: the effective, guard-corrected `Config`, plus
/// enough of the layered figments to answer `source_of` for `cox config show
/// --sources`.
pub struct LoadedConfig {
    /// The effective configuration, after the project-config guard list.
    pub config: Config,
    /// Guard violations found in the project layer, if any (already applied
    /// to `config`; report these to stderr and/or a future `Notice`).
    pub violations: Vec<GuardViolation>,
    full_fig: Figment,
    pre_project_fig: Figment,
}

impl LoadedConfig {
    /// Which layer last set `key` (`default|user|project|env|flag`), for
    /// `cox config show --sources`. A key the project guard list reverted
    /// reports the layer its *effective* (post-revert) value came from.
    pub fn source_of(&self, key: &str) -> &'static str {
        let reverted = GUARDED_KEYS.contains(&key) && self.violations.iter().any(|v| v.key == key);
        let fig = if reverted {
            &self.pre_project_fig
        } else {
            &self.full_fig
        };
        match fig.find_metadata(key).map(|m| m.name.as_ref()) {
            Some("default") => "default",
            Some("user") => "user",
            Some("project") => "project",
            Some("env") => "env",
            Some("flag") => "flag",
            _ => "default",
        }
    }
}

fn build_figment(user_path: &Path, project_path: Option<&Path>, flags: &JsonValue) -> Figment {
    // `Toml::file` (not `file_exact`): both paths here are always absolute
    // (`cox_home()`/git-root-derived), and for an absolute path `Data::file`
    // checks existence directly rather than searching parent directories —
    // it just also treats "missing" as "empty" instead of an IO error,
    // which `file_exact` does not (it always attempts to read the path).
    let mut fig = Figment::new()
        .merge(named("default", Toml::string(DEFAULT_CONFIG_TOML)))
        .merge(named("user", Toml::file(user_path)));
    if let Some(project_path) = project_path {
        fig = fig.merge(named("project", Toml::file(project_path)));
    }
    fig = fig.merge(named(
        "env",
        // `COX_PROVIDER` / `COX_SCENARIO` / `COX_CASSETTES` select a test-double
        // provider (cox-provider::from_env), not config keys.
        Env::prefixed("COX_")
            .ignore(&["home", "provider", "scenario", "cassettes"])
            .split("_"),
    ));
    if let Ok(home) = env::var("COX_HOME") {
        fig = fig.merge(named("env", Serialized::default("core.home", home)));
    }
    fig.merge(named("flag", Serialized::defaults(flags)))
}

fn to_core_error(err: figment::Error) -> CoreError {
    CoreError::Config {
        key: err.path.join("."),
        message: err.to_string(),
    }
}

/// Loads and layers config (plan.md §1.6/D13), applies the project guard
/// list, and returns the effective config plus provenance.
pub fn load(cwd: &Path, cli: &Cli) -> Result<LoadedConfig, CoreError> {
    let user_path = user_config_path();
    let project_path = project_config_path(cwd);
    let flags = flag_overrides(cli);

    let full_fig = build_figment(&user_path, project_path.as_deref(), &flags);
    let pre_project_fig = build_figment(&user_path, None, &flags);

    let full_cfg: Config = full_fig.extract().map_err(to_core_error)?;
    let without_project_cfg: Config = pre_project_fig.extract().map_err(to_core_error)?;

    let mut config = full_cfg;
    let violations = apply_project_guards(&mut config, &without_project_cfg);
    for v in &violations {
        eprintln!(
            "cox: warning: project config ignores {} = {} (guard); using {}",
            v.key, v.project_value, v.reverted_to
        );
    }

    Ok(LoadedConfig {
        config,
        violations,
        full_fig,
        pre_project_fig,
    })
}

/// Serializes every test in this crate that mutates process-wide env vars
/// (`COX_HOME`, `COX_*`) — `cargo test` runs a binary's tests concurrently
/// by default, and env vars are global process state, so without this lock
/// `config_env_overrides_project`, `config_project_cannot_raise_budget` and
/// `config_cmd::tests::config_set_*` would race each other.
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
/// Sets env vars for the duration of `f`, restoring the previous value
/// (or absence) afterwards, holding [`ENV_LOCK`] throughout so this
/// doesn't race other env-mutating tests in the crate.
pub(crate) fn temp_env(vars: &[(&str, Option<&str>)], f: impl FnOnce()) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let previous: Vec<(String, Option<String>)> = vars
        .iter()
        .map(|(k, _)| (k.to_string(), env::var(k).ok()))
        .collect();
    for (k, v) in vars {
        match v {
            Some(v) => unsafe { env::set_var(k, v) },
            None => unsafe { env::remove_var(k) },
        }
    }
    f();
    for (k, v) in previous {
        match v {
            Some(v) => unsafe { env::set_var(&k, v) },
            None => unsafe { env::remove_var(&k) },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use clap::{CommandFactory, Parser};
    use tempfile::tempdir;

    use super::*;
    use crate::cli::Cli;

    fn parse(args: &[&str]) -> Cli {
        let mut full = vec!["cox"];
        full.extend_from_slice(args);
        Cli::parse_from(full)
    }

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
        assert!(
            missing.is_empty(),
            "flags missing a config-key mapping: {missing:?}"
        );
    }

    #[test]
    fn config_defaults_parse() {
        // The embedded default.toml alone, through the same struct tree the
        // full loader uses, with no unknown fields.
        let fig = Figment::new().merge(Toml::string(DEFAULT_CONFIG_TOML));
        let cfg: Config = fig.extract().expect("default.toml deserializes cleanly");
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn config_project_cannot_raise_budget() {
        let home = tempdir().expect("tempdir");
        let git_root = tempdir().expect("tempdir");
        fs::create_dir_all(git_root.path().join(".git")).expect("mkdir .git");
        fs::create_dir_all(git_root.path().join(".cox")).expect("mkdir .cox");
        fs::write(
            git_root.path().join(".cox/config.toml"),
            "[budget]\nsession_usd = 999.0\n",
        )
        .expect("write project config");

        // SAFETY-of-intent: tests run single-threaded within this process
        // for env-var mutation (see `config_env_overrides_project`, which
        // documents why this crate accepts that constraint for T0.3).
        temp_env(&[("COX_HOME", Some(home.path().to_str().unwrap()))], || {
            let cli = parse(&[]);
            let loaded = load(git_root.path(), &cli).expect("load succeeds");
            assert_eq!(
                loaded.config.budget.session_usd, 5.0,
                "raise must be ignored"
            );
            assert!(
                loaded
                    .violations
                    .iter()
                    .any(|v| v.key == "budget.session_usd")
            );
            assert_eq!(loaded.source_of("budget.session_usd"), "default");
        });
    }

    #[test]
    fn config_env_overrides_project() {
        let home = tempdir().expect("tempdir");
        let git_root = tempdir().expect("tempdir");
        fs::create_dir_all(git_root.path().join(".git")).expect("mkdir .git");
        fs::create_dir_all(git_root.path().join(".cox")).expect("mkdir .cox");
        fs::write(
            git_root.path().join(".cox/config.toml"),
            "[tiers.code]\nmodel = \"project-model\"\n",
        )
        .expect("write project config");

        temp_env(
            &[
                ("COX_HOME", Some(home.path().to_str().unwrap())),
                ("COX_TIERS_CODE_MODEL", Some("env-model")),
            ],
            || {
                let cli = parse(&[]);
                let loaded = load(git_root.path(), &cli).expect("load succeeds");
                assert_eq!(loaded.config.tiers.code.model, "env-model");
                assert_eq!(loaded.source_of("tiers.code.model"), "env");
            },
        );
    }
}
