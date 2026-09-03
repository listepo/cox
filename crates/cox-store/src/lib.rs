//! One SQLite file (`~/.cox/cox.db`): sessions, rollouts (JSONL), the
//! tool-output archive, memory, and the cost ledger. Separate so `cox-core`
//! never opens a file directly; it only calls the `Store`/`Archive` traits
//! this crate implements (plan.md §1.7/D9). The only crate that contains
//! SQL — a workspace test asserts no other crate depends on `diesel`.

pub mod fts;
mod models;
pub mod queries;
mod rollout;
pub mod schema;

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use sha2::{Digest, Sha256};

use cox_protocol::{
    Archive, ArchiveId, ArchivePut, Event, MemoryHit, ModelId, SessionId, SessionRow,
    Store as StoreTrait, StoreError, Usage, UsageRow,
};

use models::{NewArchive, NewMemory, NewSession, UsageDbRow};
use rollout::RolloutWriter;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

/// Inline archive payloads up to this size live in `archive.inline`; larger
/// ones spill to `archive/<id>` under `home` (plan.md §1.7).
const INLINE_ARCHIVE_LIMIT: usize = 16 * 1024;

/// Fsync a rollout file at least this often (plan.md T0.4 step 3): a crash
/// loses at most this many buffered lines.
const ROLLOUT_FSYNC_EVERY: u32 = 16;

/// The concrete `cox_protocol::Store`/`Archive` implementation: one SQLite
/// connection behind a `Mutex` (D9: sync, single-process, no pool) plus a
/// small per-session rollout writer cache.
pub struct Store {
    home: PathBuf,
    conn: Mutex<SqliteConnection>,
    rollouts: Mutex<HashMap<SessionId, RolloutWriter>>,
}

impl Store {
    /// `COX_HOME`'s default when unset: `~/.cox` (plan.md §1.7). Falls back
    /// to a relative `.cox` if the OS reports no home directory at all
    /// (headless containers without `$HOME`), so this never panics.
    pub fn default_home() -> PathBuf {
        directories::BaseDirs::new()
            .map(|d| d.home_dir().join(".cox"))
            .unwrap_or_else(|| PathBuf::from(".cox"))
    }

    fn sessions_dir(&self) -> PathBuf {
        self.home.join("sessions")
    }

    fn archive_dir(&self) -> PathBuf {
        self.home.join("archive")
    }
}

/// Formats the current time as RFC 3339 UTC with millisecond precision
/// (`"2026-09-02T10:11:12.345Z"`), matching the rollout line format
/// (plan.md §1.7). No date/time crate for this: the calendar math is
/// Howard Hinnant's public-domain `civil_from_days` algorithm.
fn now_rfc3339() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let millis_total = now.as_millis();
    let secs = (millis_total / 1000) as i64;
    let millis = (millis_total % 1000) as u32;

    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let hh = sod / 3600;
    let mm = (sod % 3600) / 60;
    let ss = sod % 60;

    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{millis:03}Z")
}

/// Days-since-epoch to a proleptic-Gregorian `(year, month, day)`; public
/// domain (<https://howardhinnant.github.io/date_algorithms.html>).
/// ponytail: assumes `z >= 0` (any real wall-clock "now" since the epoch);
/// upgrade to floor division throughout if this ever needs pre-1970 dates.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Serializes a fieldless/transparent `cox_protocol` type (`Job`, `Tier`,
/// `ProviderId`) to the bare snake_case string its `serde` derive already
/// produces, so the ledger's text columns stay in lock-step with the wire
/// format instead of a hand-maintained second mapping.
fn to_tag<T: serde::Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => s,
        _ => String::new(),
    }
}

