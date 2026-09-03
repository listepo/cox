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
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn keys(state: &mut State, text: &str) {
    for c in text.chars() {
        assert!(update(state, Msg::Key(KeyEvent::from(KeyCode::Char(c)))).is_empty());
    }
}

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
    keys(&mut state, "hi");
    let cmds = update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Enter)));
    assert!(matches!(
        cmds.as_slice(),
        [Cmd::Submit(Submission::UserTurn { text, .. })] if text == "hi"
    ));
    assert!(state.composer.is_empty());
    assert!(update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Enter))).is_empty());
    // `Up` on the first row brings the last submission back.
    assert!(update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Up))).is_empty());
    assert_eq!(state.composer.text(), "hi");
}

#[test]
fn update_ctrl_c_twice_quits_when_idle_and_interrupts_when_busy() {
    let ctrl_c = Msg::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    assert!(update(&mut state, ctrl_c.clone()).is_empty());
    assert!(state.ctrl_c_armed);
    keys(&mut state, "x");
    assert!(!state.ctrl_c_armed, "any other key disarms");
    update(&mut state, ctrl_c.clone());
    assert_eq!(update(&mut state, ctrl_c.clone()), vec![Cmd::Quit]);
    state.status.busy = true;
    assert_eq!(
        update(&mut state, ctrl_c),
        vec![Cmd::Submit(Submission::Interrupt)]
    );
    assert_eq!(
        update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Esc))),
        vec![Cmd::Submit(Submission::Interrupt)]
    );
}

#[test]
fn composer_multiline() {
    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    keys(&mut state, "first line");
    update(
        &mut state,
        Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT)),
    );
    keys(&mut state, "second");
    assert_eq!(state.composer.text(), "first line\nsecond");
    insta::assert_snapshot!(buffer_to_string(&render(&state, 40, 6)));
}

#[test]
fn composer_at_mention_open() {
    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    state.files = ["README.md", "src/lib.rs", "src/main.rs", "tests/frames.rs"]
        .map(String::from)
        .to_vec();
    keys(&mut state, "look at @ma");
    insta::assert_snapshot!(buffer_to_string(&render(&state, 40, 8)));
    assert!(update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Tab))).is_empty());
    assert_eq!(state.composer.text(), "look at @src/main.rs ");
    assert!(state.modal.is_none());
}

#[test]
fn composer_slash_palette() {
    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    keys(&mut state, "/mo");
    insta::assert_snapshot!(buffer_to_string(&render(&state, 40, 8)));
    update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Enter)));
    assert_eq!(state.composer.text(), "/model ");
    // Backspacing out of an empty palette removes the `/` too.
    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    keys(&mut state, "/");
    update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Backspace)));
    assert!(state.modal.is_none());
    assert!(state.composer.is_empty());
}
