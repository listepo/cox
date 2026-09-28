//! The Context & Cost tab's cost history (T37.29.3.2, DT§5.1, mockup 10's
//! "Cost by turn"): the session's ledger `usage` rows grouped by turn, each
//! turn's subagents right under it, and the session total, every figure
//! formatted. Built from the rows the store returns, never from the meter's
//! running sums, because a cost that is not a ledger row does not exist.
//! Separate from `meter_text.rs`, which formats the live meter.

use cox_protocol::types::{Job, Usage};
use cox_store::queries::LedgerRow;
use cox_store::to_tag;
use serde::Serialize;

use crate::meter_text::tokens;
use crate::usage::add_to;

/// What the tab's "Cost by turn" grid shows.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TurnCosts {
    /// The value columns' headers: `In`, `Out`, `Cache r/w`, `$`.
    pub columns: Vec<String>,
    /// A row per turn in order, each turn's subagents under it as detail
    /// rows; empty before the first provider call.
    pub rows: Vec<CostRow>,
    /// `Session`: every row above summed.
    pub total: CostRow,
}

/// One line of the grid: `1 · code` (a subagent's `explore`), then a value
/// per column.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CostRow {
    pub label: String,
    pub values: Vec<String>,
    /// A subagent's row, drawn indented under its turn.
    pub detail: bool,
}

const COLUMNS: [&str; 4] = ["In", "Out", "Cache r/w", "$"];

/// One turn's rows so far.
struct Turn {
    label: String,
    /// When its first call was written; a subagent that starts later
    /// belongs to it until the next turn starts.
    start: String,
    sum: Option<Usage>,
    subagents: Vec<CostRow>,
}

/// `own` is the session's ledger in written order; `children` each child
/// session's, of which only subagents count (a fork or handoff is a session
/// of its own, not this one's cost).
pub fn build(own: &[LedgerRow], children: &[Vec<LedgerRow>]) -> TurnCosts {
    let mut turns: Vec<Turn> = Vec::new();
    let mut total = None;
    for row in own {
        let u = &row.usage;
        // `turn` restarts at 1 with every turn; a side call (compaction,
        // a summary) is 0 and belongs to the turn it ran in.
        if u.turn == 1 || turns.is_empty() {
            let n = turns.iter().filter(|t| !t.label.is_empty()).count() + 1;
            turns.push(Turn {
                label: if u.turn == 0 {
                    String::new()
                } else {
                    format!("{n} · {}", to_tag(&u.tier))
                },
                start: row.created_at.clone(),
                sum: None,
                subagents: Vec::new(),
            });
        }
        if let Some(turn) = turns.last_mut() {
            turn.sum = Some(add_to(turn.sum, &u.usage));
        }
        total = Some(add_to(total, &u.usage));
    }
    let mut orphans = Vec::new();
    for rows in children {
        let Some(first) = rows.first().filter(|r| is_subagent(&r.usage.job)) else {
            continue;
        };
        let mut sum = None;
        for r in rows {
            sum = Some(add_to(sum, &r.usage.usage));
            total = Some(add_to(total, &r.usage.usage));
        }
        // The job alone: with its tier the label wraps in the inspector's
        // width, and a subagent's tier is its preset's.
        let line = row(to_tag(&first.usage.job), sum, true);
        match turns.iter().rposition(|t| t.start <= first.created_at) {
            Some(at) => turns[at].subagents.push(line),
            None => orphans.push(line),
        }
    }
    let mut rows = Vec::new();
    for turn in turns {
        let label = if turn.label.is_empty() {
            "Before turn 1".to_string()
        } else {
            turn.label
        };
        rows.push(row(label, turn.sum, false));
        rows.extend(turn.subagents);
    }
    rows.extend(orphans);
    TurnCosts {
        columns: COLUMNS.map(String::from).to_vec(),
        rows,
        total: row("Session".into(), total, false),
    }
}

/// The jobs a subagent's child session runs as (`subagent::Preset::job`).
fn is_subagent(job: &Job) -> bool {
    matches!(job, Job::Explore | Job::Shell | Job::Agent)
}

