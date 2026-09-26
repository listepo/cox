//! Config loading and provenance (plan.md §1.6/D13/T0.3): layers
//! `config/default.toml` < `~/.cox/config.toml` < `<git root>/.cox/config.toml`
//! < `COX_<SECTION>_<KEY>` env vars < CLI flags via `figment`, enforces the
//! project-config guard list, and records which layer last set each key so
//! `cox config show --sources` can print it.
//!
//! `.claude/settings.json` (T7.5) is one more layer above project config (plan.md §1.6
//! "Out of scope").
//!
//! T32.16: moved here from `crates/cox`. The two inputs that come from the
//! binary's side stay there and are passed in: the CLI-flag layer (built
//! from clap's `Cli` by `crates/cox`'s `config_load::flag_overrides`) and the
//! `.claude/settings.json` reader (`cox-ext`), so this crate depends only on
//! `cox-protocol`. It prints nothing: the caller reports the guard
//! violations `load` returns.

use std::env;
use std::path::{Path, PathBuf};

use cox_protocol::config::DEFAULT_CONFIG_TOML;
use cox_protocol::{Config, CoreError, PermissionMode, SandboxMode};
use figment::providers::{Env, Format, Serialized, Toml};
use figment::value::{Dict, Map as FigMap};
use figment::{Figment, Metadata, Profile, Provider};
use serde_json::Value as JsonValue;

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

    // T34.2 review: a cost/concurrency guard like the budget keys above —
    // a project layer must not be able to widen how many `agent` tasks a
    // session lets run at once.
    if full.core.max_concurrent_subagents > without_project.core.max_concurrent_subagents {
        violations.push(GuardViolation {
            key: "core.max_concurrent_subagents",
            project_value: full.core.max_concurrent_subagents.to_string(),
            reverted_to: without_project.core.max_concurrent_subagents.to_string(),
        });
        full.core.max_concurrent_subagents = without_project.core.max_concurrent_subagents;
    }

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

    // T33.6 (PL§1, D14): a repository must not switch plugins back on
    // after the user turned them off; its own plugins still need a grant,
    // but the user's off switch has to hold regardless.
    if full.plugins.enabled && !without_project.plugins.enabled {
        violations.push(GuardViolation {
            key: "plugins.enabled",
            project_value: "true".to_string(),
            reverted_to: "false".to_string(),
        });
        full.plugins.enabled = false;
    }

    if !full.tiers.think.confirm && without_project.tiers.think.confirm {
        violations.push(GuardViolation {
            key: "tiers.think.confirm",
            project_value: "false".to_string(),
            reverted_to: "true".to_string(),
        });
        full.tiers.think.confirm = without_project.tiers.think.confirm;
    }

    // T33.42: the sandbox is exactly what contains a command a cloned
    // repository chose, so the project layer must never be the reason a
    // server's effective `sandbox` is `false` — an existing server or one
    // it adds outright, same difference: only user config (and env/flags,
    // `without_project`'s other layers) may opt a server out. A server is
    // reverted unless `without_project` alone already yields `sandbox =
    // false` for that name, i.e. a non-project layer opted it out on its
    // own.
    let weakened: Vec<String> = full
        .mcp
        .servers
        .iter()
        .filter(|(name, server)| {
            !server.sandbox
                && !without_project
                    .mcp
                    .servers
                    .get(name.as_str())
                    .is_some_and(|s| !s.sandbox)
        })
        .map(|(name, _)| name.clone())
        .collect();
    if !weakened.is_empty() {
        violations.push(GuardViolation {
            key: "mcp.servers.*.sandbox",
            project_value: format!("false ({})", weakened.join(", ")),
            reverted_to: "true".to_string(),
        });
        for name in &weakened {
            if let Some(server) = full.mcp.servers.get_mut(name) {
                server.sandbox = true;
            }
        }
    }

    violations
}