/// Inverse of `to_tag`: a stored tag is the bare string form of a unit-variant
/// enum, so it deserializes straight from a JSON string. `None` means the tag
/// no longer names a variant — a corrupt row, not a defaultable one.
fn from_tag<T: serde::de::DeserializeOwned>(s: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl StoreTrait for Store {
    fn open(home: &Path) -> Result<Self, StoreError>
    where
        Self: Sized,
    {
        for dir in [
            home.to_path_buf(),
            home.join("sessions"),
            home.join("archive"),
            home.join("logs"),
            home.join("projects"),
            home.join("cassettes"),
        ] {
            fs::create_dir_all(&dir).map_err(|_| StoreError::Open)?;
        }

        let db_path = home.join("cox.db");
        let mut conn = SqliteConnection::establish(&db_path.to_string_lossy())
            .map_err(|_| StoreError::Open)?;

        conn.batch_execute(
            "PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;",
        )
        .map_err(|_| StoreError::Open)?;

        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|_| StoreError::Migrate { from: 0, to: 1 })?;

        Ok(Self {
            home: home.to_path_buf(),
            conn: Mutex::new(conn),
            rollouts: Mutex::new(HashMap::new()),
        })
    }

    fn session_create(&self, s: &SessionRow) -> Result<(), StoreError> {
        let created = now_rfc3339();
        let new_row = NewSession {
            id: s.id.to_string(),
            created_at: created.clone(),
            updated_at: created,
            cwd: s.cwd.to_string_lossy().into_owned(),
            project_slug: s.project_slug.clone(),
            title: s.title.clone(),
            parent_id: s.parent_id.map(|p| p.to_string()),
            rollout_path: s.rollout_path.to_string_lossy().into_owned(),
            turns: 0,
            cost_usd: 0.0,
            state: "open".to_string(),
        };
        let mut conn = self.conn.lock().map_err(|_| StoreError::Io)?;
        diesel::insert_into(schema::sessions::table)
            .values(&new_row)
            .execute(&mut *conn)
            .map_err(|_| StoreError::Sqlite)?;
        Ok(())
    }

    fn rollout_append(&self, id: &SessionId, ev: &Event) -> Result<u64, StoreError> {
        let mut writers = self.rollouts.lock().map_err(|_| StoreError::Io)?;
        let writer = match writers.entry(*id) {
            Entry::Occupied(o) => o.into_mut(),
            Entry::Vacant(v) => {
                let path = self.sessions_dir().join(format!("{id}.jsonl"));
                let w = RolloutWriter::open(&path).map_err(|_| StoreError::Io)?;
                v.insert(w)
            }
        };
        let seq = writer
            .append(now_rfc3339(), ev)
            .map_err(|_| StoreError::Io)?;
        drop(writers);
        if matches!(ev, Event::TurnDone { .. }) {
            self.finish_session_turn(id)?;
        }
        Ok(seq)
    }

    fn rollout_read(&self, id: &SessionId) -> Result<Vec<Event>, StoreError> {
        Ok(self.rollout_read_with_truncation(id)?.0)
    }

    fn usage_insert(&self, row: &UsageRow) -> Result<(), StoreError> {
        let new_row = UsageDbRow {
            session_id: row.session_id.to_string(),
            turn: row.turn as i32,
            job: to_tag(&row.job),
            tier: to_tag(&row.tier),
            provider: to_tag(&row.provider),
            model: row.model.0.clone(),
            input_tokens: row.usage.input_tokens as i64,
            output_tokens: row.usage.output_tokens as i64,
            cache_read_tokens: row.usage.cache_read_tokens as i64,
            cache_write_tokens: row.usage.cache_write_tokens as i64,
            estimated: row.usage.estimated,
            cost_usd: row.usage.cost_usd,
            latency_ms: row.usage.latency_ms as i64,
            context_tokens: row.usage.context_tokens() as i64,
            created_at: now_rfc3339(),
        };
        let mut conn = self.conn.lock().map_err(|_| StoreError::Io)?;
        diesel::insert_into(schema::usage::table)
            .values(&new_row)
            .execute(&mut *conn)
            .map_err(|_| StoreError::Sqlite)?;
        Ok(())
    }

    fn archive_put(&self, a: &ArchivePut) -> Result<ArchiveId, StoreError> {
        let id = ArchiveId::new();
        let digest = sha256_hex(&a.bytes);

        let (inline, rel_path) = if a.bytes.len() <= INLINE_ARCHIVE_LIMIT {
            (Some(a.bytes.clone()), None)
        } else {
            fs::create_dir_all(self.archive_dir()).map_err(|_| StoreError::Io)?;
            let rel = format!("archive/{id}");
            fs::write(self.home.join(&rel), &a.bytes).map_err(|_| StoreError::Io)?;
            (None, Some(rel))
        };

        let new_row = NewArchive {
            id: id.to_string(),
            session_id: a.session.to_string(),
            call_id: a.call.to_string(),
            tool: a.tool.clone(),
            subject: a.subject.clone(),
            bytes: a.bytes.len() as i64,
            sha256: digest,
            inline,
            path: rel_path,
            created_at: now_rfc3339(),
        };
        let mut conn = self.conn.lock().map_err(|_| StoreError::Io)?;
        diesel::insert_into(schema::archive::table)
            .values(&new_row)
            .execute(&mut *conn)
            .map_err(|_| StoreError::Sqlite)?;
        Ok(id)
    }

    fn archive_get(&self, id: &ArchiveId) -> Result<Vec<u8>, StoreError> {
        let row: models::ArchiveBytes = {
            let mut conn = self.conn.lock().map_err(|_| StoreError::Io)?;
            schema::archive::table
                .filter(schema::archive::id.eq(id.to_string()))
                .select((
                    schema::archive::inline,
                    schema::archive::path,
                    schema::archive::sha256,
                ))
                .first(&mut *conn)
                .map_err(|e| match e {
                    diesel::result::Error::NotFound => StoreError::NotFound,
                    _ => StoreError::Sqlite,
                })?
        };

        let bytes = match (&row.inline, &row.path) {
            (Some(data), _) => data.clone(),
            (None, Some(p)) => fs::read(self.home.join(p)).map_err(|_| StoreError::Io)?,
            (None, None) => {
                return Err(StoreError::Corrupt {
                    path: self.archive_dir().join(id.to_string()),
                });
            }
        };

        if sha256_hex(&bytes) != row.sha256 {
            let bad_path = row
                .path
                .map(|p| self.home.join(p))
                .unwrap_or_else(|| PathBuf::from(format!("inline:{id}")));
            return Err(StoreError::Corrupt { path: bad_path });
        }
        Ok(bytes)
    }

    fn memory_search(&self, q: &str, limit: usize) -> Result<Vec<MemoryHit>, StoreError> {
        // Both tables are written together by `memory_upsert` with a shared
        // rowid, which is what the join below lines up on.
        let q = crate::fts::sanitize_match(q);
        if q.is_empty() {
            return Ok(Vec::new());
        }
        #[derive(diesel::QueryableByName)]
        struct Hit {
            #[diesel(sql_type = diesel::sql_types::Text)]
            name: String,
            #[diesel(sql_type = diesel::sql_types::Text)]
            path: String,
            #[diesel(sql_type = diesel::sql_types::Text)]
            snippet: String,
        }
        let mut conn = self.conn.lock().map_err(|_| StoreError::Io)?;
        let rows: Vec<Hit> = diesel::sql_query(
            "SELECT m.name AS name, m.path AS path, \
             snippet(memory_fts, 1, '', '', '...', 8) AS snippet \
             FROM memory_fts JOIN memory m ON m.rowid = memory_fts.rowid \
             WHERE memory_fts MATCH ? LIMIT ?",
        )
        .bind::<diesel::sql_types::Text, _>(q)
        .bind::<diesel::sql_types::BigInt, _>(limit as i64)
        .load(&mut *conn)
        .map_err(|_| StoreError::Sqlite)?;

        Ok(rows
            .into_iter()
            .map(|h| MemoryHit {
                name: h.name,
                path: PathBuf::from(h.path),
                snippet: h.snippet,
            })
            .collect())
    }

    fn memory_upsert(
        &self,
        project: &str,
        name: &str,
        path: &str,
        kind: &str,
        body: &str,
    ) -> Result<(), StoreError> {
        // The FTS row carries the memory row's rowid explicitly, so the
        // `memory_search` join lines up on re-saves as well as first saves.
        let mut conn = self.conn.lock().map_err(|_| StoreError::Io)?;
        let existing: Option<i32> = schema::memory::table
            .filter(schema::memory::project_slug.eq(project))
            .filter(schema::memory::name.eq(name))
            .select(schema::memory::id)
            .first(&mut *conn)
            .optional()
            .map_err(|_| StoreError::Sqlite)?;
        let rowid = match existing {
            Some(id) => {
                diesel::update(schema::memory::table.filter(schema::memory::id.eq(id)))
                    .set((
                        schema::memory::path.eq(path),
                        schema::memory::kind.eq(kind),
                        schema::memory::updated_at.eq(now_rfc3339()),
                    ))
                    .execute(&mut *conn)
                    .map_err(|_| StoreError::Sqlite)?;
                diesel::sql_query("DELETE FROM memory_fts WHERE rowid = ?")
                    .bind::<diesel::sql_types::BigInt, _>(i64::from(id))
                    .execute(&mut *conn)
                    .map_err(|_| StoreError::Sqlite)?;
                i64::from(id)
            }
            None => {
                diesel::insert_into(schema::memory::table)
                    .values(&NewMemory {
                        project_slug: project.to_string(),
                        name: name.to_string(),
                        path: path.to_string(),
                        kind: kind.to_string(),
                        updated_at: now_rfc3339(),
                    })
                    .execute(&mut *conn)
                    .map_err(|_| StoreError::Sqlite)?;
                diesel::select(diesel::dsl::sql::<diesel::sql_types::BigInt>(
                    "last_insert_rowid()",
                ))
                .get_result(&mut *conn)
                .map_err(|_| StoreError::Sqlite)?
            }
        };
        diesel::sql_query(
            "INSERT INTO memory_fts(rowid, name, body, project_slug) VALUES(?,?,?,?)",
        )
        .bind::<diesel::sql_types::BigInt, _>(rowid)
        .bind::<diesel::sql_types::Text, _>(name)
        .bind::<diesel::sql_types::Text, _>(body)
        .bind::<diesel::sql_types::Text, _>(project)
        .execute(&mut *conn)
        .map_err(|_| StoreError::Sqlite)?;
        Ok(())
    }

    fn rollout_index(&self, session: &SessionId, turn: u32, text: &str) -> Result<(), StoreError> {
        self.rollout_index_text(session, turn, text)
    }
}

