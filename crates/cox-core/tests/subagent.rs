//! Subagents through the loop (T3.9): the explore preset runs on the cheap
//! tier with read-only tools, its cost lands in the ledger under its own
//! session, and an answer over the cap comes back summarised.

mod common;

use common::{run_with, tool_results};
use cox_protocol::types::{Event, Job, Tier};

#[tokio::test]
async fn subagent_explore_uses_cheap_tier_and_read_only_tools() {
    let (events, store, _) = run_with("subagent_explore", cox_protocol::Config::default()).await;
    let created = events.iter().find_map(|e| match e {
        Event::TaskCreated { tier, label, .. } => Some((*tier, label.clone())),
        _ => None,
    });
    assert_eq!(created, Some((Tier::Cheap, "explore: find x".to_string())));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::TaskCompleted { .. }))
    );
    assert_eq!(tool_results(&events), [(true, "result: x".to_string())]);

    let rows = store.usage_rows();
    let main: Vec<_> = rows.iter().filter(|r| r.job == Job::Main).collect();
    let explore: Vec<_> = rows.iter().filter(|r| r.job == Job::Explore).collect();
    assert_eq!(main.len(), 2, "parent: the delegating turn and `done`");
    assert_eq!(
        explore.len(),
        3,
        "child: the refused write, the echo, the answer"
    );
    assert!(explore.iter().all(|r| r.tier == Tier::Cheap));
    let child_id = explore[0].session_id;
    assert!(explore.iter().all(|r| r.session_id == child_id));
    assert!(main.iter().all(|r| r.session_id != child_id));
}

#[tokio::test]
async fn subagent_result_over_cap_is_summarised_on_the_summarize_job() {
    let (events, store, _) = run_with("subagent_summary", cox_protocol::Config::default()).await;
    assert_eq!(tool_results(&events), [(true, "short summary".to_string())]);
    let summary: Vec<_> = store
        .usage_rows()
        .into_iter()
        .filter(|r| r.job == Job::Summarize)
        .collect();
    assert_eq!(summary.len(), 1);
    assert_eq!(summary[0].tier, Tier::Cheap);
}