/// Dotted keys the project-config guard list can revert (plan.md §1.6);
/// used only to pick which figment (with or without the project layer) a
/// reverted key's provenance is looked up in.
const GUARDED_KEYS: [&str; 9] = [
    "budget.session_usd",
    "budget.monthly_usd",
    "budget.warn_at",
    "core.max_concurrent_subagents",
    "mcp.servers.*.sandbox",
    "permissions.mode",
    "plugins.enabled",
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
            Some("claude-settings") => "claude-settings",
            _ => "default",
        }
    }
}

fn build_figment(
    user_path: &Path,
    project_path: Option<&Path>,
    claude: Option<&JsonValue>,
    flags: &JsonValue,
) -> Figment {
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
    if let Some(claude) = claude {
        // `adjoin`, not `merge`: imported rules and hooks add to the `.cox`
        // lists rather than replace them (D13: imported, read-only).
        fig = fig.adjoin(named(
            "claude-settings",
            Serialized::defaults(claude.clone()),
        ));
    }
    let key_tree = default_key_tree();
    fig = fig.merge(named(
        "env",
        // `COX_PROVIDER` / `COX_SCENARIO` / `COX_CASSETTES` select a test-double
        // provider (cox-provider::from_env), not config keys. `COX_HOME`
        // overrides `core.home` directly below. `COX_EXPECT_SANDBOX` pins the
        // backend a sandbox test asserts (CI sets it globally), so it must
        // not leak into the config tree as `expect.sandbox` either.
        // `COX_PLAIN` and `COX_AX_STARTUP_QUIET_MS` are read by the plain
        // surface (T29.1) itself. `COX_KEYRING` switches the OS keyring
        // off (A49). The ignore list matches pre-split keys
        // (`EXPECT_SANDBOX`, not dotted): it runs before the `map` below.
        Env::prefixed("COX_")
            .ignore(&[
                "home",
                "provider",
                "scenario",
                "cassettes",
                "expect_sandbox",
                "plain",
                "ax_startup_quiet_ms",
                "keyring",
            ])
            .map(move |name| env_key(&key_tree, name.as_str()).into()),
    ));
    if let Ok(home) = env::var("COX_HOME") {
        fig = fig.merge(named("env", Serialized::default("core.home", home)));
    }
    fig.merge(named("flag", Serialized::defaults(flags)))
}

/// The table/key tree of the embedded defaults, which [`env_key`] resolves
/// `COX_*` names against. Empty only if `default.toml` failed to parse, which
/// its own tests rule out; `env_key` then degrades to plain `_` splitting.
fn default_key_tree() -> Dict {
    Figment::from(Toml::string(DEFAULT_CONFIG_TOML))
        .extract()
        .unwrap_or_default()
}

/// Maps a `COX_`-stripped env name to a dotted key. Splitting on every `_`
/// would turn `TUI_SHOW_THINKING` into `tui.show.thinking`, so each level
/// takes the longest known name that is the whole rest or a prefix of it
/// followed by `_`. Whatever no known name covers (a user-defined tier, a
/// typo) is split on `_` as before, so it still lands where it did.
fn env_key(tree: &Dict, name: &str) -> String {
    let name = name.to_ascii_lowercase();
    let mut rest = name.as_str();
    let mut table = Some(tree);
    let mut parts: Vec<&str> = Vec::new();
    while let Some(dict) = table {
        let hit = dict
            .iter()
            .filter(|(key, _)| {
                rest.strip_prefix(key.as_str())
                    .is_some_and(|tail| tail.is_empty() || tail.starts_with('_'))
            })
            .max_by_key(|(key, _)| key.len());
        let Some((key, value)) = hit else { break };
        parts.push(key);
        rest = rest[key.len()..].strip_prefix('_').unwrap_or("");
        table = value.as_dict();
    }
    if !rest.is_empty() {
        parts.extend(rest.split('_'));
    }
    parts.join(".")
}

fn to_core_error(err: figment::Error) -> CoreError {
    CoreError::Config {
        key: err.path.join("."),
        message: err.to_string(),
    }
}

