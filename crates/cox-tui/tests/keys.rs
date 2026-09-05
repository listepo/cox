//! Keys and runtime messages the frame tests leave out (A16): quit and
//! paste, the history picker, decisions and model switches arriving from
//! the runtime, task completion, the clock, and the pinned security banner.

use cox_protocol::ids::{CallId, ItemId, TaskId, TurnId};
use cox_protocol::types::{
    DecidedBy, Decision, Event, Job, Level, ModelId, PermissionMode, Risk, SandboxMode, Tier,
    ToolCall, Why,
};
use cox_tui::picker::Kind;
use cox_tui::state::{Cmd, Modal, Msg, State, update};
use cox_tui::view::{buffer_to_string, render};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

mod common;
use common::type_line;

fn state() -> State {
    State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite)
}

fn ctrl(c: char) -> Msg {
    Msg::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
}

fn key(state: &mut State, code: KeyCode) -> Vec<Cmd> {
    update(state, Msg::Key(KeyEvent::from(code)))
}

fn status(state: &State) -> String {
    cox_tui::status::line(state).to_string()
}

#[test]
fn ctrl_d_quits_without_arming() {
    let mut state = state();
    assert_eq!(update(&mut state, ctrl('d')), vec![Cmd::Quit]);
}

#[test]
fn paste_inserts_text_without_submitting() {
    let mut state = state();
    assert!(update(&mut state, Msg::Paste("git log\n--oneline".into())).is_empty());
    assert_eq!(state.composer.text(), "git log\n--oneline");
    assert_eq!(state.composer.line_count(), 2);
}

#[test]
fn ctrl_r_lists_history_newest_first_and_a_choice_refills_the_composer() {
    let mut state = state();
    type_line(&mut state, "first");
    type_line(&mut state, "second");
    assert!(update(&mut state, ctrl('r')).is_empty());
    let Some(Modal::Picker(picker)) = &state.modal else {
        panic!("Ctrl+R opens the history picker: {:?}", state.modal);
    };
    assert_eq!(picker.kind, Kind::History);
    assert_eq!(picker.matches, ["second", "first"]);
    key(&mut state, KeyCode::Down);
    assert!(key(&mut state, KeyCode::Enter).is_empty());
    assert!(state.modal.is_none());
    assert_eq!(state.composer.text(), "first");
}

#[test]
fn approval_decided_by_a_rule_closes_the_modal() {
    let mut state = state();
    let call_id = CallId::new();
    update(
        &mut state,
        Msg::Event(Event::ApprovalRequired {
            call: ToolCall {
                id: call_id,
                name: "bash".into(),
                input: serde_json::json!({"command": "cargo test"}),
                risk: Risk::Exec,
                subject: "cargo test".into(),
            },
            why: Why::Risk { risk: Risk::Exec },
        }),
    );
    assert!(matches!(state.modal, Some(Modal::Approval(_))));
    update(
        &mut state,
        Msg::Event(Event::ApprovalDecided {
            call_id,
            decision: Decision::Allow,
            by: DecidedBy::Rule,
        }),
    );
    assert!(state.modal.is_none());
}

#[test]
fn model_switched_renames_the_status_only_for_the_running_tier() {
    let mut state = state();
    update(
        &mut state,
        Msg::Event(Event::TurnStarted {
            turn: TurnId::new(),
            job: Job::Main,
            tier: Tier::Code,
            model: ModelId("claude-sonnet-5".into()),
        }),
    );
    let switch = |tier, to: &str| Event::ModelSwitched {
        tier,
        from: ModelId("x".into()),
        to: ModelId(to.into()),
    };
    update(
        &mut state,
        Msg::Event(switch(Tier::Cheap, "claude-haiku-4-5")),
    );
    assert_eq!(state.status.model, "claude-sonnet-5");
    update(&mut state, Msg::Event(switch(Tier::Code, "claude-opus-5")));
    assert_eq!(state.status.model, "claude-opus-5");
}

#[test]
fn task_completed_drops_it_from_the_status_count() {
    let mut state = state();
    let tasks = [TaskId::new(), TaskId::new()];
    for task in tasks {
        update(
            &mut state,
            Msg::Event(Event::TaskCreated {
                task,
                label: "explore".into(),
                tier: Tier::Cheap,
            }),
        );
    }
    assert!(status(&state).contains("2 tasks"));
    update(
        &mut state,
        Msg::Event(Event::TaskCompleted {
            task: tasks[0],
            result_item: ItemId::new(),
            cost_usd: 0.01,
        }),
    );
    assert_eq!(state.tasks.len(), 1);
    assert!(!status(&state).contains("2 tasks"));
    assert!(status(&state).contains("1 task"), "{}", status(&state));
}

#[test]
fn security_notice_pins_a_banner_but_a_warning_is_a_cell() {
    let mut state = state();
    update(
        &mut state,
        Msg::Event(Event::Notice {
            level: Level::Security,
            text: "sandbox denied a write outside the workspace".into(),
        }),
    );
    assert!(state.banner.is_some());
    assert!(state.transcript.is_empty(), "a banner is not a cell");
    update(
        &mut state,
        Msg::Event(Event::Notice {
            level: Level::Warn,
            text: "hook skipped".into(),
        }),
    );
    assert_eq!(state.transcript.len(), 1);
    let frame = buffer_to_string(&render(&state, 60, 6));
    let first = frame.lines().next().unwrap_or_default();
    assert!(
        first.contains("sandbox denied"),
        "banner on the first row: {frame}"
    );
    assert!(frame.contains("hook skipped"));
}

#[test]
fn tick_advances_the_clock_and_resize_asks_nothing() {
    let mut state = state();
    for _ in 0..3 {
        assert!(update(&mut state, Msg::Tick).is_empty());
    }
    assert_eq!(state.tick, 3);
    assert!(update(&mut state, Msg::Resize(80, 24)).is_empty());
}
