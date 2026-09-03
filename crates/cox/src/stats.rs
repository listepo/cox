//! `cox stats --session <id>` command: print per-turn costs and usage statistics
//! from the ledger (plan.md T1.7); `--cache` lists cache diagnostics (T8.3).
//! Reads from the Store and formats human-readable output.

use std::path::Path;

use cox_protocol::{Event, SessionId, Store as _};
use cox_store::Store;

pub fn run(home: &Path, session_id: &str, cache_only: bool) -> anyhow::Result<()> {
    let store = Store::open(home)?;
    let session: SessionId = session_id.parse()?;
    let rows = store.usage_for_session(&session)?;

    if rows.is_empty() {
        println!("No usage records found for session {}", session_id);
        return Ok(());
    }

    if cache_only {
        return run_cache(&store, &session, &rows);
    }

    // Print a header.
    println!(
        "{:<5} {:<20} {:<12} {:<12} {:<12} {:<12} {:<12} {:<10}",
        "Turn", "Model", "Input", "Output", "Cache R", "Cache W", "Cost", "Latency"
    );
    println!("{}", "-".repeat(105));

    // Print each row.
    for row in &rows {
        println!(
            "{:<5} {:<20} {:<12} {:<12} {:<12} {:<12} ${:<11.4} {:<10}ms",
            row.turn,
            row.model.0,
            row.usage.input_tokens,
            row.usage.output_tokens,
            row.usage.cache_read_tokens,
            row.usage.cache_write_tokens,
            row.usage.cost_usd,
            row.usage.latency_ms,
        );
    }

    // Print a summary line.
    let total_cost: f64 = rows.iter().map(|r| r.usage.cost_usd).sum();
    let total_input: u32 = rows.iter().map(|r| r.usage.input_tokens).sum();
    let total_output: u32 = rows.iter().map(|r| r.usage.output_tokens).sum();
    let total_cache_read: u32 = rows.iter().map(|r| r.usage.cache_read_tokens).sum();
    let total_cache_write: u32 = rows.iter().map(|r| r.usage.cache_write_tokens).sum();

    println!("{}", "-".repeat(105));
    println!(
        "{:<5} {:<20} {:<12} {:<12} {:<12} {:<12} ${:<11.4}",
        "TOTAL", "", total_input, total_output, total_cache_read, total_cache_write, total_cost,
    );

    Ok(())
}

/// `cox stats --cache`: per-turn read ratio plus the miss `Notice`s the core
/// emitted (T8.3 step 3). A turn with ratio 0 after a non-zero one is the
/// ledger side of the same miss the rollout names by block.
fn run_cache(
    store: &Store,
    session: &SessionId,
    rows: &[cox_protocol::UsageRow],
) -> anyhow::Result<()> {
    println!(
        "{:<5} {:<8} {:<12} {:<12} {:<12}",
        "Turn", "Cache", "Input", "Read", "Write"
    );
    println!("{}", "-".repeat(55));
    for row in rows {
        let ratio = cox_core::cache_diag::ratio_of(&row.usage);
        println!(
            "{:<5} {:<8} {:<12} {:<12} {:<12}",
            row.turn,
            format!("{}%", (ratio * 100.0).round() as u64),
            row.usage.input_tokens,
            row.usage.cache_read_tokens,
            row.usage.cache_write_tokens,
        );
    }
    match store.rollout_read(session) {
        Ok(events) => {
            let misses: Vec<&str> = events
                .iter()
                .filter_map(|e| match e {
                    Event::Notice { text, .. } if text.starts_with("cache miss") => {
                        Some(text.as_str())
                    }
                    _ => None,
                })
                .collect();
            if misses.is_empty() {
                println!("no cache misses recorded");
            } else {
                for m in misses {
                    println!("{m}");
                }
            }
        }
        Err(e) => println!("rollout unavailable: {e}"),
    }
    Ok(())
}
