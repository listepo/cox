//! `Queryable`/`Insertable` row types for `schema.rs`'s non-virtual tables
//! (plan.md §1.7/D9). Every field is a plain SQL-shaped type (`String`,
//! `i64`, ...); the `Store` impl in `lib.rs` converts to/from the
//! `cox_protocol` types at the boundary.

use diesel::prelude::*;

use crate::schema::{archive, sessions, usage};

#[derive(Insertable)]
#[diesel(table_name = sessions)]
pub(crate) struct NewSession {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
    pub cwd: String,
    pub project_slug: String,
    pub title: Option<String>,
    pub parent_id: Option<String>,
    pub rollout_path: String,
    pub turns: i32,
    pub cost_usd: f64,
    pub state: String,
}

#[derive(Insertable)]
#[diesel(table_name = usage)]
pub(crate) struct NewUsage {
    pub session_id: String,
    pub turn: i32,
    pub job: String,
    pub tier: String,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub estimated: bool,
    pub cost_usd: f64,
    pub latency_ms: i64,
    pub context_tokens: i64,
    pub created_at: String,
}

#[derive(Insertable)]
#[diesel(table_name = archive)]
pub(crate) struct NewArchive {
    pub id: String,
    pub session_id: String,
    pub call_id: String,
    pub tool: String,
    pub subject: Option<String>,
    pub bytes: i64,
    pub sha256: String,
    pub inline: Option<Vec<u8>>,
    pub path: Option<String>,
    pub created_at: String,
}

/// The columns `Store::archive_get` needs to resolve and verify a payload;
/// selected explicitly rather than the whole row (nothing reads the rest).
#[derive(Queryable)]
pub(crate) struct ArchiveBytes {
    pub inline: Option<Vec<u8>>,
    pub path: Option<String>,
    pub sha256: String,
}
