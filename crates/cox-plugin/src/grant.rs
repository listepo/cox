//! The grant check (PL§3, T33.6): whether a discovered plugin may load
//! under the grant on file. Pure — the caller reads the `plugin_grants` row
//! through `PluginStore` and passes it in — so every surface (session open,
//! `cox plugin list`, the TUI dialog) answers the same question the same
//! way. Also owns the granted-capability list, the JSON `capabilities`
//! column `cox-store` persists opaquely, so writing and checking a grant
//! share one shape.

use std::collections::BTreeSet;
use std::path::Path;

use cox_plugin_api::{ModelTier, PluginManifest};
use cox_protocol::{GrantScope, PluginGrant};

use crate::discover::Source;

/// What a plugin may do at session open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The digest matches and every requested capability is granted.
    Granted,
    /// No grant for these bytes, or the manifest asks for more than was
    /// granted. `added` is what approval would newly allow, `removed` what
    /// the grant allows but the manifest no longer asks for.
    NeedsApproval {
        /// Requested and not granted, sorted.
        added: Vec<String>,
        /// Granted and no longer requested, sorted.
        removed: Vec<String>,
    },
    /// The user turned the plugin off (`cox plugin disable`).
    Disabled,
}

/// Checks `manifest` at `digest` against the grant on file, if any.
/// Asking for fewer capabilities than granted never re-asks; changed bytes
/// always do, since the grant names the digest it was decided against.
pub fn check(manifest: &PluginManifest, digest: &str, stored: Option<&PluginGrant>) -> Verdict {
    let requested = capability_list(manifest);
    let Some(grant) = stored else {
        return Verdict::NeedsApproval {
            added: requested,
            removed: Vec::new(),
        };
    };
    if !grant.enabled {
        return Verdict::Disabled;
    }
    // A row whose capabilities are not a list of strings grants nothing:
    // a corrupt row must never widen what loads.
    let granted: BTreeSet<String> =
        serde_json::from_value::<Vec<String>>(grant.capabilities.clone())
            .map_or_else(|_| BTreeSet::new(), |caps| caps.into_iter().collect());
    let added: Vec<String> = requested
        .iter()
        .filter(|cap| !covered(cap, &granted))
        .cloned()
        .collect();
    if grant.digest == digest && added.is_empty() {
        return Verdict::Granted;
    }
    let removed = granted
        .iter()
        .filter(|cap| !requested.contains(cap))
        .cloned()
        .collect();
    Verdict::NeedsApproval { added, removed }
}

/// Whether `cap` is within `granted`. The model tier is the one ordered
/// capability: a grant of `code` covers a request for `cheap`, so asking
/// for the lower tier never re-asks.
fn covered(cap: &str, granted: &BTreeSet<String>) -> bool {
    granted.contains(cap) || (cap == MODEL_CHEAP && granted.contains(MODEL_CODE))
}

const MODEL_CHEAP: &str = "model:cheap";
const MODEL_CODE: &str = "model:code";