/// Narrower async view of the archive methods for `Tool::call` (D6a); wraps
/// the same sync store, no `spawn_blocking` needed behind a `Mutex` (T0.4).
#[async_trait]
impl Archive for Store {
    async fn put(&self, put: ArchivePut) -> Result<ArchiveId, StoreError> {
        StoreTrait::archive_put(self, &put)
    }

    async fn get(&self, id: &ArchiveId) -> Result<Vec<u8>, StoreError> {
        StoreTrait::archive_get(self, id)
    }
}

/// Public query methods for surfaces like `cox stats`.
impl Store {
    /// Updates the denormalized session counters at a durable turn boundary.
    /// Usage is recorded before `TurnDone`, so the ledger is the source of
    /// truth for the stored cost rather than a second accumulator.
    fn finish_session_turn(&self, id: &SessionId) -> Result<(), StoreError> {
        let session_id = id.to_string();
        let mut conn = self.conn.lock().map_err(|_| StoreError::Io)?;
        let cost: Option<f64> = schema::usage::table
            .filter(schema::usage::session_id.eq(&session_id))
            .select(diesel::dsl::sum(schema::usage::cost_usd))
            .first(&mut *conn)
            .map_err(|_| StoreError::Sqlite)?;
        diesel::update(schema::sessions::table.filter(schema::sessions::id.eq(session_id)))
            .set((
                schema::sessions::turns.eq(schema::sessions::turns + 1),
                schema::sessions::cost_usd.eq(cost.unwrap_or_default()),
                schema::sessions::updated_at.eq(now_rfc3339()),
            ))
            .execute(&mut *conn)
            .map_err(|_| StoreError::Sqlite)?;
        Ok(())
    }

