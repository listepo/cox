//! Cell snapshots (T5.3): every cell kind rendered from the golden event
//! stream in `fixtures/events/transcript.jsonl`, so a change to how a cell
//! prints shows up as a snapshot diff rather than in a user's terminal.

use cox_protocol::ids::CallId;
use cox_protocol::types::{Event, PermissionMode, Risk, SandboxMode, ToolCall, ToolResult};
use cox_tui::cells::cell_lines;
use cox_tui::state::{Cell, Msg, State, update};
use cox_tui::view::{buffer_to_string, view};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

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
fn ascii_glyphs_leave_no_unicode_in_any_cell() {
    let mut s = replay();
    s.glyphs = cox_tui::glyph::ASCII;
    s.show_thinking = true;
    let rendered = s
        .transcript
        .iter()
        .map(|c| text(&s, c))
        .collect::<Vec<_>>()
        .join("\n")
        // The fixture's own tool output contains a `…`, which the TUI must
        // print as the tool wrote it; only cox's own glyphs are under test.
        .replace('…', "");
    assert!(rendered.is_ascii(), "non-ASCII in ascii mode:\n{rendered}");
}

#[test]
fn a_read_of_a_rust_file_is_highlighted_by_its_extension() {
    let s = replay();
    let c = cell(
        &s,
        |c| matches!(c, Cell::Tool { call, .. } if call.name == "read"),
    );
    let lines = cell_lines(c, &s.look(WIDTH));
    // The header is styled as a whole; a highlighted body line is split into
    // spans of its own, so more than one span means syntect ran.
    let painted = lines.iter().skip(1).any(|l| l.spans.len() > 2);
    assert!(
        painted,
        "read of src/main.rs was not highlighted: {lines:?}"
    );
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

// T24.4 tool cards: a dedicated session per test (rather than the fixture
// above) so each one starts from a single, isolated tool call.

fn requested(state: &mut State, name: &str, subject: &str, risk: Risk) -> CallId {
    let id = CallId::new();
    update(
        state,
        Msg::Event(Event::ToolCallRequested {
            call: ToolCall {
                id,
                name: name.into(),
                input: serde_json::json!({}),
                risk,
                subject: subject.into(),
            },
        }),
    );
    id
}

fn output(state: &mut State, id: CallId, delta: &str) {
    update(
        state,
        Msg::Event(Event::ToolCallOutput {
            call_id: id,
            delta: delta.into(),
        }),
    );
}

fn done(state: &mut State, id: CallId, ok: bool) {
    update(
        state,
        Msg::Event(Event::ToolCallDone {
            call_id: id,
            result: ToolResult {
                ok,
                visible: String::new(),
                archive: None,
                bytes: 0,
                duration_ms: 8,
                diff: None,
            },
        }),
    );
}

/// Twenty numbered lines: past `HEAD + TAIL + 1` (12), so long enough to
/// fold, with a bash exit trailer so the header's `exit N` has something to
/// parse.
fn long_output(exit: u32) -> String {
    let body = (1..=20)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    format!("{body}\n[exit {exit} in 8ms]")
}

fn tool_cell(state: &State) -> &Cell {
    cell(state, |c| matches!(c, Cell::Tool { .. }))
}

#[test]
fn card_pending() {
    let mut s = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    requested(&mut s, "bash", "sleep 5", Risk::Exec);
    for _ in 0..7 {
        update(&mut s, Msg::Tick);
    }
    insta::assert_snapshot!(text(&s, tool_cell(&s)));
}

#[test]
fn card_ok_folded() {
    let mut s = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    let id = requested(&mut s, "bash", "seq 20", Risk::Exec);
    output(&mut s, id, &long_output(0));
    done(&mut s, id, true);
    let rendered = text(&s, tool_cell(&s));
    assert!(
        rendered.contains("more lines"),
        "20 lines should still fold:\n{rendered}"
    );
    insta::assert_snapshot!(rendered);
}

#[test]
fn card_error_unfolded() {
    let mut s = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    let id = requested(&mut s, "bash", "seq 20 && exit 1", Risk::Exec);
    output(&mut s, id, &long_output(1));
    done(&mut s, id, false);
    let rendered = text(&s, tool_cell(&s));
    assert!(
        !rendered.contains("more lines"),
        "a failed call must show its output whole:\n{rendered}"
    );
    assert!(
        rendered.contains("20"),
        "the tail of the output must still be there:\n{rendered}"
    );
    insta::assert_snapshot!(rendered);
}

/// `view()` is the only place that knows which tool cell is last, so this
/// drives the real render loop rather than `cell_lines` directly, proving
/// `Ctrl+E` reaches through `state.rs` and `view.rs` into the fold.
#[test]
fn ctrl_e_expands_last_card() {
    let mut s = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    let id = requested(&mut s, "bash", "seq 20", Risk::Exec);
    output(&mut s, id, &long_output(0));
    done(&mut s, id, true);

    let mut buf = Buffer::empty(Rect::new(0, 0, WIDTH, 60));
    view(&s, Rect::new(0, 0, WIDTH, 60), &mut buf);
    let folded = buffer_to_string(&buf);
    assert!(
        folded.contains("Ctrl+E"),
        "the last card should hint at Ctrl+E while folded:\n{folded}"
    );

    update(
        &mut s,
        Msg::Key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL)),
    );
    let mut buf = Buffer::empty(Rect::new(0, 0, WIDTH, 60));
    view(&s, Rect::new(0, 0, WIDTH, 60), &mut buf);
    let expanded = buffer_to_string(&buf);
    assert!(
        !expanded.contains("more lines"),
        "Ctrl+E should unfold the last card in place:\n{expanded}"
    );
    assert!(
        expanded.contains("20"),
        "the previously hidden tail should now be visible:\n{expanded}"
    );
}
