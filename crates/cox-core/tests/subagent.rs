//! Subagents through the loop (T3.9): the explore preset runs on the cheap
//! tier with read-only tools, its cost lands in the ledger under its own
//! session, and an answer over the cap comes back summarised.

mod common;

use std::time::Duration;

use common::{drain, open, run_with, scenario, spawn_turn, tool_results};
use cox_protocol::types::{Content, Event, Job, Tier};

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

/// Collects turn events plus late background completions: `TaskCompleted`
/// may arrive after `TurnDone`, so keep receiving until `completed` pairs
/// and their `finished` notices are all in (or time out).
async fn drain_with_background(
    rx: &mut tokio::sync::mpsc::Receiver<Event>,
    completed: usize,
) -> Vec<Event> {
    let mut events = drain(rx).await;
    while completed_count(&events) < completed || !finished_notice(&events) {
        let ev = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("background completion timeout")
            .expect("event stream closed");
        events.push(ev);
    }
    events
}

fn completed_count(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, Event::TaskCompleted { .. }))
        .count()
}

fn finished_notice(events: &[Event]) -> bool {
    events.iter().any(|e| {
        matches!(
            e,
            Event::Notice { text, .. } if text.contains("finished")
        )
    })
}

#[tokio::test]
async fn tasks_background_agent_reports_pointer_then_notice() {
    let (session, store, mut rx) = open(
        &scenario("subagent_background"),
        cox_protocol::Config::default(),
    );
    let running = spawn_turn(&session, "go");
    running.await.expect("join").expect("turn");
    let events = drain_with_background(&mut rx, 1).await;

    let created = events
        .iter()
        .position(|e| matches!(e, Event::TaskCreated { .. }));
    let completed = events
        .iter()
        .position(|e| matches!(e, Event::TaskCompleted { .. }));
    assert!(created < completed, "Created before Completed");
    // The tool result is a short pointer, never the kilobyte answer.
    let results = tool_results(&events);
    assert_eq!(results.len(), 1);
    assert!(results[0].0);
    assert!(results[0].1.contains("background task"), "{}", results[0].1);
    assert!(!results[0].1.contains("CHILD-ANSWER"), "no full result");
    assert!(finished_notice(&events));
    // History holds the pointer line, and no long tool result at all.
    let history = session.history().await;
    assert!(
        history.iter().flat_map(|m| &m.content).all(|c| match c {
            Content::ToolResult { content, .. } => content.len() < 500,
            _ => true,
        }),
        "no full result in context"
    );
    assert!(
        history
            .iter()
            .flat_map(|m| &m.content)
            .any(|c| matches!(c, Content::Text { text } if text.contains("finished"))),
        "completion pointer in history"
    );
    assert!(store.usage_rows().iter().any(|r| r.job == Job::Explore));
}

#[tokio::test]
async fn tasks_two_background_agents_run_concurrently() {
    let (session, _, mut rx) = open(
        &scenario("subagent_background_two"),
        cox_protocol::Config::default(),
    );
    let running = spawn_turn(&session, "go");
    running.await.expect("join").expect("turn");
    let events = drain_with_background(&mut rx, 2).await;

    let labels: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            Event::TaskCreated { label, .. } => Some(label.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(labels.len(), 2);
    assert!(labels.iter().any(|l| l.contains("first job")), "{labels:?}");
    assert!(
        labels.iter().any(|l| l.contains("second job")),
        "{labels:?}"
    );
    assert_eq!(completed_count(&events), 2);
    let results = tool_results(&events);
    assert_eq!(results.len(), 2);
    assert!(
        results
            .iter()
            .all(|(ok, text)| *ok && text.contains("background task")),
        "{results:?}"
    );
}