    /// Reads a rollout and reports whether a crash-truncated final line was
    /// discarded. Surfaces use this to warn without treating a recoverable
    /// tail as a corrupt session.
    pub fn rollout_read_with_truncation(
        &self,
        id: &SessionId,
    ) -> Result<(Vec<Event>, bool), StoreError> {
        let path = self.sessions_dir().join(format!("{id}.jsonl"));
        rollout::read_lines(&path).map_err(|_| StoreError::Io)
    }

    /// The most recently created session for `cwd`, used by `cox run --continue`.
    pub fn latest_session_for_cwd(&self, cwd: &Path) -> Result<SessionId, StoreError> {
        let mut conn = self.conn.lock().map_err(|_| StoreError::Io)?;
        let id: Option<String> = schema::sessions::table
            .filter(schema::sessions::cwd.eq(cwd.to_string_lossy().as_ref()))
            // ULIDs make the tie-breaker chronological too: two sessions can
            // share the millisecond-resolution `created_at` timestamp.
            .order_by((
                schema::sessions::created_at.desc(),
                schema::sessions::id.desc(),
            ))
            .select(schema::sessions::id)
            .first(&mut *conn)
            .optional()
            .map_err(|_| StoreError::Sqlite)?;
        let id = id.ok_or(StoreError::NotFound)?;
        id.parse().map_err(|_| StoreError::Corrupt {
            path: self.home.join("cox.db"),
        })
    }

