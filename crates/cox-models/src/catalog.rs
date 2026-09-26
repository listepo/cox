//! The merged model catalog (plan.md T30.24; `docs/design/providers.md` §
//! "Target shape" item 3): one [`ModelRow`] per model id, combining three
//! layers in override order — built-in rows < a `Config`'s
//! `[providers.*].models` entries < a user price file. `Caps` derivation
//! (T30.25) and the effort map (T30.26, `effort.rs`) are the first real
//! readers of `context_window`/`efforts`/`capabilities`; this module only
//! builds and merges the row.

use std::collections::HashMap;

use cox_protocol::Config;
use cox_protocol::config::{DEFAULT_CONFIG_TOML, ProviderModel};
use cox_protocol::types::Effort;
use figment::Figment;
use figment::providers::{Format, Toml};
use thiserror::Error;

use crate::price::{Price, PriceError, PriceTable};

/// What a model is known to support. Every field is `None` until a data
/// source supplies it: the vendor pipeline (`cox-vendor models`, T30.20)
/// does not emit these yet, so every built-in row starts out `Default`;
/// T30.25 (`Caps`/adaptive thinking) and T30.26 (`effort_for`) are the
/// first readers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Capabilities {
    /// Accepts tool definitions on a request.
    pub tools: Option<bool>,
    /// Sends extended/adaptive thinking.
    pub adaptive_thinking: Option<bool>,
    /// Accepts a reasoning-effort wire parameter. Declared per model by
    /// `ProviderModel.reasoning_effort` (T30.26); read by
    /// [`crate::effort_for`] for the Chat wire.
    pub reasoning_effort_param: Option<bool>,
}

impl Capabilities {
    /// What a config `models` entry declares. A wire that holds only its
    /// section's entries (Chat) and the catalog merge both read it here,
    /// so the two cannot disagree.
    pub fn declared_by(model: &ProviderModel) -> Self {
        Self {
            reasoning_effort_param: model.reasoning_effort,
            ..Self::default()
        }
    }
}

/// One catalog row: everything the catalog knows about a model id.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelRow {
    /// The model id, exactly as sent on the wire (a `ModelId` string).
    pub id: String,
    /// Context window in tokens, when a layer has supplied one.
    pub context_window: Option<u32>,
    /// Max output tokens, when a layer has supplied one (no source emits
    /// this yet; see [`Capabilities`]'s doc comment).
    pub max_output: Option<u32>,
    /// Efforts this model supports; empty means "any" — `ProviderModel`'s
    /// own convention, kept so a row built from config matches it exactly.
    pub efforts: Vec<Effort>,
    /// Capability flags, absent where the data doesn't exist yet.
    pub capabilities: Capabilities,
    /// Price, once a `prices.toml` row (built-in or user) prices this id.
    pub price: Option<Price>,
}

impl ModelRow {
    fn new(id: String) -> Self {
        Self {
            id,
            context_window: None,
            max_output: None,
            efforts: Vec::new(),
            capabilities: Capabilities::default(),
            price: None,
        }
    }
}

/// Model-id prefixes that take Anthropic's `thinking: {"type": "adaptive"}`
/// field. Older models want `{"type": "enabled", "budget_tokens": N}`,
/// which is a 400 on these — cox never sends `budget_tokens`, so an
/// unlisted model simply gets no `thinking` field.
///
/// Moved here from `cox-provider::anthropic::request::
/// ADAPTIVE_THINKING_PREFIXES` (T30.25) so the rule lives with the rest of
/// the model catalog. It stays a plain prefix rule rather than a
/// `ModelRow`/`Capabilities` field: the vendor pipeline (`cox-vendor
/// models`) does not emit an adaptive-thinking signal from models.dev today
/// (`Capabilities::adaptive_thinking` is `None` on every row — see that
/// field's doc), and a row-based lookup would silently stop matching a
/// model id that names no catalog row at all (a preview or custom variant
/// the prefix table has always matched by name). Wiring this from
/// models.dev's `reasoning_options` shape (`budget_tokens` vs `effort`/
/// `toggle`, already distinguished by `scripts/vendor/src/cox_vendor/
/// models.py::cox_effort_for`) is the natural next step once that mapping
/// is verified against live data; out of scope here (T30.25 is Rust-only).
const ADAPTIVE_THINKING_PREFIXES: &[&str] = &[
    "claude-opus-5",
    "claude-sonnet-5",
    "claude-haiku-5",
    "claude-fable-5",
    "claude-mythos-5",
    "claude-opus-4-6",
    "claude-opus-4-7",
    "claude-opus-4-8",
    "claude-sonnet-4-6",
];

