//! `cox plugin` (plan.md T33.4, T33.7, T33.31, T33.32): `list`, `install`,
//! `enable`, `disable`, `update`, `remove`. `list` reports discovery results only — id, source, version,
//! digest and the manifest's *declared* capabilities, plus the real grant
//! state from `grant::check` (T33.7; T33.6 landed the check itself in
//! `session.rs`'s session-open path) — and never compiles or runs a
//! plugin's module (PL§1 line 47/445: a project plugin is untrusted
//! repository content and must not load before the user grants it).
//! `install`/`enable`/`disable`/`update`/`remove` are the only writers of
//! `plugin_grants` outside a session open. The files on disk change only
//! through `cox_plugin::install`; this module keeps the prompts and output. Every manifest string this module prints
//! (`name`, `description`, a capability line) goes through
//! `cox_sanitize::sanitize`, since a plugin's `plugin.toml` is untrusted
//! input the same way a tool result is (AGENTS "Trust boundaries").
//! `remove` (PL§1c) never needs the manifest to parse: a plugin whose
//! `plugin.toml` is corrupt must still be removable, so it is found by
//! whether its directory exists, not through `discover`.

use std::fmt::Write as _;
use std::fs;
use std::io::IsTerminal as _;
use std::path::Path;

use cox_plugin::discover::{self, Source, State};
use cox_plugin::grant::{self, Verdict};
use cox_plugin::install;
use cox_plugin_api::{Capabilities, PluginManifest};
use cox_protocol::{Config, GrantScope, PluginGrant, PluginStore as _, Store as _};
use cox_sanitize::sanitize;
use cox_store::Store;

use crate::cli::Cli;
use crate::config_load::{self, cox_home, find_git_root};

/// `cox plugin list [--json]`.
pub fn list(cli: &Cli, cwd: &Path, json: bool) -> String {
    let home = cli.home.clone().unwrap_or_else(cox_home);
    let git_root = find_git_root(cwd);
    let found = discover::discover(&home, git_root.as_deref());
    // A store that fails to open counts as no grant for every plugin
    // (`grant::check`'s own rule for a read error), never a hard failure:
    // `list` must still show what discovery found.
    let store = Store::open(&home).ok();

    if json {
        let plugins: Vec<serde_json::Value> = found
            .plugins
            .iter()
            .map(|p| row_json(p, store.as_ref(), git_root.as_deref()))
            .collect();
        return serde_json::json!({ "plugins": plugins, "notices": found.notices }).to_string();
    }

    let mut out = String::new();
    if found.plugins.is_empty() {
        let _ = writeln!(out, "plugins: none");
    }
    for p in &found.plugins {
        let _ = writeln!(out, "{}", row_line(p, store.as_ref(), git_root.as_deref()));
    }
    for notice in &found.notices {
        let _ = writeln!(out, "notice: {notice}");
    }
    out
}

/// `cox plugin install <dir> [--yes]` (PL§1): the only v1 source is a
/// local directory. Validates and digests it through the same
/// `discover::load_manifest` a discovered plugin goes through, copies it
/// into `<home>/plugins/<id>/versions/<digest12>/`, writes `current`
/// atomically (temp file, then rename), then runs the same approval flow
/// as `enable`, recording the source path in the grant (PL§1: "install
/// records `{kind: "path", path, digest}`").
pub fn install(cli: &Cli, dir: &Path, yes: bool) -> anyhow::Result<()> {
    let home = cli.home.clone().unwrap_or_else(cox_home);
    // Absolute, so `update` can re-read the recorded source from any cwd.
    let dir = &fs::canonicalize(dir)
        .map_err(|e| anyhow::anyhow!("cannot install {}: {e}", dir.display()))?;
    let manifest_path = dir.join("plugin.toml");
    let (manifest, digest) = discover::load_manifest(dir, &manifest_path, None)
        .map_err(|e| anyhow::anyhow!("cannot install {}: {e}", dir.display()))?;
    let digest12 = install::short(&digest);
    let plugin_dir = install::plugin_dir(&home, &manifest.id);
    let dest = install::stage(dir, &plugin_dir, &digest)?;
    install::activate(&plugin_dir, digest12)?;
    println!(
        "installed {} v{} (digest {digest12}) into {}",
        manifest.id,
        manifest.version,
        dest.display()
    );

    let store = Store::open(&home)?;
    let source = serde_json::json!({
        "kind": "path",
        "path": dir.display().to_string(),
        "digest": digest,
    });
    decide(
        &store,
        &manifest.id,
        &GrantScope::User,
        &digest,
        &manifest,
        source,
        yes,
    )?;
    Ok(())
}

