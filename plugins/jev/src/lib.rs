//! `cox-plugin-jev`: the Jev plugin (T33.40), TypeSafe's "System One"
//! decision model as a `[[provider]] api = "plugin"` section (`plugin.toml`).
//! This crate builds a `wasm32-unknown-unknown` `cdylib` that cox loads with
//! extism, so it lives in the guest workspace `plugins/` (PL§9) rather than
//! the main one.
//!
//! - [`wire`] — the System One request/response types and parser (T33.40.2):
//!   pure, no `extism-pdk`, so it also builds and tests as an `rlib` on the
//!   host target.
//!
//! `cox_provider_stream` (T33.40.3), the `risk` advisor (T33.40.6) and the
//! `route` advisor (T33.40.9) land in later cards; `[[models]]` and
//! `capabilities.decide` follow with them (plan.md T33.40.2's card).

pub mod wire;

/// The `wasm32-unknown-unknown` exports (PL§4), gated so the pure [`wire`]
/// module above builds and tests on the host target too (T33.40.2's plan:
/// "the extism-pdk glue sits behind `cfg(target_arch = "wasm32")`").
#[cfg(target_arch = "wasm32")]
mod guest {
    use cox_plugin_sdk::{InitIn, InitOut, SdkError};

    /// No contributions yet: this card wires the wire format only, not a
    /// decision point (`decide`/`context`/`[[models]]` are later cards). An
    /// `init` export is still required first (`docs/plugins.md`).
    fn init(_: InitIn) -> Result<InitOut, SdkError> {
        Ok(InitOut::default())
    }

    cox_plugin_sdk::register!(init => init);
}

#[cfg(test)]
mod tests {
    use cox_plugin_sdk::PluginManifest;
    use figment::Figment;
    use figment::providers::{Format, Toml};

    /// This crate's own `plugin.toml`, validated the way `cox-plugin`
    /// validates every package it loads (PL§2).
    const MANIFEST: &str = include_str!("../plugin.toml");

    #[test]
    fn manifest_validates() {
        let manifest: PluginManifest = Figment::from(Toml::string(MANIFEST))
            .extract()
            .expect("plugin.toml parses");
        assert_eq!(manifest.id, "jev");
        assert_eq!(manifest.provider.len(), 1, "the typesafe provider section");
        assert_eq!(manifest.provider[0].name, "typesafe");
        assert_eq!(manifest.validate(), Ok(()));
    }
}
