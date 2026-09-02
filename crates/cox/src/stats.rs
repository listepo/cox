//! `cox stats --session <id>` command: print per-turn costs and usage statistics
//! from the ledger (plan.md T1.7). Reads from the Store and formats human-readable output.

use std::path::Path;

use cox_protocol::{SessionId, Store as _};
use cox_store::Store;

pub fn run(home: &Path, session_id: &str) -> anyhow::Result<()> {
    let store = Store::open(home)?;
    let session: SessionId = session_id.parse()?;
    let rows = store.usage_for_session(&session)?;

    if rows.is_empty() {
        println!("No usage records found for session {}", session_id);
        return Ok(());
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
