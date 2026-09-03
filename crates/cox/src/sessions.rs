//! `cox sessions` (T10.3): list past sessions and grep them through
//! `rollout_fts`. Pure row shaping (`list_rows`, `age_of`) stays testable
//! without printing; `run` only formats.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use cox_protocol::Store as _;
use cox_store::Store;
use cox_store::fts::{RolloutHit, SessionInfo};
use serde::Serialize;

use crate::cli::SessionsArgs;

/// One listed session for humans and `--json`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    /// Session id.
    pub id: String,
    /// Generated title or `untitled`.
    pub title: String,
    /// Starting directory.
    pub cwd: String,
    /// Coarse age (`now`, `12m`, `5h`, `3d`, or a date).
    pub age: String,
    /// Finished turns.
    pub turns: i64,
    /// Ledger cost in USD.
    pub cost_usd: f64,
}

/// Shapes listing rows with `now_secs` injected, so tests pin time.
pub fn list_rows(infos: &[SessionInfo], now_secs: u64) -> Vec<Row> {
    infos
        .iter()
        .map(|info| Row {
            id: info.id.clone(),
            title: info.title.clone().unwrap_or_else(|| "untitled".into()),
            cwd: info.cwd.clone(),
            age: age_of(&info.updated_at, now_secs),
            turns: info.turns,
            cost_usd: info.cost_usd,
        })
        .collect()
}

/// Coarse human age of an RFC 3339 timestamp; `?` when unparseable.
pub fn age_of(updated_at: &str, now_secs: u64) -> String {
    let Some(then) = unix_of_rfc3339(updated_at) else {
        return "?".into();
    };
    let secs = now_secs.saturating_sub(then);
    if secs < 90 {
        "now".into()
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 48 * 3600 {
        format!("{}h", secs / 3600)
    } else if secs < 45 * 86400 {
        format!("{}d", secs / 86400)
    } else {
        updated_at.chars().take(10).collect()
    }
}

/// `YYYY-MM-DDTHH:MM:SS` prefix to unix seconds (UTC); fraction and zone
/// suffixes are ignored, like the writer emits them.
fn unix_of_rfc3339(text: &str) -> Option<u64> {
    let date = text.get(..10)?;
    let time = text.get(11..19)?;
    let number = |s: &str| s.parse::<i64>().ok();
    let days = days_from_civil(
        number(date.get(..4)?)?,
        number(date.get(5..7)?)?,
        number(date.get(8..10)?)?,
    );
    let secs =
        number(time.get(..2)?)? * 3600 + number(time.get(3..5)?)? * 60 + number(time.get(6..8)?)?;
    u64::try_from(days * 86400 + secs).ok()
}

/// Howard Hinnant's public-domain days-from-civil algorithm (the inverse of
/// the store's timestamp formatter).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `cox sessions` dispatch over `--grep`/`--json`/`--limit` (§1.12).
pub fn run(home: &Path, args: &SessionsArgs) -> anyhow::Result<()> {
    let store = Store::open(home)?;
    let limit = args.limit.unwrap_or(20).max(1) as i64;
    if let Some(q) = args.grep.as_deref() {
        let hits = store.rollout_search(q, limit * 5)?;
        let ids: Vec<String> = {
            let mut seen = std::collections::HashSet::new();
            hits.iter()
                .map(|h| h.session_id.clone())
                .filter(|id| seen.insert(id.clone()))
                .collect()
        };
        let mut infos = store.list_sessions(1000)?;
        infos.retain(|info| ids.contains(&info.id));
        infos.truncate(limit as usize);
        return print_sessions(&list_rows(&infos, now_secs()), Some(&hits), args.json);
    }
    let infos = store.list_sessions(limit)?;
    if infos.is_empty() {
        println!("No sessions found");
        return Ok(());
    }
    print_sessions(&list_rows(&infos, now_secs()), None, args.json)
}

fn print_sessions(rows: &[Row], hits: Option<&[RolloutHit]>, json: bool) -> anyhow::Result<()> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "sessions": rows,
                "hits": hits.unwrap_or(&[]),
            }))
            .unwrap_or_else(|_| "{}".into())
        );
        return Ok(());
    }
    println!(
        "{:<26} {:<20} {:<24} {:<6} {:<5} COST",
        "ID", "TITLE", "CWD", "AGE", "TURNS"
    );
    println!("{}", "-".repeat(100));
    for row in rows {
        println!(
            "{:<26} {:<20} {:<24} {:<6} {:<5} ${:.4}",
            row.id,
            truncate(&row.title, 20),
            truncate(&row.cwd, 24),
            row.age,
            row.turns,
            row.cost_usd,
        );
    }
    if let Some(hits) = hits {
        for hit in hits {
            println!("  turn {}: {}", hit.turn, hit.snippet);
        }
    }
    Ok(())
}

