//! `cox plugin` (plan.md T33.4, T33.7): `list`, `install`, `enable`,
//! `disable`. `list` reports discovery results only — id, source, version,
//! digest and the manifest's *declared* capabilities, plus the real grant
//! state from `grant::check` (T33.7; T33.6 landed the check itself in
//! `session.rs`'s session-open path) — and never compiles or runs a
//! plugin's module (PL§1 line 47/445: a project plugin is untrusted
//! repository content and must not load before the user grants it).
//! `install`/`enable`/`disable` are the only writers of `plugin_grants`
//! outside a session open. Every manifest string this module prints
//! (`name`, `description`, a capability line) goes through
//! `cox_sanitize::sanitize`, since a plugin's `plugin.toml` is untrusted
//! input the same way a tool result is (AGENTS "Trust boundaries").

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use cox_plugin::discover::{self, Source, State};
use cox_plugin::grant::{self, Verdict};
use cox_plugin_api::{Capabilities, PluginManifest};
use cox_protocol::{GrantScope, PluginGrant, PluginStore as _, Store as _};
use cox_sanitize::sanitize;
use cox_store::Store;

use crate::cli::Cli;
use crate::config_load::{cox_home, find_git_root};

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
    let manifest_path = dir.join("plugin.toml");
    let (manifest, digest) = discover::load_manifest(dir, &manifest_path, None)
        .map_err(|e| anyhow::anyhow!("cannot install {}: {e}", dir.display()))?;
    let digest12 = &digest[..12];
    let plugin_dir = home.join("plugins").join(&manifest.id);
    let dest = plugin_dir.join("versions").join(digest12);
    if !dest.exists() {
        copy_dir_recursive(dir, &dest)?;
    }
    write_current_atomic(&plugin_dir, digest12)?;
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
    store.grant_put(&PluginGrant {
        plugin_id: id.to_string(),
        scope: scope.clone(),
        digest: digest.to_string(),
        capabilities: serde_json::json!(caps),
        enabled: true,
        source,
        decided_at: cox_store::now_rfc3339(),
    })?;
    println!("plugin {id} enabled");
    Ok(true)
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

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if ty.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Writes `<plugin_dir>/current` via a temp file plus rename, atomic on
/// one filesystem (PL§1b).
fn write_current_atomic(plugin_dir: &Path, digest12: &str) -> std::io::Result<()> {
    fs::create_dir_all(plugin_dir)?;
    let tmp = plugin_dir.join("current.tmp");
    fs::write(&tmp, digest12)?;
    fs::rename(&tmp, plugin_dir.join("current"))
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
}
