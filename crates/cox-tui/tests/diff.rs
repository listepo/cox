//! Diff view (T15.3): `Ctrl+G` asks the runtime for the working tree's diff
//! and the answer opens a modal over the transcript, rendered through the
//! same per-file blocks an edit result uses.

use cox_protocol::types::{PermissionMode, SandboxMode};
use cox_tui::state::{Ask, Cmd, Modal, Msg, State, update};
use cox_tui::view::{buffer_to_string, render};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

const PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,2 +1,2 @@\n-fn old() {}\n+fn new() {}\n fn keep() {}\ndiff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1 +1,3 @@\n # cox\n+\n+A coxswain.\n";

fn ctrl_g(state: &mut State) -> Vec<Cmd> {
    update(
        state,
        Msg::Key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)),
    )
}

fn scroll(state: &State) -> usize {
    match &state.modal {
        Some(Modal::Diff { scroll, .. }) => *scroll,
        other => panic!("not the diff view: {other:?}"),
    }
}

#[test]
fn diff_view_shows_a_two_file_patch_as_two_headed_blocks() {
    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    assert_eq!(ctrl_g(&mut state), vec![Cmd::Ask(Ask::GitDiff)]);
    assert!(state.modal.is_none(), "the view waits for the answer");
    update(&mut state, Msg::Diff(Some(PATCH.into())));
    insta::assert_snapshot!(buffer_to_string(&render(&state, 60, 18)));
    // A second Ctrl+G while the view is open closes it instead of asking.
    assert!(ctrl_g(&mut state).is_empty());
    assert!(state.modal.is_none());
}

#[test]
fn diff_view_scrolls_by_page_and_esc_closes_it() {
    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    update(&mut state, Msg::Diff(Some(PATCH.into())));
    update(&mut state, Msg::Key(KeyEvent::from(KeyCode::PageDown)));
    let down = scroll(&state);
    assert!(down > 0 && down < PATCH.lines().count(), "{down}");
    update(&mut state, Msg::Key(KeyEvent::from(KeyCode::PageUp)));
    assert_eq!(scroll(&state), 0);
    update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Esc)));
    assert!(state.modal.is_none());
}

#[test]
fn diff_view_says_no_changes_for_an_empty_or_absent_diff() {
    for answer in [Some(String::new()), None] {
        let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
        update(&mut state, Msg::Diff(answer));
        let frame = buffer_to_string(&render(&state, 60, 8));
        assert!(frame.contains("no changes"), "{frame}");
    }
}
