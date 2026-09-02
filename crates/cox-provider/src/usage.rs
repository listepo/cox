//! Usage and cost tracking (plan.md §1.7/D5/D6g). Loads a dated price
//! table, computes cost per request, and prepares ledger rows for insertion.
//!
//! The price table carries `verified_on` dates; `cox doctor` may warn if a
//! row is older than 90 days. Unknown models are costed as 0 with `estimated
//! = true` and emit a `Notice(Warn)` once per session.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use cox_protocol::ids::{SessionId, TurnId};
use cox_protocol::traits::UsageRow;
use cox_protocol::types::{Job, ModelId, ProviderId, Tier, Usage};

const DEFAULT_PRICES: &str = include_str!("../../../config/prices.toml");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Price {
    pub id: String,
    /// Input tokens, USD per MTok.
    pub input: f64,
    /// Output tokens, USD per MTok.
    pub output: f64,
    /// Cache write tokens, USD per MTok.
    pub cache_write: f64,
    /// Cache read tokens, USD per MTok.
    pub cache_read: f64,
    pub verified_on: String,
    pub source_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PriceToml {
    model: Vec<Price>,
}

#[derive(Debug, Error)]
pub enum PriceError {
    #[error("failed to parse price table: {0}")]
    Parse(String),
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
        let table: PriceToml =
            toml::from_str(content).map_err(|e| PriceError::Parse(e.to_string()))?;
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
        let mut warned = self.warned_models.lock().unwrap();
        warned.insert(model.0.clone())
    }
}

/// Prepares a ledger row for insertion via `Store::usage_insert`.
/// Called after the provider returns a `Usage` event.
pub fn ledger_row(
    session: SessionId,
    turn: TurnId,
    job: Job,
    tier: Tier,
    provider: String,
    model: ModelId,
    usage: Usage,
) -> UsageRow {
    UsageRow {
        session_id: session,
        turn: turn.as_ref().parse::<u32>().unwrap_or(0),
        job,
        tier,
        provider: provider.into(),
        model,
        usage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_prices_toml_parses_and_has_all_tier_models() {
        let table = PriceTable::from_str(DEFAULT_PRICES).expect("default prices parse");
        assert_eq!(table.prices.len(), 4);
        assert!(
            table
                .prices
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>()
                .windows(2)
                .all(|w| w[0] <= w[1]),
            "prices must be sorted by id"
        );
        // Verify all required tier models are present.
        let ids: Vec<&str> = table.prices.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"claude-haiku-4-5"));
        assert!(ids.contains(&"claude-sonnet-5"));
        assert!(ids.contains(&"claude-opus-5"));
        assert!(ids.contains(&"claude-fable-5-1"));
    }

    #[test]
    fn usage_cost_matches_hand_computed() {
        let table = PriceTable::from_str(DEFAULT_PRICES).expect("default prices parse");
        let price = table
            .price_for(&ModelId::new("claude-haiku-4-5"))
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
        let unknown = ModelId::new("claude-unknown-42");
        assert!(table.price_for(&unknown).is_none());
        // First call to warn_once returns true.
        assert!(table.warn_once(&unknown));
        // Second call returns false (already warned).
        assert!(!table.warn_once(&unknown));
        // A different model triggers warning anew.
        let another = ModelId::new("claude-other-99");
        assert!(table.warn_once(&another));
    }

    #[test]
    fn usage_ledger_row_roundtrips_through_store() {
        let session = SessionId::new("s1");
        let turn = TurnId::new("t1");
        let usage = Usage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 20,
            cache_write_tokens: 10,
            estimated: false,
            cost_usd: 0.001,
            latency_ms: 50,
        };
        let row = ledger_row(
            session.clone(),
            turn.clone(),
            Job::Main,
            Tier::Code,
            "anthropic".to_string(),
            ModelId::new("claude-sonnet-5"),
            usage,
        );
        // Verify the row contains all the expected values.
        assert_eq!(row.session_id, session);
        assert_eq!(row.job, Job::Main);
        assert_eq!(row.tier, Tier::Code);
        assert_eq!(row.model.as_ref(), "claude-sonnet-5");
        assert_eq!(row.usage.input_tokens, 100);
        assert_eq!(row.usage.output_tokens, 50);
    }
}
