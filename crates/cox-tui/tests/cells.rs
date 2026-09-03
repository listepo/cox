//! Cell snapshots (T5.3): every cell kind rendered from the golden event
//! stream in `fixtures/events/transcript.jsonl`, so a change to how a cell
//! prints shows up as a snapshot diff rather than in a user's terminal.

use cox_protocol::types::{Event, PermissionMode, SandboxMode};
use cox_tui::cells::cell_lines;
use cox_tui::state::{Cell, Msg, State, update};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

const WIDTH: u16 = 60;

fn replay() -> State {
    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    let jsonl = include_str!("../../../fixtures/events/transcript.jsonl");
    // The bash call is still running: a few ticks give it an elapsed time.
    for _ in 0..3 {
        update(&mut state, Msg::Tick);
    }
    for line in jsonl.lines().filter(|l| !l.trim().is_empty()) {
        let ev: Event = serde_json::from_str(line).expect("fixture line is an Event");
        update(&mut state, Msg::Event(ev));
    }
    for _ in 0..14 {
        update(&mut state, Msg::Tick);
    }
    state
}

fn cell(state: &State, pick: impl Fn(&Cell) -> bool) -> &Cell {
    state
        .transcript
        .iter()
        .find(|c| pick(c))
        .expect("fixture has the cell")
}

fn text(state: &State, cell: &Cell) -> String {
    cell_lines(cell, &state.look(WIDTH))
        .iter()
        .map(|l| l.to_string().trim_end().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn cell_user_lists_attachments() {
    let s = replay();
    insta::assert_snapshot!(text(&s, cell(&s, |c| matches!(c, Cell::User { .. }))));
}

#[test]
fn cell_assistant_renders_markdown_wrapped() {
    let s = replay();
    insta::assert_snapshot!(text(&s, cell(&s, |c| matches!(c, Cell::Assistant { .. }))));
}

#[test]
fn cell_thinking_collapses_until_ctrl_t() {
    let mut s = replay();
    let collapsed = text(&s, cell(&s, |c| matches!(c, Cell::Thinking { .. })));
    update(
        &mut s,
        Msg::Key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL)),
    );
    let expanded = text(&s, cell(&s, |c| matches!(c, Cell::Thinking { .. })));
    insta::assert_snapshot!(format!("{collapsed}\n---\n{expanded}"));
}

#[test]
fn cell_tool_folds_output_and_hints_expand() {
    let s = replay();
    insta::assert_snapshot!(text(
        &s,
        cell(
            &s,
            |c| matches!(c, Cell::Tool { call, .. } if call.name == "read")
        )
    ));
}

#[test]
fn cell_tool_running_shows_spinner_and_elapsed() {
    let s = replay();
    insta::assert_snapshot!(text(
        &s,
        cell(
            &s,
            |c| matches!(c, Cell::Tool { call, .. } if call.name == "bash")
        )
    ));
}

#[test]
fn cell_notice_error_and_summary() {
    let s = replay();
    let rest: Vec<String> = s
        .transcript
        .iter()
        .filter(|c| {
            matches!(
                c,
                Cell::Notice { .. } | Cell::Error { .. } | Cell::Summary { .. }
            )
        })
        .map(|c| text(&s, c))
        .collect();
    insta::assert_snapshot!(rest.join("\n"));
}