    /// Every usage row for one session, in turn order — what
    /// `cox stats --session <id>` prints (T1.7).
    pub fn usage_for_session(&self, session_id: &SessionId) -> Result<Vec<UsageRow>, StoreError> {
        let mut conn = self.conn.lock().map_err(|_| StoreError::Io)?;
        let rows: Vec<UsageDbRow> = schema::usage::table
            .filter(schema::usage::session_id.eq(session_id.to_string()))
            .order_by(schema::usage::turn.asc())
            .select(UsageDbRow::as_select())
            .load(&mut *conn)
            .map_err(|_| StoreError::Sqlite)?;
        drop(conn);

        rows.into_iter()
            .map(|r| {
                let corrupt = || StoreError::Corrupt {
                    path: self.home.join("cox.db"),
                };
                Ok(UsageRow {
                    session_id: r.session_id.parse().map_err(|_| corrupt())?,
                    turn: r.turn as u32,
                    job: from_tag(&r.job).ok_or_else(corrupt)?,
                    tier: from_tag(&r.tier).ok_or_else(corrupt)?,
                    provider: from_tag(&r.provider).ok_or_else(corrupt)?,
                    model: ModelId(r.model),
                    usage: Usage {
                        input_tokens: r.input_tokens as u32,
                        output_tokens: r.output_tokens as u32,
                        cache_read_tokens: r.cache_read_tokens as u32,
                        cache_write_tokens: r.cache_write_tokens as u32,
                        estimated: r.estimated,
                        cost_usd: r.cost_usd,
                        latency_ms: r.latency_ms as u64,
                    },
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use cox_protocol::{CallId, Job, ModelId, ProviderId, Tier, Usage};

    use super::*;

    fn usage_row(session_id: SessionId, input_tokens: u32) -> UsageRow {
        UsageRow {
            session_id,
            turn: 1,
            job: Job::Main,
            tier: Tier::Code,
            provider: ProviderId::Anthropic,
            model: ModelId("claude-sonnet-5".into()),
            usage: Usage {
                input_tokens,
                output_tokens: 10,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                estimated: false,
                cost_usd: 0.01,
                latency_ms: 100,
            },
        }
    }

    #[test]
    fn migrations_are_idempotent() {
        let dir = tempfile::tempdir().expect("tempdir");
        Store::open(dir.path()).expect("first open");
        Store::open(dir.path()).expect("second open (re-runs pending migrations, none pending)");
    }

    #[test]
    fn schema_snapshot_matches() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open store");

        #[derive(diesel::QueryableByName)]
        struct Row {
            #[diesel(sql_type = diesel::sql_types::Text)]
            name: String,
            #[diesel(sql_type = diesel::sql_types::Text)]
            sql: String,
        }
        let rows: Vec<Row> = {
            let mut conn = store.conn.lock().expect("lock");
            diesel::sql_query(
                "SELECT name, sql FROM sqlite_master \
                 WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' \
                 AND name != '__diesel_schema_migrations' ORDER BY name",
            )
            .load(&mut *conn)
            .expect("query sqlite_master")
        };

        let rendered: String = rows
            .iter()
            .map(|r| format!("-- {}\n{};\n\n", r.name, r.sql))
            .collect();
        insta::assert_snapshot!(rendered);
    }

    #[test]
    fn memory_upsert_and_search_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open store");
        store
            .memory_upsert(
                "proj",
                "auth-flow",
                "auth-flow.md",
                "decision",
                "Login goes through auth.rs with sessions.",
            )
            .expect("upsert");
        let hits = store.memory_search("sessions auth", 5).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "auth-flow");
        assert_eq!(hits[0].path, PathBuf::from("auth-flow.md"));
        // Re-saving replaces both rows: the join stays aligned, so the old
        // terms stop matching and the new ones start, with no ghost rows.
        store
            .memory_upsert(
                "proj",
                "auth-flow",
                "auth-flow.md",
                "fact",
                "Completely different words here.",
            )
            .expect("re-upsert");
        assert!(
            store
                .memory_search("sessions", 5)
                .expect("search")
                .is_empty()
        );
        let hits = store.memory_search("different words", 5).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "auth-flow");
    }

    #[test]
    fn archive_roundtrip_inline_and_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open store");
        let session = SessionId::new();

        let small = ArchivePut {
            session,
            call: CallId::new(),
            tool: "read".into(),
            subject: Some("src/lib.rs".into()),
            bytes: b"hello".to_vec(),
        };
        let small_id = store.archive_put(&small).expect("put inline");
        let back = store.archive_get(&small_id).expect("get inline");
        assert_eq!(back, b"hello");
        assert!(
            !dir.path()
                .join("archive")
                .join(small_id.to_string())
                .exists()
        );

        let big = ArchivePut {
            session,
            call: CallId::new(),
            tool: "bash".into(),
            subject: None,
            bytes: vec![7u8; INLINE_ARCHIVE_LIMIT + 1],
        };
        let big_id = store.archive_put(&big).expect("put file");
        let back_big = store.archive_get(&big_id).expect("get file");
        assert_eq!(back_big, vec![7u8; INLINE_ARCHIVE_LIMIT + 1]);
        assert!(dir.path().join("archive").join(big_id.to_string()).exists());
    }

