//! Live sessions in the TUI (T16.3): `/agents` lists the fed records with
//! their status and files, and the status line counts them only when there
//! are any. T27.2 turns that list into one card per live agent — a sibling
//! session (`Presence`) or a subagent/background task this session started
//! (`TaskCreated`) — so this file also covers the task half of the card.

mod common;

use std::str::FromStr;

use cox_protocol::ids::{SessionId, TaskId};
use cox_protocol::types::{Event, PermissionMode, Presence, PresenceStatus, SandboxMode, Tier};
use cox_tui::state::{Cell, Msg, State, update};
use cox_tui::status;

fn record(id: &str, pid: u32, status: PresenceStatus, touched: &[&str]) -> Presence {
    Presence {
        session: SessionId::from_str(id).expect("ulid"),
        pid,
        cwd: "/w".into(),
        project: "/w".into(),
        status,
        turn: 3,
        touched: touched.iter().map(|s| (*s).to_string()).collect(),
        updated: 0,
        worktree: None,
    }
}

fn two() -> Vec<Presence> {
    vec![
        record(
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",
            41,
            PresenceStatus::Active,
            &["crates/cox-tui/src/state.rs", "plan.md"],
        ),
        record(
            "01BX5ZZKBKACTAV9WEVGEMMVRZ",
            42,
            PresenceStatus::Waiting,
            &[],
        ),
    ]
}

/// One card per sibling session and one per a live subagent task: name,
/// preset, tier, cost, elapsed, state (plan.md T27.2's narrow card — model,
/// tokens and last tool need a new event and are a follow-up).
#[test]
fn agents_cards_snapshot() {
    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    update(&mut state, Msg::Agents(two()));
    update(
        &mut state,
        Msg::Event(Event::TaskCreated {
            task: TaskId::new(),
            label: "explore: find the flaky test".into(),
            tier: Tier::Cheap,
        }),
    );
    for _ in 0..7 {
        update(&mut state, Msg::Tick);
    }
    common::type_line(&mut state, "/agents");
    let Some(Cell::Notice { text, .. }) = state.transcript.last() else {
        panic!("no notice cell");
    };
    // `record`'s session ids are fixed ULIDs, and the task's own id never
    // prints (only its label does), so the snapshot needs no redaction.
    insta::assert_snapshot!(text);
}

#[test]
fn agents_command_says_so_when_alone() {
    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    common::type_line(&mut state, "/agents");
    let Some(Cell::Notice { text, .. }) = state.transcript.last() else {
        panic!("no notice cell");
    };
    assert_eq!(text, "no live agents");
}

#[test]
fn status_line_counts_agents_and_flags_one_waiting() {
    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    let alone = status::line(&state).to_string();
    assert!(!alone.contains("agent"));
    update(&mut state, Msg::Agents(two()));
    assert!(status::line(&state).to_string().contains("2 agents!"));
    update(&mut state, Msg::Agents(vec![two().remove(0)]));
    let one = status::line(&state).to_string();
    assert!(one.contains("1 agent") && !one.contains('!'));
    update(&mut state, Msg::Agents(Vec::new()));
    assert_eq!(status::line(&state).to_string(), alone);
}