/// The manifest's requests as one sorted line each — the unit of approval
/// (PL§2) and the JSON array stored in `plugin_grants.capabilities`. Covers
/// `[capabilities]` plus the sections that reach the network or run a
/// program (`[[provider]]`, `[[mcp]]`, `[[external_agents]]`), so a diff
/// names every one of them.
pub fn capability_list(manifest: &PluginManifest) -> Vec<String> {
    let caps = &manifest.capabilities;
    let mut out = BTreeSet::new();
    let lists = [
        ("events", &caps.events),
        ("hooks", &caps.hooks),
        ("tools", &caps.tools),
        ("invoke", &caps.invoke),
        ("net", &caps.net),
        ("fs.read", &caps.fs.read),
        ("fs.write", &caps.fs.write),
        ("decide", &caps.decide),
        ("ui.render", &caps.ui.render),
    ];
    for (kind, items) in lists {
        out.extend(items.iter().map(|item| format!("{kind}:{item}")));
    }
    let flags = [
        ("wasi", manifest.wasi),
        ("context", caps.context),
        ("kv", caps.kv),
        ("ui.status", caps.ui.status),
        ("ui.panel", caps.ui.panel),
        ("ui.overlay", caps.ui.overlay),
        ("ui.commands", caps.ui.commands),
        ("ui.keys", caps.ui.keys),
    ];
    out.extend(
        flags
            .iter()
            .filter(|(_, on)| *on)
            .map(|(k, _)| k.to_string()),
    );
    match caps.model {
        Some(ModelTier::Cheap) => out.insert(MODEL_CHEAP.to_string()),
        Some(ModelTier::Code) => out.insert(MODEL_CODE.to_string()),
        None => false,
    };
    for p in &manifest.provider {
        let key = p.api_key_env.as_deref().unwrap_or("-");
        out.insert(format!("provider:{} {} key={key}", p.name, p.base_url));
    }
    for m in &manifest.mcp {
        let target = m.url.clone().unwrap_or_else(|| {
            std::iter::once(m.command.as_deref().unwrap_or(""))
                .chain(m.args.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(" ")
        });
        out.insert(format!("mcp:{} {target}", m.name));
    }
    for a in &manifest.external_agents {
        let argv = std::iter::once(a.command.as_str())
            .chain(a.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ");
        out.insert(format!("agent:{} {argv} key={}", a.name, a.key_env));
    }
    out.into_iter().collect()
}

/// The grant scope a discovered plugin is checked under: a project
/// plugin's grant is keyed on its repository root (PL§1), so the same id
/// in another clone asks again.
pub fn scope(source: Source, project_root: Option<&Path>) -> Option<GrantScope> {
    match source {
        Source::User => Some(GrantScope::User),
        Source::Project => project_root.map(|root| GrantScope::Project(root.to_path_buf())),
    }
}

/// The command a headless surface names for a plugin that did not load
/// (PL§3: "one `Notice(Warn)` naming the command to run").
pub fn enable_command(id: &str, source: Source) -> String {
    match source {
        Source::User => format!("cox plugin enable {id}"),
        Source::Project => format!("cox plugin enable {id} --project"),
    }
}

#[cfg(test)]
mod tests {
    use cox_plugin_api::Capabilities;
    use serde_json::json;

    use super::*;

    fn manifest(caps: Capabilities) -> PluginManifest {
        PluginManifest {
            api: 1,
            id: "demo".into(),
            version: "0.1.0".into(),
            name: "Demo".into(),
            description: String::new(),
            wasm: "plugin.wasm".into(),
            wasi: false,
            limits: Default::default(),
            capabilities: caps,
            provider: Vec::new(),
            models: Vec::new(),
            mcp: Vec::new(),
            external_agents: Vec::new(),
        }
    }

    fn grant(digest: &str, caps: &[&str]) -> PluginGrant {
        PluginGrant {
            plugin_id: "demo".into(),
            scope: GrantScope::User,
            digest: digest.into(),
            capabilities: json!(caps),
            enabled: true,
            source: json!({}),
            decided_at: "2026-09-26T00:00:00Z".into(),
        }
    }

    #[test]
    fn narrower_request_needs_no_reapproval() {
        let m = manifest(Capabilities {
            kv: true,
            model: Some(ModelTier::Cheap),
            ..Capabilities::default()
        });
        let stored = grant("d1", &["kv", "model:code", "net:api.github.com"]);
        assert_eq!(check(&m, "d1", Some(&stored)), Verdict::Granted);
    }

    #[test]
    fn new_digest_needs_approval() {
        let m = manifest(Capabilities {
            kv: true,
            ..Capabilities::default()
        });
        let stored = grant("d1", &["kv"]);
        assert_eq!(
            check(&m, "d2", Some(&stored)),
            Verdict::NeedsApproval {
                added: Vec::new(),
                removed: Vec::new()
            }
        );
        assert!(matches!(
            check(&m, "d2", None),
            Verdict::NeedsApproval { added, .. } if added == ["kv"]
        ));
    }

    #[test]
    fn widened_capability_is_reported_in_added() {
        let m = manifest(Capabilities {
            kv: true,
            net: vec!["api.github.com".into()],
            model: Some(ModelTier::Code),
            ..Capabilities::default()
        });
        let stored = grant("d1", &["kv", "model:cheap", "context"]);
        assert_eq!(
            check(&m, "d1", Some(&stored)),
            Verdict::NeedsApproval {
                added: vec!["model:code".into(), "net:api.github.com".into()],
                removed: vec!["context".into(), "model:cheap".into()],
            }
        );
    }

    #[test]
    fn disabled_grant_never_loads_and_corrupt_row_grants_nothing() {
        let m = manifest(Capabilities {
            kv: true,
            ..Capabilities::default()
        });
        let mut off = grant("d1", &["kv"]);
        off.enabled = false;
        assert_eq!(check(&m, "d1", Some(&off)), Verdict::Disabled);

        let mut corrupt = grant("d1", &[]);
        corrupt.capabilities = json!({"kv": true});
        assert!(matches!(
            check(&m, "d1", Some(&corrupt)),
            Verdict::NeedsApproval { added, .. } if added == ["kv"]
        ));
    }
}