/// `cox plugin enable <id> [--project] [--yes]` (PL§3). `--project` is
/// what makes a project plugin reachable at all: without it, discovery
/// never looks under `.cox/plugins/`, so a project-only id comes back
/// "not found" and no grant is written — a repository must not gain a
/// grant just by being visited (`project_plugin_needs_project_grant`).
pub fn enable(cli: &Cli, cwd: &Path, id: &str, project: bool, yes: bool) -> anyhow::Result<()> {
    let home = cli.home.clone().unwrap_or_else(cox_home);
    let root = if project { find_git_root(cwd) } else { None };
    if project && root.is_none() {
        println!(
            "plugin {id}: --project needs a git repository at {}",
            cwd.display()
        );
        return Ok(());
    }
    let found = discover::discover(&home, root.as_deref());
    let Some(p) = found.plugins.iter().find(|p| p.id == id) else {
        println!(
            "plugin {id} not found{}",
            if project {
                ""
            } else {
                " (pass --project to grant a project plugin)"
            }
        );
        return Ok(());
    };
    let (manifest, digest) = match &p.state {
        State::Loaded { manifest, digest } => (manifest.as_ref(), digest.as_str()),
        State::Skipped { reason } => {
            println!("plugin {id} skipped: {reason}");
            return Ok(());
        }
    };
    // `discover` only returns a project plugin when `root` is `Some`, so
    // `grant::scope` always resolves here.
    let Some(scope) = grant::scope(p.source, root.as_deref()) else {
        println!("plugin {id}: no repository root to scope the grant to");
        return Ok(());
    };
    let store = Store::open(&home)?;
    let stored = store.grant_get(id, &scope, digest).ok().flatten();
    if grant::check(manifest, digest, stored.as_ref()) == Verdict::Granted {
        println!("plugin {id} is already granted");
        return Ok(());
    }
    let source = stored
        .map(|g| g.source)
        .unwrap_or_else(|| default_source(p.source, &p.dir));
    decide(&store, id, &scope, digest, manifest, source, yes)?;
    Ok(())
}