/// Loads and layers config (plan.md §1.6/D13), applies the project guard
/// list, and returns the effective config plus provenance.
///
/// `flags` is the sparse CLI-flag override tree; `claude_layer` reads the
/// imported `.claude/settings.json` layer for `cwd`, and is called only when
/// the `.cox` layers leave `permissions.import_claude_settings` on. The
/// caller reports `violations` (T32.16: no terminal output in this crate).
pub fn load(
    cwd: &Path,
    flags: &JsonValue,
    claude_layer: impl FnOnce(&Path) -> Option<JsonValue>,
) -> Result<LoadedConfig, CoreError> {
    let user_path = user_config_path();
    let project_path = project_config_path(cwd);

    // Whether to import is itself a config key, so the `.cox` layers decide
    // before the Claude layer exists.
    let native: Config = build_figment(&user_path, project_path.as_deref(), None, flags)
        .extract()
        .map_err(to_core_error)?;
    let claude = native
        .permissions
        .import_claude_settings
        .then(|| claude_layer(cwd))
        .flatten();
    let full_fig = build_figment(&user_path, project_path.as_deref(), claude.as_ref(), flags);
    let pre_project_fig = build_figment(&user_path, None, claude.as_ref(), flags);

    let full_cfg: Config = full_fig.extract().map_err(to_core_error)?;
    let without_project_cfg: Config = pre_project_fig.extract().map_err(to_core_error)?;

    let mut config = full_cfg;
    let violations = apply_project_guards(&mut config, &without_project_cfg);

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
/// `cmd::tests::config_set_*` would race each other. The
/// `test-util` feature (T32.16) exposes it to `crates/cox`'s own
/// env-mutating tests, so there is one lock, not a copy per crate.
#[cfg(any(test, feature = "test-util"))]
pub static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(any(test, feature = "test-util"))]
/// Sets env vars for the duration of `f`, restoring the previous value
/// (or absence) afterwards, holding [`ENV_LOCK`] throughout so this
/// doesn't race other env-mutating tests in the crate.
pub fn temp_env(vars: &[(&str, Option<&str>)], f: impl FnOnce()) {
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

    use tempfile::tempdir;

    use super::*;

    /// `load` with no CLI flags and no `.claude/settings.json` layer: the
    /// flag layer and the Claude import are the caller's (T32.16).
    fn load_plain(cwd: &Path) -> Result<LoadedConfig, CoreError> {
        load(cwd, &JsonValue::Object(Default::default()), |_| None)
    }

    #[test]
    fn config_defaults_parse() {
        // The embedded default.toml alone, through the same struct tree the
        // full loader uses, with no unknown fields. `Config::default()` is
        // the neutral fallback *beneath* this layer (empty model lists, no
        // custom providers), so this spot-checks the registry it cannot
        // carry instead of asserting full equality with it.
        let fig = Figment::new().merge(Toml::string(DEFAULT_CONFIG_TOML));
        let cfg: Config = fig.extract().expect("default.toml deserializes cleanly");
        assert_eq!(cfg.tiers.code.model, "claude-sonnet-5");
        assert!(cfg.providers.custom.contains_key("deepseek"));
        assert!(!cfg.providers.anthropic.models.is_empty());
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
            let loaded = load_plain(git_root.path()).expect("load succeeds");
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

    /// T34.2 review: `core.max_concurrent_subagents` is a cost/concurrency
    /// guard like the budget keys — a project's `.cox/config.toml` must not
    /// be able to raise it, the same treatment `config_project_cannot_raise_budget`
    /// proves for `budget.session_usd`.
    #[test]
    fn config_project_cannot_raise_max_concurrent_subagents() {
        let home = tempdir().expect("tempdir");
        let git_root = tempdir().expect("tempdir");
        fs::create_dir_all(git_root.path().join(".git")).expect("mkdir .git");
        fs::create_dir_all(git_root.path().join(".cox")).expect("mkdir .cox");
        fs::write(
            git_root.path().join(".cox/config.toml"),
            "[core]\nmax_concurrent_subagents = 999\n",
        )
        .expect("write project config");

        temp_env(&[("COX_HOME", Some(home.path().to_str().unwrap()))], || {
            let loaded = load_plain(git_root.path()).expect("load succeeds");
            assert_eq!(
                loaded.config.core.max_concurrent_subagents, 8,
                "raise must be ignored"
            );
            assert!(
                loaded
                    .violations
                    .iter()
                    .any(|v| v.key == "core.max_concurrent_subagents")
            );
            assert_eq!(loaded.source_of("core.max_concurrent_subagents"), "default");
        });
    }

    /// T33.6: the user's `plugins.enabled = false` holds against a
    /// repository's own `.cox/config.toml` turning plugins back on.
    #[test]
    fn config_project_cannot_turn_plugins_on() {
        let home = tempdir().expect("tempdir");
        let git_root = tempdir().expect("tempdir");
        fs::write(
            home.path().join("config.toml"),
            "[plugins]\nenabled = false\n",
        )
        .expect("write user config");
        fs::create_dir_all(git_root.path().join(".git")).expect("mkdir .git");
        fs::create_dir_all(git_root.path().join(".cox")).expect("mkdir .cox");
        fs::write(
            git_root.path().join(".cox/config.toml"),
            "[plugins]\nenabled = true\n",
        )
        .expect("write project config");

        temp_env(&[("COX_HOME", Some(home.path().to_str().unwrap()))], || {
            let loaded = load_plain(git_root.path()).expect("load succeeds");
            assert!(!loaded.config.plugins.enabled, "turn-on must be ignored");
            assert!(loaded.violations.iter().any(|v| v.key == "plugins.enabled"));
            assert_eq!(loaded.source_of("plugins.enabled"), "user");
        });
    }

    /// T33.9: a `[plugins.<id>]` table flattens into `plugins.entries`
    /// beside the fixed `enabled` key, the `HooksConfig` pattern.
    #[test]
    fn plugin_table_flattens_into_entries() {
        let home = tempdir().expect("tempdir");
        let cwd = tempdir().expect("tempdir");
        fs::write(
            home.path().join("config.toml"),
            "[plugins]\nenabled = true\n\n[plugins.jev]\nroute = \"cheap\"\nlimit = 3\n",
        )
        .expect("write user config");

        temp_env(&[("COX_HOME", Some(home.path().to_str().unwrap()))], || {
            let plugins = load_plain(cwd.path())
                .expect("load succeeds")
                .config
                .plugins;
            assert!(plugins.enabled);
            assert_eq!(
                plugins.entries.get("jev"),
                Some(&serde_json::json!({ "route": "cheap", "limit": 3 }))
            );
            assert!(!plugins.entries.contains_key("enabled"));
        });
    }

    /// T33.42: a project layer must not be able to flip an existing,
    /// user-configured server's `sandbox` off — that would silently drop
    /// the wrap on a server the user already trusted as sandboxed, without
    /// touching its command, so the change is easy to miss in review.
    #[test]
    fn config_project_cannot_disable_an_mcp_server_sandbox() {
        let home = tempdir().expect("tempdir");
        let git_root = tempdir().expect("tempdir");
        fs::write(
            home.path().join("config.toml"),
            "[mcp.servers.gh]\ncommand = \"gh-mcp\"\n\n[mcp.servers.opt-out]\ncommand = \"y\"\nsandbox = false\n",
        )
        .expect("write user config");
        fs::create_dir_all(git_root.path().join(".git")).expect("mkdir .git");
        fs::create_dir_all(git_root.path().join(".cox")).expect("mkdir .cox");
        // The project layer both disables an existing, user-trusted server
        // and adds a brand-new one already unsandboxed — the sandbox is
        // exactly what contains a command a cloned repository chose, so
        // neither may take effect.
        fs::write(
            git_root.path().join(".cox/config.toml"),
            "[mcp.servers.gh]\nsandbox = false\n\n[mcp.servers.new]\ncommand = \"x\"\nsandbox = false\n",
        )
        .expect("write project config");

        temp_env(&[("COX_HOME", Some(home.path().to_str().unwrap()))], || {
            let loaded = load_plain(git_root.path()).expect("load succeeds");
            let gh = &loaded.config.mcp.servers["gh"];
            assert!(
                gh.sandbox,
                "the opt-out on an existing server must be ignored"
            );
            assert_eq!(
                gh.command.as_deref(),
                Some("gh-mcp"),
                "the guard must revert only sandbox, not the whole server"
            );
            assert!(
                loaded.config.mcp.servers["new"].sandbox,
                "a project layer must not be able to add an unsandboxed server either"
            );
            // Only user config (and env/flags) may opt a server out: its
            // own choice for a server it named must hold.
            assert!(!loaded.config.mcp.servers["opt-out"].sandbox);
            let violation = loaded
                .violations
                .iter()
                .find(|v| v.key == "mcp.servers.*.sandbox")
                .expect("both project-caused opt-outs are one violation");
            assert!(violation.project_value.contains("gh"), "{violation:?}");
            assert!(violation.project_value.contains("new"), "{violation:?}");
            assert!(
                !violation.project_value.contains("opt-out"),
                "{violation:?}"
            );
        });
    }

    #[test]
    fn config_ignores_test_only_cox_env_vars() {
        // `COX_EXPECT_SANDBOX` (set globally in CI) and the provider
        // selectors are not config keys; they must not fail the load as
        // `expect.sandbox` / unknown fields.
        let home = tempdir().expect("tempdir");
        let cwd = tempdir().expect("tempdir");
        temp_env(
            &[
                ("COX_HOME", Some(home.path().to_str().unwrap())),
                // Isolate from the real ~/.claude/settings.json import.
                ("HOME", Some(home.path().to_str().unwrap())),
                ("COX_EXPECT_SANDBOX", Some("bwrap")),
                ("COX_PROVIDER", Some("scripted")),
                ("COX_SCENARIO", Some("/tmp/scenario.toml")),
                ("COX_PLAIN", Some("1")),
                ("COX_AX_STARTUP_QUIET_MS", Some("300")),
                ("COX_KEYRING", Some("off")),
            ],
            || {
                let loaded = load_plain(cwd.path()).expect("load succeeds");
                // `COX_HOME` still overrides `core.home`; everything else
                // must be the embedded default layer (no `expect` layer
                // leaked in).
                assert_eq!(loaded.config.core.home, home.path().to_str().unwrap());
                let mut expected: Config = Figment::new()
                    .merge(Toml::string(DEFAULT_CONFIG_TOML))
                    .extract()
                    .expect("defaults parse");
                expected.core.home = loaded.config.core.home.clone();
                assert_eq!(loaded.config, expected);
            },
        );
    }

    #[test]
    fn config_env_overrides_keys_with_underscores() {
        let home = tempdir().expect("tempdir");
        let cwd = tempdir().expect("tempdir");
        temp_env(
            &[
                ("COX_HOME", Some(home.path().to_str().unwrap())),
                ("HOME", Some(home.path().to_str().unwrap())),
                ("COX_TUI_SHOW_THINKING", Some("full")),
                ("COX_TIERS_CODE_MAX_TOKENS", Some("1234")),
            ],
            || {
                let loaded = load_plain(cwd.path()).expect("load succeeds");
                assert_eq!(loaded.config.tui.show_thinking, "full");
                assert_eq!(loaded.config.tiers.code.max_tokens, 1234);
                assert_eq!(loaded.source_of("tui.show_thinking"), "env");
            },
        );
    }

    #[test]
    fn env_key_resolves_known_keys_and_splits_the_rest() {
        let tree = default_key_tree();
        assert_eq!(env_key(&tree, "HOOKS_TIMEOUT_S"), "hooks.timeout_s");
        assert_eq!(env_key(&tree, "TIERS_CODE_MODEL"), "tiers.code.model");
        assert_eq!(env_key(&tree, "TUI_ICONS_TOOL_ICON"), "tui.icons.tool.icon");
        assert_eq!(env_key(&tree, "TIERS_FAST_MODEL"), "tiers.fast.model");
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
                let loaded = load_plain(git_root.path()).expect("load succeeds");
                assert_eq!(loaded.config.tiers.code.model, "env-model");
                assert_eq!(loaded.source_of("tiers.code.model"), "env");
            },
        );
    }
}
