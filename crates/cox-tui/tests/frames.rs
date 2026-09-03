//! Frame snapshots (T5.1): the screen the TUI draws for a given `State`,
//! rendered through the same `view` the runtime uses, so a snapshot here is
//! what a user sees.

use cox_protocol::ids::{CallId, ItemId, TurnId};
use cox_protocol::types::{
    Event, ItemKind, Job, ModelId, PermissionMode, Risk, SandboxMode, StopReason, Submission, Tier,
    ToolCall, ToolResult,
};
use cox_tui::state::{Cmd, Msg, State, update};
use cox_tui::view::{buffer_to_string, render};
use crossterm::event::{KeyCode, KeyEvent};

#[test]
fn frame_empty_session() {
    let state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    insta::assert_snapshot!(buffer_to_string(&render(&state, 80, 6)));
}

/// One replayed turn: user text, a streamed reply, a tool call with output
/// and its result. The finished cells leave for scrollback in order and the
/// still-streaming reply stays in the viewport.
#[test]
fn frame_after_one_turn_replays_events() {
    let mut state = State::new(PermissionMode::Default, SandboxMode::ReadOnly);
    let (turn, user, reply, call_id) = (TurnId::new(), ItemId::new(), ItemId::new(), CallId::new());
    let call = ToolCall {
        id: call_id,
        name: "read".into(),
        input: serde_json::json!({"path": "src/main.rs"}),
        risk: Risk::ReadOnly,
        subject: "src/main.rs".into(),
    };
    let events = [
        Event::TurnStarted {
            turn,
            job: Job::Main,
            tier: Tier::Code,
            model: ModelId("claude-sonnet-5".into()),
        },
        Event::ItemStarted {
            item: user,
            kind: ItemKind::UserMessage {
                text: "read main".into(),
                attachments: Vec::new(),
            },
        },
        Event::ToolCallRequested { call },
        Event::ToolCallOutput {
            call_id,
            delta: "fn main() {}\n".into(),
        },
        Event::ToolCallDone {
            call_id,
            result: ToolResult {
                ok: true,
                visible: "fn main() {}".into(),
                archive: None,
                bytes: 12,
                duration_ms: 3,
                diff: None,
            },
        },
        Event::ItemStarted {
            item: reply,
            kind: ItemKind::AssistantMessage {
                text: String::new(),
            },
        },
        Event::TextDelta {
            item: reply,
            text: "It prints ".into(),
        },
        Event::TextDelta {
            item: reply,
            text: "nothing.".into(),
        },
    ];
    for ev in events {
        assert!(update(&mut state, Msg::Event(ev)).is_empty());
    }
    assert!(state.status.busy);
    let finished = state.take_finished();
    assert_eq!(
        finished.len(),
        2,
        "user cell and tool cell are done: {finished:?}"
    );
    assert_eq!(state.transcript.len(), 1, "the streaming reply stays");
    update(
        &mut state,
        Msg::Event(Event::TurnDone {
            turn,
            stop: StopReason::EndTurn,
        }),
    );
    assert!(!state.status.busy);
    let scrollback = finished
        .iter()
        .flat_map(cox_tui::view::cell_lines)
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(format!(
        "{scrollback}\n----\n{}",
        buffer_to_string(&render(&state, 60, 5))
    ));
}

/// `update` is a free function over `&mut State` returning commands: no
/// `self`, no async, nothing it can reach but the state and the message.
#[test]
fn update_is_pure() {
    fn pure(_: fn(&mut State, Msg) -> Vec<Cmd>) {}
    pure(update);
}

#[test]
fn update_enter_submits_the_composer_as_a_user_turn() {
    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    for c in "hi".chars() {
        assert!(update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Char(c)))).is_empty());
    }
    let cmds = update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Enter)));
    assert!(matches!(
        cmds.as_slice(),
        [Cmd::Submit(Submission::UserTurn { text, .. })] if text == "hi"
    ));
    assert!(state.composer.is_empty());
    assert!(update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Enter))).is_empty());
}