/// Whether `model_id` takes Anthropic's adaptive `thinking` field (T30.25;
/// see [`ADAPTIVE_THINKING_PREFIXES`]).
pub fn supports_adaptive_thinking(model_id: &str) -> bool {
    ADAPTIVE_THINKING_PREFIXES
        .iter()
        .any(|p| model_id.starts_with(p))
}

/// Why a catalog could not be built.
#[derive(Debug, Error)]
pub enum CatalogError {
    /// A price table (built-in or user-supplied) failed to parse.
    #[error(transparent)]
    Price(#[from] PriceError),
    /// The embedded or supplied config TOML failed to parse.
    #[error("failed to parse config: {0}")]
    Config(String),
}

/// The merged model catalog: one row per id, built-in rows overridden by
/// config, then by a user price file (in that order, by id).
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    rows: HashMap<String, ModelRow>,
}

impl Catalog {
    /// Built-in rows only: `default.toml`'s `[providers.*].models` arrays
    /// merged with the embedded `prices.toml` — both written only by
    /// `cox-vendor models` (AGENTS.md A48; never hand-edited).
    pub fn builtin() -> Result<Self, CatalogError> {
        let config: Config = Figment::from(Toml::string(DEFAULT_CONFIG_TOML))
            .extract()
            .map_err(|e| CatalogError::Config(e.to_string()))?;
        let mut catalog = Self::default();
        catalog.overlay_config(&config);
        catalog.overlay_prices(&PriceTable::embedded()?);
        Ok(catalog)
    }

    /// [`Catalog::builtin`] overridden by `config`'s `[providers.*].models`
    /// entries, then by `user_prices_toml` (the contents of a
    /// `prices.toml`-shaped file — already read by the caller; this crate
    /// does no I/O of its own).
    pub fn load(config: &Config, user_prices_toml: Option<&str>) -> Result<Self, CatalogError> {
        let mut catalog = Self::builtin()?;
        catalog.overlay_config(config);
        if let Some(content) = user_prices_toml {
            catalog.overlay_prices(&PriceTable::parse(content)?);
        }
        Ok(catalog)
    }

    /// A model's row, if the catalog has one.
    pub fn get(&self, id: &str) -> Option<&ModelRow> {
        self.rows.get(id)
    }

    /// Every row, for diagnostics (`cox doctor`, T30.27).
    pub fn rows(&self) -> impl Iterator<Item = &ModelRow> {
        self.rows.values()
    }

    fn row_mut(&mut self, id: &str) -> &mut ModelRow {
        self.rows
            .entry(id.to_string())
            .or_insert_with(|| ModelRow::new(id.to_string()))
    }

    fn overlay_model(&mut self, model: &ProviderModel) {
        let row = self.row_mut(&model.id);
        row.context_window = Some(model.context_window);
        if !model.efforts.is_empty() {
            row.efforts = model.efforts.clone();
        }
        // Like `efforts`: an entry that declares nothing keeps the row's.
        if let Some(param) = Capabilities::declared_by(model).reasoning_effort_param {
            row.capabilities.reasoning_effort_param = Some(param);
        }
    }

    fn overlay_config(&mut self, config: &Config) {
        for model in &config.providers.anthropic.models {
            self.overlay_model(model);
        }
        for model in &config.providers.openai.models {
            self.overlay_model(model);
        }
        for model in &config.providers.local.models {
            self.overlay_model(model);
        }
        for model in &config.providers.typesafe.models {
            self.overlay_model(model);
        }
        for section in config.providers.custom.values() {
            for model in &section.models {
                self.overlay_model(model);
            }
        }
    }

