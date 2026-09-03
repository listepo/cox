//! Background task list (T9.2): two running tasks in the status count,
//! `/tasks` output, and the snapshot of the list.

use cox_protocol::ids::TaskId;
use cox_protocol::types::{Event, PermissionMode, SandboxMode, Tier};
use cox_tui::commands;
use cox_tui::state::{Cell, Msg, State, update};
use crossterm::event::{KeyCode, KeyEvent};

fn running(state: &mut State) {
    for label in ["explore: find x", "shell: cargo test"] {
        update(
            state,
            Msg::Event(Event::TaskCreated {
                task: TaskId::new(),
                label: label.into(),
                tier: Tier::Cheap,
            }),
        );
    }
}

fn type_line(state: &mut State, line: &str) {
    let mut chars = line.chars();
    if let Some(c) = chars.next() {
        update(state, Msg::Key(KeyEvent::from(KeyCode::Char(c))));
        // A `/` at column 0 opened the palette; Esc closes it and the rest
        // is typed as text.
        if c == '/' {
            update(state, Msg::Key(KeyEvent::from(KeyCode::Esc)));
        }
    }
    for c in chars {
        update(state, Msg::Key(KeyEvent::from(KeyCode::Char(c))));
    }
    update(state, Msg::Key(KeyEvent::from(KeyCode::Enter)));
}

#[test]
fn tasks_list_shows_two_running_tasks() {
    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    running(&mut state);
    assert!(
        cox_tui::status::line(&state)
            .to_string()
            .contains("2 tasks"),
        "status count"
    );
    assert_eq!(
        commands::parse("/tasks", Tier::Code),
        Some(cox_tui::commands::Action::Tasks)
    );
    type_line(&mut state, "/tasks");
    let Some(Cell::Notice { text, .. }) = state.transcript.last() else {
        panic!("/tasks is a notice cell");
    };
    assert!(text.contains("explore: find x"), "{text}");
    assert!(text.contains("shell: cargo test"), "{text}");
    // Task ids are ULIDs: redact them before snapshotting.
    let redacted = text
        .lines()
        .map(|l| match l.split_once(": ") {
            Some((_, label)) if l.starts_with("- ") => format!("- <task>: {label}"),
            _ => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(redacted);
}