/// `cox plugin disable <id> [--project]` (PL§1): clears `enabled` on the
/// grant row; the plugin's files and kv data stay untouched.
pub fn disable(cli: &Cli, cwd: &Path, id: &str, project: bool) -> anyhow::Result<()> {
    let home = cli.home.clone().unwrap_or_else(cox_home);
    let root = if project { find_git_root(cwd) } else { None };
    let found = discover::discover(&home, root.as_deref());
    let Some(p) = found.plugins.iter().find(|p| p.id == id) else {
        println!("plugin {id} not found");
        return Ok(());
    };
    let State::Loaded { digest, .. } = &p.state else {
        println!("plugin {id} is skipped; nothing to disable");
        return Ok(());
    };
    let Some(scope) = grant::scope(p.source, root.as_deref()) else {
        println!("plugin {id}: no repository root to scope the grant to");
        return Ok(());
    };
    let store = Store::open(&home)?;
    match store.grant_set_enabled(id, &scope, digest, false) {
        Ok(()) => println!("plugin {id} disabled"),
        Err(cox_protocol::StoreError::NotFound) => {
            println!("plugin {id} has no grant to disable");
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

/// `cox plugin update [<id>… | --all] [--check] [--rollback] [--yes]`
/// (PL§1b, T33.31). User plugins only: a project plugin is read in place,
/// so there is nothing to update. Each id is handled on its own, so one
/// broken source does not stop the rest; any failure makes the exit
/// non-zero.
pub fn update(
    cli: &Cli,
    ids: &[String],
    all: bool,
    check: bool,
    rollback: bool,
    yes: bool,
) -> anyhow::Result<()> {
    let home = cli.home.clone().unwrap_or_else(cox_home);
    let found = discover::discover(&home, None);
    let store = Store::open(&home)?;
    let targets: Vec<&str> = if all {
        found.plugins.iter().map(|p| p.id.as_str()).collect()
    } else {
        ids.iter().map(String::as_str).collect()
    };
    let mut failed = false;
    for id in targets {
        let Some(p) = found.plugins.iter().find(|p| p.id == id) else {
            println!("plugin {id} not found (update covers installed user plugins)");
            failed = true;
            continue;
        };
        let done = if rollback {
            rollback_one(&store, &home, p, yes)
        } else {
            update_one(&store, &home, p, check, yes)
        };
        if let Err(e) = done {
            println!("plugin {id}: {e:#}");
            failed = true;
        }
    }
    if failed {
        anyhow::bail!("not every plugin was updated");
    }
    Ok(())
}

/// `cox plugin remove <id> [--keep-data] [--yes]` (PL§1c, T33.32). Found by
/// whether `<home>/plugins/<id>` or `<git root>/.cox/plugins/<id>` exists,
/// not through `discover`: a plugin whose `plugin.toml` no longer parses
/// must still be removable. Steps 2 and 4 of PL§1c collapse into one:
/// `grants_delete` removes every grant row for the id up front, which
/// already makes `grant::check` answer `NeedsApproval` for any digest, so
/// a concurrent session's next open cannot load it — the same effect
/// `disable` would have had, without needing to know which exact digest is
/// current. A project plugin's files are repository content (PL§1c): only
/// its grant and kv go, and the path is printed for the user to delete
/// with git.
pub fn remove(cli: &Cli, cwd: &Path, id: &str, keep_data: bool, yes: bool) -> anyhow::Result<()> {
    let home = cli.home.clone().unwrap_or_else(cox_home);
    let user_dir = install::plugin_dir(&home, id);
    let git_root = find_git_root(cwd);
    let project_dir = git_root
        .as_deref()
        .map(|root| root.join(".cox/plugins").join(id));
    let user_exists = user_dir.exists();
    let project_exists = project_dir.as_deref().is_some_and(Path::exists);
    if !user_exists && !project_exists {
        println!("plugin {id} not found");
        return Ok(());
    }
    if !yes && !confirm(&format!("remove plugin {id}?")) {
        println!("plugin {id} not removed");
        return Ok(());
    }

    let store = Store::open(&home)?;
    store.grants_delete(id)?;

    if user_exists {
        install::remove(&home, id)?;
    }
    if let Some(project_dir) = &project_dir
        && project_exists
    {
        println!(
            "plugin {id} is a project plugin; its files at {} stay — remove them with git if you want them gone",
            project_dir.display()
        );
    }

    if keep_data {
        println!("plugin {id}: kept its stored data (--keep-data)");
    } else {
        store.kv_delete_all(id)?;
    }

    println!("plugin {id} removed");

    if let Ok(loaded) = config_load::load(cwd, cli) {
        for r in config_refs(&loaded.config, id) {
            println!("note: config still references {id}: {r}");
        }
        for r in keybinding_refs(&home, id) {
            println!("note: keybindings.toml still references {id}: {r}");
        }
    }
    Ok(())
}

/// PL§1c step 6: what in the effective config still names `id` after
/// removal — never edited, only reported. Checked against the merged
/// `Config` rather than grepping the raw TOML text, so a reference through
/// a layered project or env override is still caught.
fn config_refs(config: &Config, id: &str) -> Vec<String> {
    let mut refs = Vec::new();
    if config.plugins.entries.contains_key(id) {
        refs.push(format!("[plugins.{id}]"));
    }
    if let Some(decide) = config
        .plugins
        .entries
        .get("decide")
        .and_then(|v| v.as_object())
    {
        for (point, plugin) in decide {
            if plugin.as_str() == Some(id) {
                refs.push(format!("[plugins.decide] {point} = \"{id}\""));
            }
        }
    }
    let prefix = format!("{id}-");
    let mut provider_names: Vec<&str> = Vec::new();
    for name in config.providers.custom.keys() {
        if name == id || name.starts_with(prefix.as_str()) {
            refs.push(format!("[providers.{name}]"));
            provider_names.push(name.as_str());
        }
    }
    for (tier_name, tier) in [
        ("cheap", &config.tiers.cheap),
        ("code", &config.tiers.code),
        ("think", &config.tiers.think),
    ] {
        if provider_names.contains(&tier.provider.as_str()) {
            refs.push(format!(
                "tiers.{tier_name}.provider = \"{}\"",
                tier.provider
            ));
        }
    }
    for name in config.mcp.servers.keys() {
        if name == id || name.starts_with(prefix.as_str()) {
            refs.push(format!("[mcp.servers.{name}]"));
        }
    }
    refs
}

/// PL§1c step 6's `keybindings.toml` rows for `plugin.<id>.*`: a plain
/// text scan, since `keybindings.toml` is a separate file `cox-config`
/// does not merge into `Config` (T25.5, `crate::config_load::keymap`).
fn keybinding_refs(home: &Path, id: &str) -> Vec<String> {
    let text = fs::read_to_string(home.join("keybindings.toml")).unwrap_or_default();
    let prefix = format!("plugin.{id}.");
    text.lines()
        .map(str::trim)
        .filter(|line| line.starts_with(prefix.as_str()))
        .map(str::to_string)
        .collect()
}

/// PL§1b steps 1–8 for one plugin: re-read the source the current grant
/// recorded, validate and digest it through `discover::load_manifest`,
/// print the capability diff against that grant, then stage and switch.
fn update_one(
    store: &Store,
    home: &Path,
    p: &discover::Plugin,
    check: bool,
    yes: bool,
) -> anyhow::Result<()> {
    let id = p.id.as_str();
    let current = match &p.state {
        State::Loaded { digest, .. } => digest.as_str(),
        State::Skipped { reason } => anyhow::bail!("current version is skipped: {reason}"),
    };
    let stored = store
        .grant_get(id, &GrantScope::User, current)
        .ok()
        .flatten();
    let src = stored
        .as_ref()
        .filter(|g| g.source["kind"] == "path")
        .and_then(|g| g.source["path"].as_str())
        .map(Path::new)
        .ok_or_else(|| {
            anyhow::anyhow!("no recorded source path; reinstall with `cox plugin install <dir>`")
        })?;
    let (manifest, digest) = discover::load_manifest(src, &src.join("plugin.toml"), Some(id))
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", src.display()))?;
    if digest == current {
        println!("plugin {id} is up to date ({})", install::short(current));
        return Ok(());
    }
    println!(
        "plugin {id}: {} -> {} (v{})",
        install::short(current),
        install::short(&digest),
        sanitize(&manifest.version)
    );
    print_diff(&manifest, &digest, stored.as_ref());
    if check {
        return Ok(());
    }
    let plugin_dir = install::plugin_dir(home, id);
    install::stage(src, &plugin_dir, &digest)?;
    let source = serde_json::json!({
        "kind": "path",
        "path": src.display().to_string(),
        "digest": digest,
    });
    switch_to(
        store,
        &plugin_dir,
        id,
        (&manifest, &digest),
        source,
        yes,
        "",
    )
}

/// `--rollback`: make `previous` current again. Its grant is keyed on its
/// own digest (PL§3), so a grant still on file lets it switch without
/// asking; a revoked or missing one asks like any other new digest.
fn rollback_one(store: &Store, home: &Path, p: &discover::Plugin, yes: bool) -> anyhow::Result<()> {
    let id = p.id.as_str();
    let plugin_dir = install::plugin_dir(home, id);
    let Some(previous) = install::read_pointer(&plugin_dir, install::PREVIOUS)? else {
        println!("plugin {id} has no previous version to roll back to");
        return Ok(());
    };
    let dir = plugin_dir.join("versions").join(&previous);
    // Digested again rather than trusted by name: a tampered directory
    // gets a different digest, so no stored grant matches it.
    let (manifest, digest) = discover::load_manifest(&dir, &dir.join("plugin.toml"), Some(id))
        .map_err(|e| anyhow::anyhow!("cannot read previous version: {e}"))?;
    let source = match &p.state {
        State::Loaded { digest, .. } => store
            .grant_get(id, &GrantScope::User, digest)
            .ok()
            .flatten()
            .map(|g| g.source),
        State::Skipped { .. } => None,
    }
    .unwrap_or_else(|| default_source(Source::User, &dir));
    switch_to(
        store,
        &plugin_dir,
        id,
        (&manifest, &digest),
        source,
        yes,
        " --rollback",
    )
}

/// Makes a staged version current once its grant allows it: a grant on
/// file for that exact digest switches at once; otherwise the user is
/// asked through `decide`. Headless never approves (PL§1b): stdin that is
/// not a terminal — `cox run -p`, ACP, a pipe, CI — gets no prompt at all,
/// even if it could supply a `y`, so `current` stays and a warning names
/// the command to run. Only `--yes`, a user's own provisioning script,
/// approves without a terminal.
fn switch_to(
    store: &Store,
    plugin_dir: &Path,
    id: &str,
    (manifest, digest): (&PluginManifest, &str),
    source: serde_json::Value,
    yes: bool,
    flag: &str,
) -> anyhow::Result<()> {
    let digest12 = install::short(digest);
    let own = store
        .grant_get(id, &GrantScope::User, digest)
        .ok()
        .flatten();
    let approved = if grant::check(manifest, digest, own.as_ref()) == Verdict::Granted {
        true
    } else if !yes && !std::io::stdin().is_terminal() {
        println!("warning: update for {id} waits for approval: run `cox plugin update {id}{flag}`");
        return Ok(());
    } else {
        decide(store, id, &GrantScope::User, digest, manifest, source, yes)?
    };
    if approved {
        install::activate(plugin_dir, digest12)?;
        println!("plugin {id} is now at {digest12}");
    }
    Ok(())
}

/// PL§1b step 4: what the new manifest asks for beyond the stored grant
/// (`+`, first so it stands out) and what it no longer asks for (`-`).
/// Computed by `grant::check` itself so the diff and the load decision
/// never disagree; a disabled grant is diffed as if enabled, since
/// `Disabled` would otherwise hide the capabilities it granted.
fn print_diff(manifest: &PluginManifest, digest: &str, stored: Option<&PluginGrant>) {
    let active = stored.map(|g| PluginGrant {
        enabled: true,
        ..g.clone()
    });
    let Verdict::NeedsApproval { added, removed } = grant::check(manifest, digest, active.as_ref())
    else {
        return;
    };
    if added.is_empty() && removed.is_empty() {
        println!("  capabilities unchanged; the new bytes still need approval");
    }
    for cap in &added {
        println!("  + {} (new)", sanitize(cap));
    }
    for cap in &removed {
        println!("  - {}", sanitize(cap));
    }
}

/// Prints the capability list in words and asks on stdin unless `yes`;
/// on approval, upserts the grant (`grant_put` replaces any row at the
/// same `(plugin_id, scope, digest)`, PL§3) and returns whether it is now
/// enabled. A decline writes nothing, so a later `enable` sees a fresh
/// `NeedsApproval`, not a stale `Disabled`.
fn decide(
    store: &Store,
    id: &str,
    scope: &GrantScope,
    digest: &str,
    manifest: &PluginManifest,
    source: serde_json::Value,
    yes: bool,
) -> anyhow::Result<bool> {
    let caps = grant::capability_list(manifest);
    println!(
        "{} ({})",
        sanitize(&manifest.name),
        sanitize(&manifest.version)
    );
    if !manifest.description.is_empty() {
        println!("  {}", sanitize(&manifest.description));
    }
    if caps.is_empty() {
        println!("  asks for no capabilities");
    } else {
        println!("  asks to be able to:");
        for cap in &caps {
            println!("    - {}", sanitize(cap));
        }
    }
    let approved = yes || confirm(&format!("grant {id} these capabilities?"));
    if !approved {
        println!("plugin {id} not enabled");
        return Ok(false);
    }
    write_grant(store, id, scope, digest, caps, source)?;
    println!("plugin {id} enabled");
    Ok(true)
}

/// Builds and writes one `PluginGrant` row (PL§3): the single place the row
/// is assembled, so `decide` (`cox plugin enable`/`install`) and the TUI's
/// grant dialog (`crates/cox/src/session.rs`'s `write_plugin_grant`) share
/// it instead of each holding its own `grant_put` literal (T33.8).
pub(crate) fn write_grant(
    store: &Store,
    id: &str,
    scope: &GrantScope,
    digest: &str,
    capabilities: Vec<String>,
    source: serde_json::Value,
) -> Result<(), cox_protocol::StoreError> {
    store.grant_put(&PluginGrant {
        plugin_id: id.to_string(),
        scope: scope.clone(),
        digest: digest.to_string(),
        capabilities: serde_json::json!(capabilities),
        enabled: true,
        source,
        decided_at: cox_store::now_rfc3339(),
    })
}

/// `[y/N]` on stdin, same idiom as `session::offer_worktree_removal`.
fn confirm(question: &str) -> bool {
    eprint!("{question} [y/N] ");
    let mut answer = String::new();
    let _ = std::io::stdin().read_line(&mut answer);
    matches!(answer.trim(), "y" | "Y" | "yes")
}

/// The `source` a grant records when nothing was stored yet: a project
/// plugin is repository content, so there is no external path to
/// remember; a user plugin's is its version directory, the best guess
/// available to a bare `enable` that did not go through `install`.
fn default_source(source: Source, dir: &Path) -> serde_json::Value {
    match source {
        Source::Project => serde_json::json!({ "kind": "project" }),
        Source::User => serde_json::json!({ "kind": "path", "path": dir.display().to_string() }),
    }
}

/// The grant verdict for one discovered, loaded plugin: looks up the row
/// at its exact digest (T33.6's own key) and runs it through
/// `grant::check`, the one pure answer every surface shares. A missing or
/// unreadable store counts as no grant, never a wider one.
fn verdict_for(
    p: &discover::Plugin,
    manifest: &PluginManifest,
    digest: &str,
    store: Option<&Store>,
    project_root: Option<&Path>,
) -> Verdict {
    let stored = grant::scope(p.source, project_root)
        .and_then(|scope| store.and_then(|s| s.grant_get(&p.id, &scope, digest).ok().flatten()));
    grant::check(manifest, digest, stored.as_ref())
}

/// Counts of what a manifest's `[capabilities]` declares, never what a
/// plugin actually does at runtime: nothing here has executed the module.
fn declared_summary(caps: &Capabilities) -> String {
    let counted = [
        ("tools", caps.tools.len()),
        ("events", caps.events.len()),
        ("hooks", caps.hooks.len()),
        ("invoke", caps.invoke.len()),
        ("decide", caps.decide.len()),
        ("render", caps.ui.render.len()),
    ];
    let mut parts: Vec<String> = counted
        .into_iter()
        .filter(|(_, n)| *n > 0)
        .map(|(name, n)| format!("{name}={n}"))
        .collect();
    for (flag, name) in [
        (caps.ui.status, "status"),
        (caps.ui.panel, "panel"),
        (caps.ui.overlay, "overlay"),
        (caps.ui.commands, "commands"),
        (caps.ui.keys, "keys"),
    ] {
        if flag {
            parts.push(name.to_string());
        }
    }
    if parts.is_empty() {
        "no declared capabilities".to_string()
    } else {
        parts.join(" ")
    }
}

/// `("granted"|"disabled"|"needs approval (…)", "loaded"|"not loaded")`.
fn grant_words(v: &Verdict) -> (String, &'static str) {
    match v {
        Verdict::Granted => ("granted".to_string(), "loaded"),
        Verdict::Disabled => ("disabled".to_string(), "not loaded"),
        Verdict::NeedsApproval { added, .. } if added.is_empty() => {
            ("needs approval (package changed)".to_string(), "not loaded")
        }
        Verdict::NeedsApproval { added, .. } => (
            format!("needs approval ({})", added.join(", ")),
            "not loaded",
        ),
    }
}

fn row_line(p: &discover::Plugin, store: Option<&Store>, project_root: Option<&Path>) -> String {
    match &p.state {
        State::Skipped { reason } => format!("{} ({}): skipped — {reason}", p.id, p.source),
        State::Loaded { manifest, digest } => {
            let digest12 = &digest[..12];
            let verdict = verdict_for(p, manifest, digest, store, project_root);
            let (grant_word, loaded_word) = grant_words(&verdict);
            format!(
                "{} ({}, v{}, digest {digest12}, grant {grant_word}, {loaded_word}): {}",
                p.id,
                p.source,
                manifest.version,
                declared_summary(&manifest.capabilities)
            )
        }
    }
}

fn row_json(
    p: &discover::Plugin,
    store: Option<&Store>,
    project_root: Option<&Path>,
) -> serde_json::Value {
    match &p.state {
        State::Skipped { reason } => serde_json::json!({
            "id": p.id,
            "source": p.source.to_string(),
            "state": "skipped",
            "reason": reason,
        }),
        State::Loaded { manifest, digest } => {
            let verdict = verdict_for(p, manifest, digest, store, project_root);
            let (grant_state, loaded, added, removed) = match &verdict {
                Verdict::Granted => ("granted", true, None, None),
                Verdict::Disabled => ("disabled", false, None, None),
                Verdict::NeedsApproval { added, removed } => {
                    ("needs_approval", false, Some(added), Some(removed))
                }
            };
            let mut v = serde_json::json!({
                "id": p.id,
                "source": p.source.to_string(),
                "version": manifest.version,
                "digest": digest,
                "digest12": &digest[..12],
                "grant": grant_state,
                "loaded": loaded,
                "state": "discovered",
                "declared": serde_json::to_value(&manifest.capabilities).unwrap_or_default(),
            });
            if let (Some(added), Some(removed)) = (added, removed) {
                v["grant_added"] = serde_json::json!(added);
                v["grant_removed"] = serde_json::json!(removed);
            }
            v
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_summary_lists_only_nonempty_kinds() {
        let caps = Capabilities {
            tools: vec!["summarise".to_string()],
            ..Capabilities::default()
        };
        assert_eq!(declared_summary(&caps), "tools=1");
        assert_eq!(
            declared_summary(&Capabilities::default()),
            "no declared capabilities"
        );
    }

    #[test]
    fn grant_words_name_loaded_only_when_granted() {
        assert_eq!(
            grant_words(&Verdict::Granted),
            ("granted".to_string(), "loaded")
        );
        assert_eq!(
            grant_words(&Verdict::Disabled),
            ("disabled".to_string(), "not loaded")
        );
        assert_eq!(
            grant_words(&Verdict::NeedsApproval {
                added: vec!["kv".to_string()],
                removed: Vec::new(),
            }),
            ("needs approval (kv)".to_string(), "not loaded")
        );
    }

    #[test]
    fn config_refs_reports_plugins_table_and_decide_entry_and_edits_nothing() {
        let mut config = Config::default();
        config
            .plugins
            .entries
            .insert("git-glance".to_string(), serde_json::json!({"foo": 1}));
        config.plugins.entries.insert(
            "decide".to_string(),
            serde_json::json!({"route": "git-glance"}),
        );

        let refs = config_refs(&config, "git-glance");

        assert!(
            refs.contains(&"[plugins.git-glance]".to_string()),
            "{refs:?}"
        );
        assert!(
            refs.iter()
                .any(|r| r.contains("decide") && r.contains("route")),
            "{refs:?}"
        );
        // Unrelated ids never show up.
        assert!(config_refs(&config, "other-id").is_empty());
    }

    #[test]
    fn config_refs_reports_a_provider_section_and_the_tier_naming_it() {
        let mut config = Config::default();
        config.providers.custom.insert(
            "git-glance-relay".to_string(),
            cox_protocol::config::CompatibleProviderConfig::default(),
        );
        config.tiers.code.provider = "git-glance-relay".to_string();

        let refs = config_refs(&config, "git-glance");

        assert!(
            refs.contains(&"[providers.git-glance-relay]".to_string()),
            "{refs:?}"
        );
        assert!(
            refs.contains(&"tiers.code.provider = \"git-glance-relay\"".to_string()),
            "{refs:?}"
        );
    }

    #[test]
    fn config_refs_reports_an_mcp_server_named_after_the_plugin() {
        let mut config = Config::default();
        config
            .mcp
            .servers
            .insert("git-glance-relay".to_string(), Default::default());

        let refs = config_refs(&config, "git-glance");

        assert!(
            refs.contains(&"[mcp.servers.git-glance-relay]".to_string()),
            "{refs:?}"
        );
    }

    #[test]
    fn keybinding_refs_reports_rows_for_the_plugin_only() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("keybindings.toml"),
            "plugin.git-glance.status = \"ctrl-g\"\nplugin.other.status = \"ctrl-o\"\n",
        )
        .unwrap();

        let refs = keybinding_refs(home.path(), "git-glance");

        assert_eq!(refs, vec!["plugin.git-glance.status = \"ctrl-g\""]);
    }
}
