//! `cox sessions` (T10.3): list past sessions, grep them through
//! `rollout_fts`, and print one session's record (A13). Pure row shaping
//! (`list_rows`, `age_of`, `detail_of`) stays testable without printing;
//! `run` only formats.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use cox_protocol::{Store as _, UsageRow};
use cox_store::Store;
use cox_store::fts::{RolloutHit, SessionInfo};
use serde::Serialize;

use crate::cli::SessionsArgs;
use crate::stats::effort_tag;

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

/// One session's stored record: the `sessions` row plus what the ledger
/// says about the calls it paid for (A13).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Detail {
    /// Session id.
    pub id: String,
    /// Generated title or `untitled`.
    pub title: String,
    /// Starting directory.
    pub cwd: String,
    /// RFC 3339 timestamp of `session_create`.
    pub started_at: String,
    /// RFC 3339 timestamp of the last finished turn — a session cox is
    /// still running has no other end.
    pub ended_at: String,
    /// Whole seconds between the two, or `None` if either is unparseable.
    pub duration_secs: Option<u64>,
    /// Finished turns.
    pub turns: i64,
    /// Ledger cost in USD.
    pub cost_usd: f64,
    /// One entry per distinct `(provider, model, effort)` the session used.
    pub by_model: Vec<ModelTotal>,
}

/// What one `(provider, model, effort)` combination cost and consumed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelTotal {
    /// Provider tag (`anthropic`, `openai`, a custom section name).
    pub provider: String,
    /// Model id as the provider named it.
    pub model: String,
    /// Effort tag, or `-` for a call recorded before effort was stored.
    pub effort: String,
    /// Provider calls aggregated here.
    pub calls: u64,
    /// Summed usage.
    pub input_tokens: u64,
    /// Summed usage.
    pub output_tokens: u64,
    /// Summed usage.
    pub cache_read_tokens: u64,
    /// Summed usage.
    pub cache_write_tokens: u64,
    /// Summed cost.
    pub cost_usd: f64,
}

/// Joins a session row with its ledger rows; pure, so tests pin the shape.
pub fn detail_of(info: &SessionInfo, usage: &[UsageRow]) -> Detail {
    let mut groups: BTreeMap<(String, String, String), ModelTotal> = BTreeMap::new();
    for row in usage {
        let key = (
            serde_json::to_value(row.provider)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default(),
            row.model.0.clone(),
            effort_tag(&row.effort),
        );
        let total = groups.entry(key.clone()).or_insert(ModelTotal {
            provider: key.0,
            model: key.1,
            effort: key.2,
            calls: 0,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cost_usd: 0.0,
        });
        total.calls += 1;
        total.input_tokens += u64::from(row.usage.input_tokens);
        total.output_tokens += u64::from(row.usage.output_tokens);
        total.cache_read_tokens += u64::from(row.usage.cache_read_tokens);
        total.cache_write_tokens += u64::from(row.usage.cache_write_tokens);
        total.cost_usd += row.usage.cost_usd;
    }
    Detail {
        id: info.id.clone(),
        title: info.title.clone().unwrap_or_else(|| "untitled".into()),
        cwd: info.cwd.clone(),
        started_at: info.created_at.clone(),
        ended_at: info.updated_at.clone(),
        duration_secs: unix_of_rfc3339(&info.updated_at)
            .zip(unix_of_rfc3339(&info.created_at))
            .map(|(end, start)| end.saturating_sub(start)),
        turns: info.turns,
        cost_usd: info.cost_usd,
        by_model: groups.into_values().collect(),
    }
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
    if let Some(id) = args.id.as_deref() {
        let session = id.parse()?;
        let detail = detail_of(
            &store.session_info(&session)?,
            &store.usage_for_session(&session)?,
        );
        return print_detail(&detail, args.json);
    }
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

fn print_detail(detail: &Detail, json: bool) -> anyhow::Result<()> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(detail).unwrap_or_else(|_| "{}".into())
        );
        return Ok(());
    }
    println!("{:<12} {}", "id", detail.id);
    println!("{:<12} {}", "title", detail.title);
    println!("{:<12} {}", "cwd", detail.cwd);
    println!("{:<12} {}", "started", detail.started_at);
    println!("{:<12} {}", "ended", detail.ended_at);
    if let Some(secs) = detail.duration_secs {
        println!(
            "{:<12} {}h{:02}m{:02}s",
            "duration",
            secs / 3600,
            (secs % 3600) / 60,
            secs % 60
        );
    }
    println!("{:<12} {}", "turns", detail.turns);
    println!("{:<12} ${:.4}", "cost", detail.cost_usd);
    if detail.by_model.is_empty() {
        println!("\nNo usage recorded for this session");
        return Ok(());
    }
    println!(
        "\n{:<12} {:<24} {:<7} {:<6} {:<10} {:<10} {:<10} {:<10} COST",
        "PROVIDER", "MODEL", "EFFORT", "CALLS", "INPUT", "OUTPUT", "CACHE R", "CACHE W"
    );
    println!("{}", "-".repeat(110));
    for total in &detail.by_model {
        println!(
            "{:<12} {:<24} {:<7} {:<6} {:<10} {:<10} {:<10} {:<10} ${:.4}",
            truncate(&total.provider, 12),
            truncate(&total.model, 24),
            total.effort,
            total.calls,
            total.input_tokens,
            total.output_tokens,
            total.cache_read_tokens,
            total.cache_write_tokens,
            total.cost_usd,
        );
    }
    Ok(())
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
    fn sessions_detail_groups_by_provider_model_effort() {
        use cox_protocol::{Effort, Job, ModelId, ProviderId, Tier, Usage};

        let mut info = info("abc", "2026-09-03T11:00:00Z");
        info.created_at = "2026-09-03T10:30:00Z".into();
        let usage_row = |effort, input| UsageRow {
            session_id: SessionId::new(),
            turn: 1,
            job: Job::Main,
            tier: Tier::Code,
            provider: ProviderId::Anthropic,
            model: ModelId("claude-sonnet-5".into()),
            effort,
            usage: Usage {
                input_tokens: input,
                output_tokens: 5,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                estimated: false,
                cost_usd: 0.01,
                latency_ms: 1,
            },
        };
        let detail = detail_of(
            &info,
            &[
                usage_row(Some(Effort::High), 100),
                usage_row(Some(Effort::High), 200),
                usage_row(None, 7),
            ],
        );
        assert_eq!(detail.duration_secs, Some(1800));
        // Two effort values, so two groups: the untracked row keeps its own.
        assert_eq!(detail.by_model.len(), 2);
        let high = detail
            .by_model
            .iter()
            .find(|t| t.effort == "high")
            .expect("high group");
        assert_eq!((high.calls, high.input_tokens), (2, 300));
        assert_eq!(
            detail
                .by_model
                .iter()
                .find(|t| t.effort == "-")
                .unwrap()
                .calls,
            1
        );
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
