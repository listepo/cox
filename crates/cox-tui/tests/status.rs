//! Status line, todo panel and slash commands (T5.5): the §1.13 row after
//! real usage events, and what each command line turns into.

use cox_protocol::ids::{CallId, TurnId};
use cox_protocol::types::{
    Event, Job, ModelId, PermissionMode, Risk, SandboxMode, StopReason, Submission, Tier, ToolCall,
    ToolResult, Usage,
};
use cox_tui::commands::{self, Action, COMMANDS};
use cox_tui::state::{Cell, Cmd, Msg, State, update};
use cox_tui::view::{buffer_to_string, render};
use crossterm::event::{KeyCode, KeyEvent};

fn turn(state: &mut State, model: &str, cost: f64, input: u32) {
    let turn = TurnId::new();
    for ev in [
        Event::TurnStarted {
            turn,
            job: Job::Main,
            tier: Tier::Code,
            model: ModelId(model.into()),
        },
        Event::Usage {
            turn,
            usage: Usage {
                input_tokens: input,
                output_tokens: 500,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                estimated: false,
                cost_usd: cost,
                latency_ms: 900,
            },
        },
        Event::TurnDone {
            turn,
            stop: StopReason::EndTurn,
        },
    ] {
        update(state, Msg::Event(ev));
    }
}

fn submit(state: &mut State, line: &str) -> Vec<Cmd> {
    let mut chars = line.chars();
    if let Some(c) = chars.next() {
        update(state, Msg::Key(KeyEvent::from(KeyCode::Char(c))));
        // A `/` at column 0 opened the palette; Esc closes it, keeps the `/`
        // and lets the rest be typed as text.
        if c == '/' {
            update(state, Msg::Key(KeyEvent::from(KeyCode::Esc)));
        }
    }
    for c in chars {
        update(state, Msg::Key(KeyEvent::from(KeyCode::Char(c))));
    }
    update(state, Msg::Key(KeyEvent::from(KeyCode::Enter)))
}

#[test]
fn status_line_after_two_turns() {
    let mut state = State::new(PermissionMode::Plan, SandboxMode::WorkspaceWrite);
    turn(&mut state, "claude-sonnet-5", 0.41, 60_000);
    turn(&mut state, "claude-sonnet-5", 0.42, 82_000);
    insta::assert_snapshot!(cox_tui::status::line(&state).to_string());
}

#[test]
fn command_slash_model_opus_emits_switch_model() {
    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    turn(&mut state, "claude-sonnet-5", 0.1, 10);
    assert_eq!(
        submit(&mut state, "/model opus"),
        vec![Cmd::Submit(Submission::SwitchModel {
            tier: Tier::Code,
            model: Some(ModelId("opus".into())),
        })]
    );
    assert_eq!(
        commands::parse("/model think claude-opus-5", Tier::Code),
        Some(Action::Submit(Submission::SwitchModel {
            tier: Tier::Think,
            model: Some(ModelId("claude-opus-5".into())),
        }))
    );
}

#[test]
fn command_lines_map_to_their_submissions() {
    let parse = |l: &str| commands::parse(l, Tier::Code);
    assert_eq!(
        parse("/compact the auth work"),
        Some(Action::Submit(Submission::Compact {
            focus: Some("the auth work".into())
        }))
    );
    assert_eq!(
        parse("/permissions auto"),
        Some(Action::Mode(PermissionMode::Auto))
    );
    assert_eq!(parse("/quit"), Some(Action::Quit));
    assert!(matches!(parse("/think"), Some(Action::Notice(_))));
    assert!(matches!(parse("/nope"), Some(Action::Notice(_))));
    assert_eq!(parse("not a command"), None);
    assert!(matches!(
        parse("/expand 01ARZ3NDEKTSV4RRFFQ69G5FAA"),
        Some(Action::Submit(Submission::Command { command }))
            if command.name == "expand" && command.args == ["01ARZ3NDEKTSV4RRFFQ69G5FAA"]
    ));
}

#[test]
fn command_help_lists_every_command_and_tab_cycles_the_mode() {
    let mut state = State::new(PermissionMode::Default, SandboxMode::ReadOnly);
    assert!(submit(&mut state, "/help").is_empty());
    let Some(Cell::Notice { text, .. }) = state.transcript.last() else {
        panic!("help is a notice cell");
    };
    for (name, ..) in COMMANDS {
        assert!(
            text.contains(&format!("/{name}")),
            "{name} missing from /help"
        );
    }
    assert_eq!(
        update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Tab))),
        vec![Cmd::Submit(Submission::SetPermissionMode {
            mode: PermissionMode::Plan
        })]
    );
    assert_eq!(state.mode, PermissionMode::Plan);
}

#[test]
fn command_todo_shows_the_panel_from_the_tool_output() {
    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    let call_id = CallId::new();
    update(
        &mut state,
        Msg::Event(Event::ToolCallRequested {
            call: ToolCall {
                id: call_id,
                name: "todo".into(),
                input: serde_json::json!({}),
                risk: Risk::ReadOnly,
                subject: "3 items".into(),
            },
        }),
    );
    update(
        &mut state,
        Msg::Event(Event::ToolCallDone {
            call_id,
            result: ToolResult {
                ok: true,
                visible: "[x] 1: read the loop\n[~] 2: write cells\n[ ] 3: snapshots".into(),
                archive: None,
                bytes: 0,
                duration_ms: 1,
                diff: None,
            },
        }),
    );
    assert!(submit(&mut state, "/todo").is_empty());
    insta::assert_snapshot!(buffer_to_string(&render(&state, 60, 8)));
}