fn truncate(text: &str, width: usize) -> String {
    if text.len() <= width {
        text.to_string()
    } else {
        format!(
            "{}…",
            text.chars()
                .take(width.saturating_sub(1))
                .collect::<String>()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cox_protocol::{SessionId, SessionRow};

    fn info(id: &str, updated_at: &str) -> SessionInfo {
        SessionInfo {
            id: id.into(),
            title: None,
            cwd: "/tmp/work".into(),
            created_at: updated_at.into(),
            updated_at: updated_at.into(),
            turns: 3,
            cost_usd: 0.02,
        }
    }

    #[test]
    fn sessions_age_buckets() {
        // 2026-09-03T00:00:00Z == 20322 days * 86400.
        let now = 20699 * 86400 + 12 * 3600;
        assert_eq!(age_of("2026-09-03T11:59:00Z", now), "now");
        assert_eq!(age_of("2026-09-03T11:30:00Z", now), "30m");
        assert_eq!(age_of("2026-09-03T07:00:00Z", now), "5h");
        assert_eq!(age_of("2026-08-25T12:00:00Z", now), "9d");
        assert_eq!(age_of("2026-06-01T00:00:00Z", now), "2026-06-01");
        assert_eq!(age_of("garbage", now), "?");
    }

    #[test]
    fn sessions_list_rows_shape() {
        let rows = list_rows(
            &[info("abc", "2026-09-03T11:00:00Z")],
            20699 * 86400 + 12 * 3600,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "untitled");
        assert_eq!(rows[0].age, "1h");
        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&rows).unwrap()).unwrap();
        assert_eq!(value[0]["turns"], 3);
    }

    fn session_row(id: SessionId) -> SessionRow {
        SessionRow {
            id,
            created_at: String::new(),
            cwd: "/tmp/work".into(),
            project_slug: "work".into(),
            title: Some("auth work".into()),
            parent_id: None,
            rollout_path: std::path::PathBuf::from("/tmp/work.jsonl"),
        }
    }

    #[test]
    fn sessions_list_limits_rows() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let (a, b) = (SessionId::new(), SessionId::new());
        store.session_create(&session_row(a)).unwrap();
        store.session_create(&session_row(b)).unwrap();
        assert_eq!(store.list_sessions(10).unwrap().len(), 2);
        assert_eq!(store.list_sessions(1).unwrap().len(), 1);
    }

    #[test]
    fn sessions_grep_finds_indexed_text() {
        use cox_protocol::traits::Store as _;
        let home = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let id = SessionId::new();
        store.session_create(&session_row(id)).unwrap();
        store
            .rollout_index_text(&id, 1, "the auth module handles login with sessions")
            .unwrap();
        store
            .rollout_index_text(&id, 2, "unrelated filler words here")
            .unwrap();
        let hits = store.rollout_search("auth login", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session_id, id.to_string());
        assert_eq!(hits[0].turn, 1);
        assert!(hits[0].snippet.contains("auth"), "{}", hits[0].snippet);
        assert!(
            store
                .rollout_search("no-such-term-xyz", 10)
                .unwrap()
                .is_empty()
        );
    }
}