/// In is what the calls sent (input and cache), as the meter's `↑ Sent`.
fn row(label: String, sum: Option<Usage>, detail: bool) -> CostRow {
    let u = sum.unwrap_or(Usage {
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        estimated: false,
        cost_usd: 0.0,
        latency_ms: 0,
    });
    CostRow {
        label,
        values: vec![
            tokens(u.context_tokens()),
            tokens(u.output_tokens),
            format!(
                "{}/{}",
                tokens(u.cache_read_tokens),
                tokens(u.cache_write_tokens)
            ),
            format!("{:.2}", u.cost_usd),
        ],
        detail,
    }
}

#[cfg(test)]
mod tests {
    use cox_protocol::ids::SessionId;
    use cox_protocol::traits::UsageRow;
    use cox_protocol::types::{ModelId, ProviderId, Tier};

    use super::*;

    fn at(second: u32, turn: u32, job: Job, tier: Tier, input: u32, cost: f64) -> LedgerRow {
        LedgerRow {
            usage: UsageRow {
                session_id: SessionId::new(),
                turn,
                job,
                tier,
                provider: ProviderId::Anthropic,
                model: ModelId("m".into()),
                effort: None,
                usage: Usage {
                    input_tokens: input,
                    output_tokens: 100,
                    cache_read_tokens: 1000,
                    cache_write_tokens: 0,
                    estimated: false,
                    cost_usd: cost,
                    latency_ms: 0,
                },
            },
            created_at: format!("2026-09-28T10:00:{second:02}.000Z"),
        }
    }

    fn labels(costs: &TurnCosts) -> Vec<(&str, bool)> {
        costs
            .rows
            .iter()
            .map(|r| (r.label.as_str(), r.detail))
            .collect()
    }

    #[test]
    fn a_turn_starts_where_the_call_ordinal_restarts_and_side_calls_join_it() {
        let own = [
            at(1, 1, Job::Main, Tier::Code, 500, 0.10),
            at(2, 2, Job::Main, Tier::Code, 500, 0.10),
            at(3, 0, Job::Compact, Tier::Cheap, 500, 0.01),
            at(4, 1, Job::Main, Tier::Code, 500, 0.10),
        ];
        let costs = build(&own, &[]);
        assert_eq!(labels(&costs), [("1 · code", false), ("2 · code", false)]);
        assert_eq!(costs.rows[0].values, ["4.5k", "300", "3.0k/0", "0.21"]);
        assert_eq!(costs.rows[1].values[3], "0.10");
        assert_eq!(costs.total.label, "Session");
        assert_eq!(costs.total.values, ["6.0k", "400", "4.0k/0", "0.31"]);
    }

    #[test]
    fn a_subagent_sits_under_the_turn_it_started_in_and_a_fork_is_left_out() {
        let own = [
            at(1, 1, Job::Main, Tier::Code, 500, 0.10),
            at(5, 2, Job::Main, Tier::Code, 500, 0.10),
            at(6, 1, Job::Main, Tier::Code, 500, 0.10),
        ];
        let explore = vec![
            at(2, 1, Job::Explore, Tier::Cheap, 100, 0.01),
            at(3, 2, Job::Explore, Tier::Cheap, 100, 0.01),
        ];
        let fork = vec![at(7, 1, Job::Main, Tier::Code, 900, 9.0)];
        let costs = build(&own, &[explore, fork]);
        assert_eq!(
            labels(&costs),
            [("1 · code", false), ("explore", true), ("2 · code", false)]
        );
        assert_eq!(costs.rows[1].values, ["2.2k", "200", "2.0k/0", "0.02"]);
        assert_eq!(costs.total.values[3], "0.32");
    }

    #[test]
    fn a_side_call_before_the_first_turn_gets_its_own_row() {
        let own = [
            at(1, 0, Job::Summarize, Tier::Cheap, 10, 0.0),
            at(2, 1, Job::Main, Tier::Code, 10, 0.0),
        ];
        let costs = build(&own, &[]);
        assert_eq!(
            labels(&costs),
            [("Before turn 1", false), ("1 · code", false)]
        );
    }

    #[test]
    fn an_empty_ledger_has_no_rows_and_a_zero_total() {
        let costs = build(&[], &[]);
        assert!(costs.rows.is_empty());
        assert_eq!(costs.columns, ["In", "Out", "Cache r/w", "$"]);
        assert_eq!(costs.total.values, ["0", "0", "0/0", "0.00"]);
    }
}
