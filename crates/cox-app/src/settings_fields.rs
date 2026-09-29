//! The Settings screen's layout rules (DT§5.7, T58.4.8): the page a key
//! falls on, the box it sits in, its label, its detail line, the provider
//! sections a key can be stored for, and whether a typed key may be stored.
//! Separate from `settings`, which loads and edits the config, because these
//! only decide what a client shows; every client reads them from here
//! instead of writing them again (T58.4).

use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;

use crate::permissions::RuleKind;
use crate::settings::Layer;

/// The pages DT§5.7 names, in its order, by a key's top-level table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsGroup {
    General,
    Models,
    Permissions,
    Sandbox,
    Budget,
    Mcp,
    Plugins,
    Appearance,
    Advanced,
}

impl SettingsGroup {
    /// DT§5.7's order, the sidebar's.
    pub const ALL: [Self; 9] = [
        Self::General,
        Self::Models,
        Self::Permissions,
        Self::Sandbox,
        Self::Budget,
        Self::Mcp,
        Self::Plugins,
        Self::Appearance,
        Self::Advanced,
    ];

    /// The page `key` falls on; a table DT§5.7 does not name is Advanced.
    pub fn of(key: &str) -> Self {
        match key.split_once('.').map_or(key, |(table, _)| table) {
            "core" => Self::General,
            "tiers" | "jobs" | "providers" => Self::Models,
            "permissions" => Self::Permissions,
            "sandbox" => Self::Sandbox,
            "budget" => Self::Budget,
            "mcp" => Self::Mcp,
            "plugins" => Self::Plugins,
            "desktop" => Self::Appearance,
            _ => Self::Advanced,
        }
    }
}

/// The key's last segment in words, `base_url` → `Base url`: the label a
/// field shows and the search also matches.
pub fn title(key: &str) -> String {
    let words = key.rsplit('.').next().unwrap_or(key).replace('_', " ");
    let mut chars = words.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// The box `key` sits in, the table that holds it (`tiers.code`). A rule
/// list has none: the Permissions page shows those as its own rule box.
pub fn table(key: &str) -> Option<String> {
    if RuleKind::ALL.iter().any(|kind| kind.key() == key) {
        return None;
    }
    Some(
        key.rsplit_once('.')
            .map_or("", |(table, _)| table)
            .to_owned(),
    )
}

/// The provider section a `providers.<name>` table's own key belongs to,
/// so that box takes the provider's key; a deeper table has none.
pub fn provider(key: &str) -> Option<String> {
    match key.split('.').collect::<Vec<_>>()[..] {
        ["providers", name, _] => Some(name.to_owned()),
        _ => None,
    }
}

/// The line under a field: the project file for a value the project sets,
/// else the schema's help, else nothing.
pub fn detail(layer: Layer, description: &str, project_file: Option<&Path>) -> Option<String> {
    match project_file {
        Some(file) if layer == Layer::Project => Some(format!("Set in {}", file.display())),
        _ => (!description.is_empty()).then(|| description.to_owned()),
    }
}

/// The provider sections any key names, sorted and each once.
pub fn providers<'a>(keys: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    keys.into_iter()
        .filter_map(|key| match key.split('.').collect::<Vec<_>>()[..] {
            ["providers", name, _, ..] => Some(name.to_owned()),
            _ => None,
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// A dropped value's change, `999 → 5`: what the project set, then what holds.
pub fn change(value: &str, kept: &str) -> String {
    format!("{value} → {kept}")
}

/// Why a provider key was not stored.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    #[error("the key is empty")]
    Empty,
    /// Not a `[providers.<name>]` section of the current view.
    #[error("no provider section `{provider}`")]
    UnknownProvider { provider: String },
}

/// The key to store for `provider`, trimmed; refused when nothing is left
/// or `providers` (a view's `providers`) does not list the section.
pub fn check_key(providers: &[String], provider: &str, secret: &str) -> Result<String, KeyError> {
    let secret = secret.trim();
    if secret.is_empty() {
        return Err(KeyError::Empty);
    }
    if !providers.iter().any(|p| p == provider) {
        return Err(KeyError::UnknownProvider {
            provider: provider.to_owned(),
        });
    }
    Ok(secret.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_fall_into_dt_5_7_groups() {
        let keys = [
            "core.max_turns",
            "tiers.code.model",
            "jobs.title.model",
            "providers.anthropic.base_url",
            "permissions.mode",
            "sandbox.network",
            "budget.session_usd",
            "mcp.servers.x.url",
            "plugins.dir",
            "desktop.appearance.tint",
            "telemetry.otlp",
            "lonely",
        ];
        let groups: Vec<SettingsGroup> = keys.iter().map(|k| SettingsGroup::of(k)).collect();
        use SettingsGroup::*;
        assert_eq!(
            groups,
            [
                General,
                Models,
                Models,
                Models,
                Permissions,
                Sandbox,
                Budget,
                Mcp,
                Plugins,
                Appearance,
                Advanced,
                Advanced
            ]
        );
        assert_eq!(SettingsGroup::ALL.first(), Some(&General));
        assert_eq!(SettingsGroup::ALL.last(), Some(&Advanced));
    }

    #[test]
    fn a_title_is_the_last_segment_in_words() {
        assert_eq!(title("providers.anthropic.base_url"), "Base url");
        assert_eq!(title("tiers.code.model"), "Model");
        assert_eq!(title("lonely"), "Lonely");
    }

    #[test]
    fn rule_lists_have_no_box() {
        assert_eq!(table("permissions.allow"), None);
        assert_eq!(table("permissions.deny"), None);
        assert_eq!(table("permissions.mode").as_deref(), Some("permissions"));
        assert_eq!(table("tiers.code.model").as_deref(), Some("tiers.code"));
    }

    #[test]
    fn only_a_provider_tables_own_key_names_its_provider() {
        assert_eq!(
            provider("providers.anthropic.base_url").as_deref(),
            Some("anthropic")
        );
        assert_eq!(provider("providers.anthropic.headers.x"), None);
        assert_eq!(provider("tiers.code.model"), None);
        assert_eq!(
            providers([
                "providers.openai.base_url",
                "providers.anthropic.models",
                "providers.anthropic.headers.x",
                "tiers.code.model",
            ]),
            ["anthropic", "openai"]
        );
    }

    #[test]
    fn a_project_value_names_its_file_else_the_help() {
        let file = Path::new("/p/.cox/config.toml");
        assert_eq!(
            detail(Layer::Project, "help", Some(file)).as_deref(),
            Some("Set in /p/.cox/config.toml")
        );
        assert_eq!(
            detail(Layer::User, "help", Some(file)).as_deref(),
            Some("help")
        );
        assert_eq!(detail(Layer::Default, "", None), None);
    }

    #[test]
    fn a_key_for_an_unknown_provider_is_refused() {
        let providers = ["anthropic".to_owned()];
        assert_eq!(
            check_key(&providers, "nope", "k"),
            Err(KeyError::UnknownProvider {
                provider: "nope".into()
            })
        );
        assert_eq!(
            check_key(&providers, "anthropic", " \n"),
            Err(KeyError::Empty)
        );
        assert_eq!(
            check_key(&providers, "anthropic", "  sk-test \n").as_deref(),
            Ok("sk-test")
        );
    }
}
