//! One SQLite file (`~/.cox/cox.db`): sessions, rollouts (JSONL), the
//! tool-output archive, memory, and the cost ledger. Separate so `cox-core`
//! never opens a file directly; it only calls the `Store`/`Archive` traits
//! this crate implements (plan.md §1.7/D9). The only crate that contains
//! SQL — a workspace test asserts no other crate depends on `diesel`.

mod models;
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
    Archive, ArchiveId, ArchivePut, Event, MemoryHit, SessionId, SessionRow, Store as StoreTrait,
    StoreError, UsageRow,
};

use models::{NewArchive, NewSession, NewUsage};
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

fn from_tag<T: serde::de::DeserializeOwned>(s: &str) -> T {
    // Reconstruct the JSON value that to_tag would have produced from T.
    // to_tag serializes to a bare string, so we deserialize from that string directly.
    serde_json::from_value(serde_json::Value::String(s.to_string()))
        .unwrap_or_else(|_| serde_json::from_value(serde_json::json!(s)).unwrap_or_default())
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
        writer.append(now_rfc3339(), ev).map_err(|_| StoreError::Io)
    }

    fn rollout_read(&self, id: &SessionId) -> Result<Vec<Event>, StoreError> {
        let path = self.sessions_dir().join(format!("{id}.jsonl"));
        let (events, _truncated) = rollout::read_lines(&path).map_err(|_| StoreError::Io)?;
        Ok(events)
    }

    fn usage_insert(&self, row: &UsageRow) -> Result<(), StoreError> {
        let new_row = NewUsage {
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
        // ponytail: no writer populates `memory`/`memory_fts` yet (memory
        // ingestion is a later task), so this is a real but unexercised FTS5
        // query. It joins by rowid, which only lines up once a future
        // inserter writes both tables together; fine while the table is
        // empty. Upgrade once T-memory-ingest exists.
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
    /// Retrieves all usage rows for a session, ordered by turn.
    /// Used by `cox stats --session <id>` to display per-turn costs (T1.7).
    pub fn usage_for_session(&self, session_id: &SessionId) -> Result<Vec<UsageRow>, StoreError> {
        use diesel::prelude::*;
        let mut conn = self.conn.lock().map_err(|_| StoreError::Io)?;
        let rows: Vec<(
            String,
            i32,
            String,
            String,
            String,
            String,
            i64,
            i64,
            i64,
            i64,
            bool,
            f64,
            i64,
        )> = schema::usage::table
            .filter(schema::usage::session_id.eq(session_id.to_string()))
            .order_by(schema::usage::turn.asc())
            .select((
                schema::usage::session_id,
                schema::usage::turn,
                schema::usage::job,
                schema::usage::tier,
                schema::usage::provider,
                schema::usage::model,
                schema::usage::input_tokens,
                schema::usage::output_tokens,
                schema::usage::cache_read_tokens,
                schema::usage::cache_write_tokens,
                schema::usage::estimated,
                schema::usage::cost_usd,
                schema::usage::latency_ms,
            ))
            .load(&mut *conn)
            .map_err(|_| StoreError::Sqlite)?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    session_id,
                    turn,
                    job,
                    tier,
                    provider,
                    model,
                    input,
                    output,
                    cache_read,
                    cache_write,
                    estimated,
                    cost_usd,
                    latency_ms,
                )| {
                    UsageRow {
                        session_id: SessionId::new(&session_id),
                        turn: turn as u32,
                        job: from_tag(&job),
                        tier: from_tag(&tier),
                        provider: provider.into(),
                        model: ModelId::new(&model),
                        usage: Usage {
                            input_tokens: input as u32,
                            output_tokens: output as u32,
                            cache_read_tokens: cache_read as u32,
                            cache_write_tokens: cache_write as u32,
                            estimated,
                            cost_usd,
                            latency_ms: latency_ms as u64,
                        },
                    }
                },
            )
            .collect())
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
    }
}
