//! Ledger aggregations for `cox stats` (T8.4): usage grouped by period,
//! tier and job, plus top tools by archived bytes. Raw SQL lives here —
//! `cox-store` is the only crate that contains SQL (D9); callers group
//! nothing themselves.

use diesel::prelude::*;
use diesel::sql_types::{BigInt, Double, Text};

use cox_protocol::{SessionId, StoreError};

use super::Store;

/// One `(period, tier, job)` aggregate over the `usage` ledger. `period` is
/// a day (`2026-09-03`), a month (`2026-09`) or `all`, depending on the
/// [`Period`] asked for.
#[derive(Debug, Clone, PartialEq, QueryableByName)]
pub struct TierJobRow {
    /// The time bucket (day, month or `all`).
    #[diesel(sql_type = Text)]
    pub period: String,
    /// Tier tag (`cheap`, `code`, `think`).
    #[diesel(sql_type = Text)]
    pub tier: String,
    /// Job tag (`main`, `compact`, …).
    #[diesel(sql_type = Text)]
    pub job: String,
    /// Provider calls in the bucket.
    #[diesel(sql_type = BigInt)]
    pub calls: i64,
    /// Summed tokens and cost.
    #[diesel(sql_type = BigInt)]
    pub input_tokens: i64,
    /// Summed tokens and cost.
    #[diesel(sql_type = BigInt)]
    pub output_tokens: i64,
    /// Summed tokens and cost.
    #[diesel(sql_type = BigInt)]
    pub cache_read_tokens: i64,
    /// Summed tokens and cost.
    #[diesel(sql_type = BigInt)]
    pub cache_write_tokens: i64,
    /// Summed tokens and cost.
    #[diesel(sql_type = BigInt)]
    pub context_tokens: i64,
    /// Summed tokens and cost.
    #[diesel(sql_type = Double)]
    pub cost_usd: f64,
}

/// One tool's archived-byte total over the `archive` table.
#[derive(Debug, Clone, PartialEq, QueryableByName)]
pub struct ToolBytesRow {
    /// Tool name (`read`, `bash`, `mcp__srv__tool`, …).
    #[diesel(sql_type = Text)]
    pub tool: String,
    /// Total archived bytes.
    #[diesel(sql_type = BigInt)]
    pub bytes: i64,
    /// Archived calls.
    #[diesel(sql_type = BigInt)]
    pub calls: i64,
}

/// Which time bucket [`Store::usage_by_period`] groups by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    /// One row set per day (`YYYY-MM-DD`).
    Day,
    /// One row set per month (`YYYY-MM`).
    Month,
    /// A single `all` bucket over the whole ledger.
    All,
}

impl Store {
    /// Usage grouped by period, tier and job, oldest bucket first.
    pub fn usage_by_period(&self, period: Period) -> Result<Vec<TierJobRow>, StoreError> {
        // Fixed strings only — no user input reaches the format.
        let bucket = match period {
            Period::Day => "date(created_at)",
            Period::Month => "strftime('%Y-%m', created_at)",
            Period::All => "'all'",
        };
        let sql = format!(
            "SELECT {bucket} AS period, tier, job, COUNT(*) AS calls, \
             SUM(input_tokens) AS input_tokens, SUM(output_tokens) AS output_tokens, \
             SUM(cache_read_tokens) AS cache_read_tokens, \
             SUM(cache_write_tokens) AS cache_write_tokens, \
             SUM(context_tokens) AS context_tokens, SUM(cost_usd) AS cost_usd \
             FROM usage GROUP BY period, tier, job ORDER BY period, tier, job"
        );
        let mut conn = self.conn.lock().map_err(|_| StoreError::Io)?;
        diesel::sql_query(sql)
            .load(&mut *conn)
            .map_err(|_| StoreError::Sqlite)
    }

    /// Tools ordered by archived bytes, most first. `session` scopes the
    /// totals to one session; `None` totals the whole archive.
    pub fn top_tools(
        &self,
        session: Option<&SessionId>,
        limit: i64,
    ) -> Result<Vec<ToolBytesRow>, StoreError> {
        let mut conn = self.conn.lock().map_err(|_| StoreError::Io)?;
        match session {
            Some(id) => diesel::sql_query(
                "SELECT tool, SUM(bytes) AS bytes, COUNT(*) AS calls FROM archive \
                 WHERE session_id = ? GROUP BY tool ORDER BY bytes DESC LIMIT ?",
            )
            .bind::<Text, _>(id.to_string())
            .bind::<BigInt, _>(limit)
            .load(&mut *conn)
            .map_err(|_| StoreError::Sqlite),
            None => diesel::sql_query(
                "SELECT tool, SUM(bytes) AS bytes, COUNT(*) AS calls FROM archive \
                 GROUP BY tool ORDER BY bytes DESC LIMIT ?",
            )
            .bind::<BigInt, _>(limit)
            .load(&mut *conn)
            .map_err(|_| StoreError::Sqlite),
        }
    }
}
