//! Git-aware completion (T15.4): `Tab` on a `git` line opens the picker over
//! subcommands, branches or paths by position, and a choice replaces the
//! word being typed. Any other line keeps Tab's old meaning.

use cox_protocol::types::{PermissionMode, SandboxMode};
use cox_tui::picker::Kind;
use cox_tui::state::{Modal, Msg, State, update};
use crossterm::event::{KeyCode, KeyEvent};

/// Types `line` into a fresh composer and presses Tab.
fn tab_after(line: &str) -> State {
    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    state.files = vec!["src/lib.rs".into(), "README.md".into()];
    state.git_branches = vec!["main".into(), "feature/x".into()];
    for c in line.chars() {
        update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Char(c))));
    }
    update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Tab)));
    state
}

fn offered(state: &State) -> Vec<String> {
    match &state.modal {
        Some(Modal::Picker(p)) if p.kind == Kind::Shell => p.matches.clone(),
        other => panic!("no shell picker: {other:?}"),
    }
}

#[test]
fn shell_tab_offers_a_subcommand_a_branch_or_a_path_by_position() {
    assert_eq!(offered(&tab_after("git ch"))[0], "checkout");
    assert_eq!(offered(&tab_after("git checkout ma")), ["main"]);
    assert_eq!(offered(&tab_after("git add sr")), ["src/lib.rs"]);
}

#[test]
fn shell_tab_on_another_command_opens_nothing() {
    assert!(tab_after("ls ").modal.is_none());
}

#[test]
fn shell_choice_replaces_the_word_being_typed() {
    let mut state = tab_after("git checkout ma");
    update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Enter)));
    assert!(state.modal.is_none());
    assert_eq!(state.composer.text(), "git checkout main ");
}