    fn overlay_prices(&mut self, prices: &PriceTable) {
        for price in prices.prices() {
            self.row_mut(&price.id).price = Some(price.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_price_toml(id: &str, input: f64) -> String {
        format!(
            r#"[[model]]
id = "{id}"
input = {input}
output = 1.0
cache_write = 1.0
cache_read = 1.0
verified_on = "2026-01-01"
source_url = "https://example.test"
"#
        )
    }

    #[test]
    fn builtin_prices_and_windows_the_default_models() {
        let catalog = Catalog::builtin().expect("builtin catalog");
        let row = catalog.get("claude-haiku-4-5").expect("haiku row");
        assert_eq!(row.context_window, Some(200_000));
        assert!(row.price.is_some());
    }

    #[test]
    fn config_models_override_builtin_context_window_and_efforts() {
        let mut config = Config::default();
        config.providers.anthropic.models = vec![ProviderModel {
            id: "claude-haiku-4-5".into(),
            context_window: 555_000,
            efforts: vec![Effort::Low, Effort::High],
            ..ProviderModel::default()
        }];
        let catalog = Catalog::load(&config, None).expect("catalog");
        let row = catalog.get("claude-haiku-4-5").expect("haiku row");
        assert_eq!(row.context_window, Some(555_000));
        assert_eq!(row.efforts, vec![Effort::Low, Effort::High]);
        // The price layer is untouched by the config layer: still built-in.
        assert!(row.price.is_some());
    }

    #[test]
    fn user_price_file_overrides_both_builtin_and_config_layers() {
        let mut config = Config::default();
        config.providers.anthropic.models = vec![ProviderModel {
            id: "claude-haiku-4-5".into(),
            context_window: 555_000,
            efforts: vec![],
            ..ProviderModel::default()
        }];
        let user = user_price_toml("claude-haiku-4-5", 42.0);
        let catalog = Catalog::load(&config, Some(&user)).expect("catalog");
        let row = catalog.get("claude-haiku-4-5").expect("haiku row");
        // config's context_window still wins (the user file carries no
        // such field) — the user file only overrides price.
        assert_eq!(row.context_window, Some(555_000));
        assert_eq!(row.price.as_ref().expect("priced").input, 42.0);
    }

    #[test]
    fn user_price_for_a_model_absent_from_config_still_gets_a_row() {
        let user = user_price_toml("brand-new-model", 7.0);
        let catalog = Catalog::load(&Config::default(), Some(&user)).expect("catalog");
        let row = catalog.get("brand-new-model").expect("new row");
        assert_eq!(row.context_window, None);
        assert_eq!(row.price.as_ref().expect("priced").input, 7.0);
    }

    #[test]
    fn supports_adaptive_thinking_matches_listed_prefixes_only() {
        assert!(supports_adaptive_thinking("claude-sonnet-5"));
        // Prefix match, not exact match: a dated/preview suffix still hits.
        assert!(supports_adaptive_thinking("claude-sonnet-5-20260115"));
        // Not listed: an older/unlisted family gets no `thinking` field.
        assert!(!supports_adaptive_thinking("claude-haiku-4-5"));
        assert!(!supports_adaptive_thinking("gpt-5.1"));
    }

    #[test]
    fn config_entry_declares_the_reasoning_effort_param() {
        let mut config = Config::default();
        config.providers.local.models = vec![ProviderModel {
            id: "qwen3-coder".into(),
            context_window: 32_768,
            reasoning_effort: Some(true),
            ..ProviderModel::default()
        }];
        let catalog = Catalog::load(&config, None).expect("catalog");
        let row = catalog.get("qwen3-coder").expect("qwen row");
        assert_eq!(row.capabilities.reasoning_effort_param, Some(true));
        // A built-in row declares nothing: Chat sends no effort for it.
        let haiku = catalog.get("claude-haiku-4-5").expect("haiku row");
        assert_eq!(haiku.capabilities.reasoning_effort_param, None);
    }

    #[test]
    fn empty_config_efforts_do_not_erase_the_builtin_efforts() {
        // `ProviderModel.efforts: []` means "any" (its own doc comment) —
        // a config override that doesn't mention efforts must not clear
        // whatever the built-in row already had.
        let mut config = Config::default();
        config.providers.anthropic.models = vec![ProviderModel {
            id: "claude-haiku-4-5".into(),
            context_window: 555_000,
            efforts: vec![],
            ..ProviderModel::default()
        }];
        let builtin = Catalog::builtin().expect("builtin catalog");
        let builtin_efforts = builtin
            .get("claude-haiku-4-5")
            .expect("haiku row")
            .efforts
            .clone();
        let catalog = Catalog::load(&config, None).expect("catalog");
        let row = catalog.get("claude-haiku-4-5").expect("haiku row");
        assert_eq!(row.efforts, builtin_efforts);
    }
}
