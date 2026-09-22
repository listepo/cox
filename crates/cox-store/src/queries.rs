//! Ledger aggregations for `cox stats` (T8.4): usage grouped by period,
//! tier and job, plus top tools by archived bytes. Raw SQL lives here —
//! `cox-store` is the only crate that contains SQL (D9); callers group
//! nothing themselves. Also the session tree `/sessions` and `cox sessions`
//! nest forks and handoffs by (T26.3), over Diesel's typed DSL.

use std::collections::{HashMap, HashSet};

use diesel::prelude::*;
use diesel::sql_types::{BigInt, Double, Text};

use cox_protocol::{SessionId, StoreError};

use super::Store;
use crate::fts::SessionInfo;
use crate::schema::sessions;

/// One [`Store::sessions_tree`] row: a session and how deep it nests.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeRow {
    /// The session's ledger columns.
    pub info: SessionInfo,
    /// `0` for a root; a child sits one deeper than its parent.
    pub depth: usize,
}

/// `(id, title, cwd, created_at, updated_at, turns, cost_usd, parent_id)`.
type SessionCols = (
    String,
    Option<String>,
    String,
    String,
    String,
    i32,
    f64,
    Option<String>,
);

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

    /// The newest `limit` sessions with every child listed under its parent
    /// (newest first at each level). A child whose parent is not in the
    /// page is shown as a root, so a limit never hides a session.
    pub fn sessions_tree(&self, limit: i64) -> Result<Vec<TreeRow>, StoreError> {
        let rows: Vec<SessionCols> = {
            let mut conn = self.conn.lock().map_err(|_| StoreError::Io)?;
            sessions::table
                .select((
                    sessions::id,
                    sessions::title,
                    sessions::cwd,
                    sessions::created_at,
                    sessions::updated_at,
                    sessions::turns,
                    sessions::cost_usd,
                    sessions::parent_id,
                ))
                .order_by((sessions::updated_at.desc(), sessions::id.desc()))
                .limit(limit)
                .load(&mut *conn)
                .map_err(|_| StoreError::Sqlite)?
        };
        let present: HashSet<String> = rows.iter().map(|r| r.0.clone()).collect();
        let mut children: HashMap<String, Vec<usize>> = HashMap::new();
        let mut roots = Vec::new();
        for (at, row) in rows.iter().enumerate() {
            match row.7.as_ref().filter(|p| present.contains(*p)) {
                Some(parent) => children.entry(parent.clone()).or_default().push(at),
                None => roots.push(at),
            }
        }
        let mut out = Vec::with_capacity(rows.len());
        let mut seen = HashSet::new();
        // Depth-first, children pushed in reverse so they pop newest first.
        let mut stack: Vec<(usize, usize)> = roots.into_iter().rev().map(|at| (at, 0)).collect();
        while let Some((at, depth)) = stack.pop() {
            if !seen.insert(at) {
                continue;
            }
            let (id, title, cwd, created_at, updated_at, turns, cost_usd, _) = rows[at].clone();
            for &child in children.get(&id).into_iter().flatten().rev() {
                stack.push((child, depth + 1));
            }
            out.push(TreeRow {
                info: SessionInfo {
                    id,
                    title,
                    cwd,
                    created_at,
                    updated_at,
                    turns: i64::from(turns),
                    cost_usd,
                },
                depth,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use cox_protocol::{SessionRow, Store as _};

    use super::*;

    fn create(store: &Store, parent: Option<SessionId>) -> SessionId {
        // `updated_at` has millisecond resolution; keep the order strict.
        std::thread::sleep(std::time::Duration::from_millis(3));
        let id = SessionId::new();
        store
            .session_create(&SessionRow {
                id,
                created_at: String::new(),
                cwd: "/tmp/work".into(),
                project_slug: "work".into(),
                title: None,
                parent_id: parent,
                rollout_path: std::path::PathBuf::new(),
            })
            .expect("session_create");
        id
    }

    /// T26.3: a fork sits under its parent and a fork of the fork one level
    /// deeper; an unrelated session stays a root; a child whose parent fell
    /// outside the page is a root rather than hidden.
    #[test]
    fn sessions_tree_nests_children() {
        let home = tempfile::tempdir().expect("home");
        let store = Store::open(home.path()).expect("store");
        let root = create(&store, None);
        let fork = create(&store, Some(root));
        let grandchild = create(&store, Some(fork));
        let other = create(&store, None);
        let sibling = create(&store, Some(root));

        let tree = store.sessions_tree(10).expect("tree");
        let shape: Vec<(String, usize)> =
            tree.iter().map(|r| (r.info.id.clone(), r.depth)).collect();
        assert_eq!(
            shape,
            vec![
                (other.to_string(), 0),
                (root.to_string(), 0),
                (sibling.to_string(), 1),
                (fork.to_string(), 1),
                (grandchild.to_string(), 2),
            ]
        );

        let page = store.sessions_tree(2).expect("page");
        assert!(
            page.iter().all(|r| r.depth == 0),
            "a parent outside the page makes its child a root: {page:?}"
        );
    }
}
