//! Usage and cost tracking (plan.md §1.7/D5/D6g). Loads a dated price
//! table, computes cost per request, and prepares ledger rows for insertion.
//!
//! The price table carries `verified_on` dates; `cox doctor` may warn if a
//! row is older than 90 days. Unknown models are costed as 0 with `estimated
//! = true` and emit a `Notice(Warn)` once per session.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

use figment::Figment;
use figment::providers::{Format, Toml};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use cox_protocol::ids::SessionId;
use cox_protocol::traits::UsageRow;
use cox_protocol::types::{Job, ModelId, ProviderId, Tier, Usage};

const DEFAULT_PRICES: &str = include_str!("../../../config/prices.toml");

/// One `[[model]]` row of `config/prices.toml`: the four per-MTok rates a
/// call is billed at, plus the provenance that lets `cox doctor` flag a
/// stale table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Price {
    /// The `ModelId` string this row prices.
    pub id: String,
    /// Input tokens, USD per MTok.
    pub input: f64,
    /// Output tokens, USD per MTok.
    pub output: f64,
    /// Cache write tokens, USD per MTok.
    pub cache_write: f64,
    /// Cache read tokens, USD per MTok.
    pub cache_read: f64,
    /// ISO date these rates were last checked against `source_url`.
    pub verified_on: String,
    /// The official pricing page the rates were read from.
    pub source_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PriceToml {
    model: Vec<Price>,
}

/// Why a price table could not be loaded.
#[derive(Debug, Error)]
pub enum PriceError {
    /// The TOML did not parse, or did not match the `[[model]]` shape.
    #[error("failed to parse price table: {0}")]
    Parse(String),
    /// The price file existed but could not be read.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Thread-safe price table for cost computation. Loaded from `config/prices.toml`
/// or from embedded defaults.
pub struct PriceTable {
    prices: Vec<Price>,
    /// Models that have already been warned about (missing/unknown).
    warned_models: Mutex<HashSet<String>>,
}

impl PriceTable {
    /// Load the price table from a file, or use embedded defaults if the file
    /// is not found.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, PriceError> {
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => DEFAULT_PRICES.to_string(),
            Err(e) => return Err(PriceError::Io(e)),
        };
        Self::from_str(&content)
    }

    fn from_str(content: &str) -> Result<Self, PriceError> {
        let table: PriceToml = Figment::from(Toml::string(content))
            .extract()
            .map_err(|e| PriceError::Parse(e.to_string()))?;
        Ok(PriceTable {
            prices: table.model,
            warned_models: Mutex::new(HashSet::new()),
        })
    }

    /// Get the price for a model id, or None if not found.
    pub fn price_for(&self, model: &ModelId) -> Option<&Price> {
        self.prices.iter().find(|p| p.id == model.0)
    }

    /// Compute the cost of a `Usage` row using this price table.
    /// If the model is unknown, returns 0 with `estimated = true` (the usage
    /// will be marked as estimated so the caller can emit a warning).
    pub fn cost(&self, usage: &Usage, price: &Price) -> f64 {
        let input_cost = (usage.input_tokens as f64) * price.input / 1_000_000.0;
        let output_cost = (usage.output_tokens as f64) * price.output / 1_000_000.0;
        let cache_write_cost = (usage.cache_write_tokens as f64) * price.cache_write / 1_000_000.0;
        let cache_read_cost = (usage.cache_read_tokens as f64) * price.cache_read / 1_000_000.0;
        input_cost + output_cost + cache_write_cost + cache_read_cost
    }

    /// Returns true the first time this model is seen, false on subsequent calls.
    /// Used to emit one `Notice(Warn)` per session for unknown models.
    pub fn warn_once(&self, model: &ModelId) -> bool {
        // A poisoned lock only means some other thread panicked mid-warn; the
        // warned-set is advisory, so recovering beats propagating.
        let mut warned = self
            .warned_models
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        warned.insert(model.0.clone())
    }
}