    #[test]
    fn archive_get_detects_corrupt_bytes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open store");
        let put = ArchivePut {
            session: SessionId::new(),
            call: CallId::new(),
            tool: "bash".into(),
            subject: None,
            bytes: vec![9u8; INLINE_ARCHIVE_LIMIT + 1],
        };
        let id = store.archive_put(&put).expect("put file");
        fs::write(dir.path().join("archive").join(id.to_string()), b"tampered")
            .expect("tamper with archived file");

        let err = store.archive_get(&id).expect_err("sha256 mismatch");
        assert!(matches!(err, StoreError::Corrupt { .. }));
    }

    #[test]
    fn rollout_survives_truncated_tail() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open store");
        let session = SessionId::new();
        let turn_done = Event::TurnDone {
            turn: cox_protocol::TurnId::new(),
            stop: cox_protocol::StopReason::EndTurn,
        };
        store.rollout_append(&session, &turn_done).expect("append");

        // Truncate the file mid-line to simulate a crash during the write.
        let path = dir.path().join("sessions").join(format!("{session}.jsonl"));
        let full = fs::read(&path).expect("read rollout");
        fs::write(&path, &full[..full.len() - 3]).expect("truncate");

        let events = store
            .rollout_read(&session)
            .expect("read tolerates truncation");
        assert!(events.is_empty());
    }

    #[test]
    fn latest_session_for_cwd_excludes_other_directories() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open store");
        let cwd = PathBuf::from("/workspace/cox");
        let older = SessionId::new();

        for (id, row_cwd) in [
            (older, cwd.clone()),
            (SessionId::new(), PathBuf::from("/elsewhere")),
        ] {
            store
                .session_create(&SessionRow {
                    id,
                    created_at: String::new(),
                    cwd: row_cwd,
                    project_slug: String::new(),
                    title: None,
                    parent_id: None,
                    rollout_path: PathBuf::new(),
                })
                .expect("session");
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
        let newer = SessionId::new();
        store
            .session_create(&SessionRow {
                id: newer,
                created_at: String::new(),
                cwd: cwd.clone(),
                project_slug: String::new(),
                title: None,
                parent_id: None,
                rollout_path: PathBuf::new(),
            })
            .expect("session");

        assert_eq!(store.latest_session_for_cwd(&cwd).expect("latest"), newer);
    }

    #[test]
    fn usage_insert_and_sum() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::open(dir.path()).expect("open store");
        let session = SessionId::new();
        store
            .session_create(&SessionRow {
                id: session,
                created_at: now_rfc3339(),
                cwd: PathBuf::from("/tmp"),
                project_slug: "cox".into(),
                title: None,
                parent_id: None,
                rollout_path: dir.path().join("sessions").join(format!("{session}.jsonl")),
            })
            .expect("session_create (usage.session_id has a foreign key on sessions.id)");

        store
            .usage_insert(&usage_row(session, 100))
            .expect("insert 1");
        store
            .usage_insert(&usage_row(session, 250))
            .expect("insert 2");

        let mut conn = store.conn.lock().expect("lock");
        let input_tokens: Vec<i64> = schema::usage::table
            .filter(schema::usage::session_id.eq(session.to_string()))
            .select(schema::usage::input_tokens)
            .load(&mut *conn)
            .expect("load usage rows");
        let total: i64 = input_tokens.iter().sum();
        assert_eq!(total, 350);
        drop(conn);

        store
            .rollout_append(
                &session,
                &Event::TurnDone {
                    turn: cox_protocol::TurnId::new(),
                    stop: cox_protocol::StopReason::EndTurn,
                },
            )
            .expect("turn done");
        let mut conn = store.conn.lock().expect("lock");
        let (turns, cost): (i32, f64) = schema::sessions::table
            .filter(schema::sessions::id.eq(session.to_string()))
            .select((schema::sessions::turns, schema::sessions::cost_usd))
            .first(&mut *conn)
            .expect("session counters");
        assert_eq!(turns, 1);
        assert_eq!(cost, 0.02);
    }
}
