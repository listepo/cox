//! `cox-plugin-cursor`: the Cursor plugin (P35), the first real user of
//! `[[external_agents]]` (`plan.md` T35.6, design `docs/design/external-agents.md`,
//! cited as EA§n). This crate builds a `wasm32-unknown-unknown` `cdylib`
//! that cox loads with extism, so it lives in the guest workspace `plugins/`
//! (PL§9) rather than the main one.
//!
//! Spawning and driving Cursor's official `agent` CLI over ACP or
//! `stream-json` is entirely the host's job (EA§2): everything this plugin
//! needs is the `[[external_agents]]` entry in `plugin.toml`, so the guest
//! exports only `cox_init`, no other capability — unlike Jev's
//! `[[provider]] api = "plugin"` guest (T33.40).

/// The `wasm32-unknown-unknown` export (PL§4).
#[cfg(target_arch = "wasm32")]
mod guest {
    use cox_plugin_sdk::{InitIn, InitOut, SdkError};

    /// No contributions beyond `plugin.toml`'s `[[external_agents]]` entry:
    /// the host resolves, sandboxes and spawns `agent` itself (EA§2), so
    /// `init` does nothing. An `init` export is still required first
    /// (`docs/plugins.md`).
    fn init(_: InitIn) -> Result<InitOut, SdkError> {
        Ok(InitOut::default())
    }

    cox_plugin_sdk::register!(init => init);
}

#[cfg(test)]
mod tests {
    use cox_plugin_sdk::{AgentMode, PluginManifest};
    use figment::Figment;
    use figment::providers::{Format, Toml};

    /// This crate's own `plugin.toml`, validated the way `cox-plugin`
    /// validates every package it loads (PL§2) — reusing the same
    /// Figment-based helper `cox-plugin-jev`'s `manifest_validates` test
    /// uses, rather than a second parsing path.
    const MANIFEST: &str = include_str!("../plugin.toml");

    #[test]
    fn cursor_plugin_toml_matches_the_manifest_schema() {
        let manifest: PluginManifest = Figment::from(Toml::string(MANIFEST))
            .extract()
            .expect("plugin.toml parses");
        assert_eq!(manifest.id, "cursor");
        assert!(
            manifest.capabilities.tools.is_empty()
                && manifest.provider.is_empty()
                && manifest.mcp.is_empty(),
            "the smallest possible plugin: no capability beyond the agent entry"
        );
        assert_eq!(
            manifest.external_agents.len(),
            1,
            "the one cursor external agent entry"
        );
        let agent = &manifest.external_agents[0];
        assert_eq!(agent.name, "cursor");
        assert_eq!(agent.command, "agent");
        assert_eq!(agent.mode, AgentMode::Acp);
        assert_eq!(agent.key_env, "CURSOR_API_KEY");
        assert_eq!(manifest.validate(), Ok(()));
    }
}