/// Fills in the cost of a call and returns the row `Store::usage_insert`
/// takes. `price_for` returning `None` is not an error: the row is still
/// written, costed at 0 and flagged `estimated`, so an unpriced model can
/// never make a call disappear from the ledger.
#[allow(clippy::too_many_arguments)]
pub fn ledger_row(
    session: SessionId,
    turn: u32,
    job: Job,
    tier: Tier,
    provider: ProviderId,
    model: ModelId,
    effort: Option<cox_protocol::types::Effort>,
    usage: Usage,
    prices: &PriceTable,
) -> UsageRow {
    let mut usage = usage;
    match prices.price_for(&model) {
        Some(price) => usage.cost_usd = prices.cost(&usage, price),
        None => {
            usage.cost_usd = 0.0;
            usage.estimated = true;
        }
    }
    UsageRow {
        session_id: session,
        turn,
        job,
        tier,
        provider,
        model,
        effort,
        usage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_prices_toml_parses_and_has_all_tier_models() {
        let table = PriceTable::from_str(DEFAULT_PRICES).expect("default prices parse");
        assert_eq!(table.prices.len(), 20);
        // `price_for` is a linear find, so row order carries no meaning and
        // is not asserted on.
        // Verify all required tier models are present.
        let ids: Vec<&str> = table.prices.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"claude-haiku-4-5"));
        assert!(ids.contains(&"claude-sonnet-5"));
        assert!(ids.contains(&"claude-opus-5"));
        assert!(ids.contains(&"claude-fable-5-1"));
    }

    #[test]
    fn usage_prices_cover_every_configured_model() {
        // The ledger must price every model a user can route to without
        // touching a price file: all `[tiers.*].model` defaults, every
        // `[providers.*]` default `model`, and every `models` list entry.
        // A missing row is not fatal at runtime (costed 0, `estimated`), so
        // this test is the sync check between default.toml and prices.toml.
        use cox_protocol::Config;
        let table = PriceTable::from_str(DEFAULT_PRICES).expect("default prices parse");
        let cfg: Config = Figment::from(Toml::string(cox_protocol::config::DEFAULT_CONFIG_TOML))
            .extract()
            .expect("default.toml parses");
        let mut want = vec![
            cfg.tiers.cheap.model.clone(),
            cfg.tiers.code.model.clone(),
            cfg.tiers.think.model.clone(),
            cfg.providers.local.model.clone(),
        ];
        for section in [
            &cfg.providers.anthropic.models,
            &cfg.providers.openai.models,
            &cfg.providers.local.models,
        ] {
            want.extend(section.iter().map(|m| m.id.clone()));
        }
        for custom in cfg.providers.custom.values() {
            want.push(custom.model.clone());
            want.extend(custom.models.iter().map(|m| m.id.clone()));
        }
        want.sort();
        want.dedup();
        for id in &want {
            assert!(
                table.price_for(&ModelId(id.clone())).is_some(),
                "prices.toml has no row for configured model `{id}`"
            );
        }
    }

    #[test]
    fn usage_cost_matches_hand_computed() {
        let table = PriceTable::from_str(DEFAULT_PRICES).expect("default prices parse");
        let price = table
            .price_for(&ModelId("claude-haiku-4-5".into()))
            .expect("haiku price");
        let usage = Usage {
            input_tokens: 1_000_000,
            output_tokens: 100_000,
            cache_read_tokens: 50_000,
            cache_write_tokens: 30_000,
            estimated: false,
            cost_usd: 0.0, // placeholder; will be set by cost()
            latency_ms: 100,
        };
        let cost = table.cost(&usage, price);
        // Haiku: input $1/M, output $5/M, cache_write $1.25/M, cache_read $0.10/M
        // = (1M * $1) + (100k * $5/M) + (30k * $1.25/M) + (50k * $0.10/M)
        // = $1 + $0.50 + $0.0375 + $0.005
        // = $1.5425
        assert!((cost - 1.5425).abs() < 0.0001);
    }

    #[test]
    fn usage_unknown_model_is_estimated_and_warns_once() {
        let table = PriceTable::from_str(DEFAULT_PRICES).expect("default prices parse");
        let unknown = ModelId("claude-unknown-42".into());
        assert!(table.price_for(&unknown).is_none());
        // First call to warn_once returns true.
        assert!(table.warn_once(&unknown));
        // Second call returns false (already warned).
        assert!(!table.warn_once(&unknown));
        // A different model triggers warning anew.
        let another = ModelId("claude-other-99".into());
        assert!(table.warn_once(&another));
    }

    fn sample_usage() -> Usage {
        Usage {
            input_tokens: 1_000_000,
            output_tokens: 100_000,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            estimated: false,
            cost_usd: 0.0,
            latency_ms: 50,
        }
    }

    #[test]
    fn usage_ledger_row_carries_cost_and_identity() {
        let table = PriceTable::from_str(DEFAULT_PRICES).expect("default prices parse");
        let session = SessionId::new();
        let row = ledger_row(
            session,
            3,
            Job::Main,
            Tier::Code,
            ProviderId::Anthropic,
            ModelId("claude-haiku-4-5".into()),
            Some(cox_protocol::types::Effort::Low),
            sample_usage(),
            &table,
        );
        assert_eq!(row.session_id, session);
        assert_eq!(row.turn, 3);
        assert_eq!(row.job, Job::Main);
        assert_eq!(row.tier, Tier::Code);
        assert_eq!(row.provider, ProviderId::Anthropic);
        assert_eq!(row.model.0, "claude-haiku-4-5");
        // Haiku: 1M input @ $1/M + 100k output @ $5/M = $1.50.
        assert!((row.usage.cost_usd - 1.5).abs() < 0.0001);
        assert!(!row.usage.estimated);
    }

    #[test]
    fn usage_ledger_row_for_unknown_model_is_zero_cost_and_estimated() {
        let table = PriceTable::from_str(DEFAULT_PRICES).expect("default prices parse");
        let row = ledger_row(
            SessionId::new(),
            1,
            Job::Main,
            Tier::Code,
            ProviderId::Local,
            ModelId("some-local-model".into()),
            Some(cox_protocol::types::Effort::High),
            sample_usage(),
            &table,
        );
        // An unpriced model still produces a row — costed 0, flagged estimated.
        assert_eq!(row.usage.cost_usd, 0.0);
        assert!(row.usage.estimated);
        assert_eq!(row.usage.input_tokens, 1_000_000);
    }
}
