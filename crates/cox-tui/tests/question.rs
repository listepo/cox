//! `ask_user` modal (T22.1): what the modal shows for a question with
//! options, and which `Cmd` each key sends — through the same
//! `update`/`view` the runtime uses, exactly like `approval.rs` covers the
//! approval modal.

use cox_protocol::ids::CallId;
use cox_protocol::types::{PermissionMode, SandboxMode};
use cox_tui::state::{Cmd, Msg, State, update};
use cox_tui::view::{buffer_to_string, render};
use crossterm::event::{KeyCode, KeyEvent};

fn key(state: &mut State, code: KeyCode) -> Vec<Cmd> {
    update(state, Msg::Key(KeyEvent::from(code)))
}

fn question_asked() -> (State, CallId) {
    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    let call_id = CallId::new();
    update(
        &mut state,
        Msg::Question {
            call: call_id,
            question: "which environment?".into(),
            options: vec!["staging".into(), "production".into()],
        },
    );
    (state, call_id)
}

#[test]
fn modal_question_with_options() {
    let (state, _) = question_asked();
    insta::assert_snapshot!(buffer_to_string(&render(&state, 60, 7)));
}

#[test]
fn question_digit_picks_the_option_and_sends_answer() {
    let (mut state, call_id) = question_asked();
    assert_eq!(
        key(&mut state, KeyCode::Char('2')),
        vec![Cmd::Answer(call_id, Some("production".into()))]
    );
    assert!(state.modal.is_none());
}

#[test]
fn question_enter_sends_the_typed_free_text() {
    let (mut state, call_id) = question_asked();
    for c in "canary".chars() {
        key(&mut state, KeyCode::Char(c));
    }
    assert_eq!(
        key(&mut state, KeyCode::Enter),
        vec![Cmd::Answer(call_id, Some("canary".into()))]
    );
    assert!(state.modal.is_none());
}

#[test]
fn question_esc_dismisses_with_no_answer() {
    let (mut state, call_id) = question_asked();
    assert_eq!(
        key(&mut state, KeyCode::Esc),
        vec![Cmd::Answer(call_id, None)]
    );
    assert!(state.modal.is_none());
}
