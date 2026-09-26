//! The plugin contract shared by the host (`cox-plugin`), the guest SDK and
//! `cox_protocol::plugin`: the `plugin.toml` manifest today, the ABI payloads
//! and widget tree in later P33 cards. A crate of its own because the guest
//! SDK builds it for `wasm32-unknown-unknown`, so it depends on no workspace
//! crate and does no I/O (plan.md §1.1, A52).
//!
//! - [`manifest`] — `PluginManifest` and its validation (PL§2); the schema is
//!   committed as `docs/plugin.schema.json`.

#![warn(missing_docs)]

pub mod manifest;

pub use manifest::{
    API_MAJOR, Capabilities, FsCaps, Limits, ManifestError, McpDecl, ModelDecl, ModelTier,
    PluginManifest, PriceDecl, ProviderApi, ProviderAuth, ProviderDecl, UiCaps,
};

#[cfg(test)]
mod tests {
    use std::path::Path;

    use schemars::schema_for;

    use crate::PluginManifest;

    /// Pins the manifest's JSON Schema to `docs/plugin.schema.json`, so a
    /// shape change is a reviewable diff for plugin authors and their
    /// editors instead of a silent break. Same shape as `cox-protocol`'s
    /// `protocol_jsonschema_matches_committed_file`.
    #[test]
    fn plugin_schema_matches_committed_file() {
        let generated = serde_json::to_string_pretty(&schema_for!(PluginManifest))
            .expect("schema serializes")
            + "\n";

        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/plugin.schema.json");
        match std::fs::read_to_string(&path) {
            Ok(committed) => assert_eq!(
                committed, generated,
                "docs/plugin.schema.json is stale; regenerate it (see this test) and commit it"
            ),
            Err(_) => {
                // First run: create it. `git status` will show it as new for review.
                std::fs::write(&path, &generated).expect("write docs/plugin.schema.json");
            }
        }
    }
}
