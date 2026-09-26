//! `cox plugin` (plan.md T33.4): today only `list`. Reports discovery
//! results only — id, source, version, digest and the manifest's *declared*
//! capabilities — and never compiles or runs a plugin's module. PL§1 line 47
//! and line 445: a project plugin is untrusted repository content (D14) and
//! must not load before the user grants it, and `list` compiles only
//! *granted* plugins. Grants land in T33.6, which will replace the
//! `"unknown"` grant column with a real check and, only for a `Granted`
//! plugin, compile it and call `cox_init` for its live contributions. Lives
//! behind the default-on `plugins` feature, like `cox-plugin` itself.

use std::fmt::Write as _;
use std::path::Path;

use cox_plugin::discover::{self, State};
use cox_plugin_api::Capabilities;

use crate::cli::Cli;
use crate::config_load::{cox_home, find_git_root};

/// `cox plugin list [--json]`.
pub fn list(cli: &Cli, cwd: &Path, json: bool) -> String {
    let home = cli.home.clone().unwrap_or_else(cox_home);
    let git_root = find_git_root(cwd);
    let found = discover::discover(&home, git_root.as_deref());

    if json {
        let plugins: Vec<serde_json::Value> = found.plugins.iter().map(row_json).collect();
        return serde_json::json!({ "plugins": plugins, "notices": found.notices }).to_string();
    }

    let mut out = String::new();
    if found.plugins.is_empty() {
        let _ = writeln!(out, "plugins: none");
    }
    for p in &found.plugins {
        let _ = writeln!(out, "{}", row_line(p));
    }
    for notice in &found.notices {
        let _ = writeln!(out, "notice: {notice}");
    }
    out
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

fn row_line(p: &discover::Plugin) -> String {
    match &p.state {
        State::Skipped { reason } => format!("{} ({}): skipped — {reason}", p.id, p.source),
        State::Loaded { manifest, digest } => {
            let digest12 = &digest[..12];
            format!(
                "{} ({}, v{}, digest {digest12}, grant unknown, discovered): {}",
                p.id,
                p.source,
                manifest.version,
                declared_summary(&manifest.capabilities)
            )
        }
    }
}

fn row_json(p: &discover::Plugin) -> serde_json::Value {
    match &p.state {
        State::Skipped { reason } => serde_json::json!({
            "id": p.id,
            "source": p.source.to_string(),
            "state": "skipped",
            "reason": reason,
        }),
        State::Loaded { manifest, digest } => serde_json::json!({
            "id": p.id,
            "source": p.source.to_string(),
            "version": manifest.version,
            "digest": digest,
            "digest12": &digest[..12],
            "grant": "unknown",
            "state": "discovered",
            "declared": serde_json::to_value(&manifest.capabilities).unwrap_or_default(),
        }),
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
}
