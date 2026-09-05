//! Whole-screen snapshots (A16): finished cells above the inline viewport,
//! laid out the way `app::run` does with `insert_before`, so one snapshot
//! is the terminal as a user sees it. Each test also writes the frame as
//! SVG when `COX_SCREENSHOTS=<dir>` is set (`just screenshots`), which is
//! how `docs/screenshots/*.svg` is made.

use std::path::Path;

use cox_protocol::errors::CoreError;
use cox_protocol::ids::{CallId, ItemId, TaskId, TurnId};
use cox_protocol::types::{
    Event, ItemKind, Job, Level, ModelId, PermissionMode, Risk, SandboxMode, StopReason, Tier,
    ToolCall, ToolResult, Usage, Why,
};
use cox_tui::cells::cell_lines;
use cox_tui::state::{GitStatus, Msg, State, update};
use cox_tui::svg::buffer_to_svg;
use cox_tui::view::{buffer_to_string, view};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};

mod common;
use common::type_line;

/// Wide enough for the whole status line; a narrower terminal clips it.
const COLS: u16 = 100;
/// `app::VIEWPORT_ROWS`: the inline viewport drawn below the scrollback.
const VIEWPORT: u16 = 15;

const PATCH: &str = "diff --git a/src/loop.rs b/src/loop.rs\n--- a/src/loop.rs\n+++ b/src/loop.rs\n@@ -12,4 +12,5 @@\n fn step(&mut self) {\n-    let ev = self.next();\n+    let ev = self.next_event();\n+    self.ledger.record(&ev);\n     self.emit(ev);\n }\ndiff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1 +1,3 @@\n # cox\n+\n+The coxswain steers and calls the strokes.\n";

fn ev(state: &mut State, event: Event) {
    update(state, Msg::Event(event));
}

fn keys(state: &mut State, text: &str) {
    for c in text.chars() {
        update(state, Msg::Key(KeyEvent::from(KeyCode::Char(c))));
    }
}

fn fresh() -> State {
    let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
    update(
        &mut state,
        Msg::Git(Some(GitStatus {
            branch: "main".into(),
            added: 12,
            removed: 3,
        })),
    );
    state
}

fn user(state: &mut State, text: &str) {
    ev(
        state,
        Event::ItemStarted {
            item: ItemId::new(),
            kind: ItemKind::UserMessage {
                text: text.into(),
                attachments: Vec::new(),
            },
        },
    );
}

fn start_turn(state: &mut State) -> TurnId {
    let turn = TurnId::new();
    ev(
        state,
        Event::TurnStarted {
            turn,
            job: Job::Main,
            tier: Tier::Code,
            model: ModelId("claude-sonnet-5".into()),
        },
    );
    turn
}

fn usage(state: &mut State, turn: TurnId, input: u32, cached: u32, cost: f64) {
    ev(
        state,
        Event::Usage {
            turn,
            usage: Usage {
                input_tokens: input,
                output_tokens: 640,
                cache_read_tokens: cached,
                cache_write_tokens: 0,
                estimated: false,
                cost_usd: cost,
                latency_ms: 1_800,
            },
        },
    );
}

fn end_turn(state: &mut State, turn: TurnId) {
    ev(
        state,
        Event::TurnDone {
            turn,
            stop: StopReason::EndTurn,
        },
    );
}

fn tool(state: &mut State, name: &str, subject: &str, risk: Risk, output: &str) -> CallId {
    let id = CallId::new();
    let key = if name == "bash" { "command" } else { "path" };
    ev(
        state,
        Event::ToolCallRequested {
            call: ToolCall {
                id,
                name: name.into(),
                input: serde_json::json!({ key: subject }),
                risk,
                subject: subject.into(),
            },
        },
    );
    ev(
        state,
        Event::ToolCallOutput {
            call_id: id,
            delta: output.into(),
        },
    );
    id
}

fn tool_done(state: &mut State, call_id: CallId, visible: &str) {
    ev(
        state,
        Event::ToolCallDone {
            call_id,
            result: ToolResult {
                ok: true,
                visible: visible.into(),
                archive: None,
                bytes: u64::try_from(visible.len()).unwrap_or(u64::MAX),
                duration_ms: 12,
                diff: None,
            },
        },
    );
}

fn reply(state: &mut State, text: &str, done: bool) {
    let item = ItemId::new();
    ev(
        state,
        Event::ItemStarted {
            item,
            kind: ItemKind::AssistantMessage {
                text: String::new(),
            },
        },
    );
    ev(
        state,
        Event::TextDelta {
            item,
            text: text.into(),
        },
    );
    if done {
        ev(state, Event::ItemDone { item });
    }
}

/// The terminal: scrollback rows for every finished cell, then the
/// viewport `view` draws.
fn screen(state: &mut State) -> (Buffer, Option<Position>) {
    let look = state.look(COLS);
    let scrollback: Vec<Line<'static>> = state
        .take_finished()
        .iter()
        .flat_map(|c| cell_lines(c, &look))
        .collect();
    let above = u16::try_from(scrollback.len()).unwrap();
    let mut buf = Buffer::empty(Rect::new(0, 0, COLS, above + VIEWPORT));
    Paragraph::new(scrollback).render(Rect::new(0, 0, COLS, above), &mut buf);
    let cursor = view(state, Rect::new(0, above, COLS, VIEWPORT), &mut buf);
    cox_tui::color::map_buffer(&mut buf, state.depth);
    (buf, cursor)
}

/// The snapshot text; the SVG lands in `$COX_SCREENSHOTS/<name>.svg`.
fn shot(name: &str, state: &mut State) -> String {
    let (buf, cursor) = screen(state);
    if let Ok(dir) = std::env::var("COX_SCREENSHOTS") {
        let dir = Path::new(&dir);
        std::fs::create_dir_all(dir).unwrap();
        let svg = buffer_to_svg(&buf, cursor, state.dark);
        std::fs::write(dir.join(format!("{name}.svg")), svg).unwrap();
    }
    buffer_to_string(&buf)
}

#[test]
fn screen_fresh_session_with_a_prompt_typed() {
    let mut state = fresh();
    keys(&mut state, "explain how the agent loop assembles context");
    insta::assert_snapshot!(shot("fresh_session", &mut state));
}

#[test]
fn screen_streaming_reply_after_a_read_tool() {
    let mut state = fresh();
    user(&mut state, "read src/loop.rs and summarize it");
    let turn = start_turn(&mut state);
    let src = "//! The agent loop as a state machine.\n\npub fn step(&mut self, sub: Submission) -> Vec<Event> {\n    self.turns.push(Turn::from(sub));\n    self.assemble_context()\n}\n";
    let call = tool(&mut state, "read", "src/loop.rs", Risk::ReadOnly, src);
    tool_done(&mut state, call, src);
    usage(&mut state, turn, 18_400, 12_000, 0.07);
    reply(
        &mut state,
        "The loop is a **state machine**: `Submission` in, `Event` out.\n\n- context is assembled per turn from the instruction files and the last two turns verbatim\n- compaction never edits earlier turns in place\n\n```rust\nlet events = session.step(Submission::UserTurn { text, .. });\n```\n\nNothing in this file touches the network",
        false,
    );
    insta::assert_snapshot!(shot("streaming_reply", &mut state));
}

#[test]
fn screen_finished_turn_with_thinking_collapsed_and_a_follow_up() {
    let mut state = fresh();
    user(&mut state, "why does compaction keep the last two turns?");
    let turn = start_turn(&mut state);
    let item = ItemId::new();
    ev(
        &mut state,
        Event::ItemStarted {
            item,
            kind: ItemKind::Thinking {
                text: String::new(),
                signature: None,
            },
        },
    );
    ev(
        &mut state,
        Event::ThinkingDelta {
            item,
            text: "The user wants the rationale, not the mechanism. plan.md D3 says the\nlast two turns are what the model is most likely to refer back to.\n".into(),
        },
    );
    ev(&mut state, Event::ItemDone { item });
    reply(
        &mut state,
        "Because the model refers back to them most: a summary of the turn it is answering would change the answer. Everything older is replaced by one summary item.",
        true,
    );
    usage(&mut state, turn, 21_000, 18_000, 0.05);
    end_turn(&mut state, turn);
    keys(&mut state, "now add a test for it");
    insta::assert_snapshot!(shot("finished_turn", &mut state));
}

#[test]
fn screen_bash_approval_modal() {
    let mut state = fresh();
    user(&mut state, "run the tests");
    start_turn(&mut state);
    ev(
        &mut state,
        Event::ApprovalRequired {
            call: ToolCall {
                id: CallId::new(),
                name: "bash".into(),
                input: serde_json::json!({"command": "cargo test --workspace"}),
                risk: Risk::Exec,
                subject: "cargo test --workspace".into(),
            },
            why: Why::Risk { risk: Risk::Exec },
        },
    );
    insta::assert_snapshot!(shot("approval_modal", &mut state));
}

#[test]
fn screen_slash_command_palette() {
    let mut state = fresh();
    keys(&mut state, "/mo");
    insta::assert_snapshot!(shot("slash_palette", &mut state));
}

#[test]
fn screen_file_picker_on_at_mention() {
    let mut state = fresh();
    state.files = [
        "README.md",
        "src/loop.rs",
        "src/context.rs",
        "src/compact.rs",
        "tests/loop.rs",
    ]
    .map(String::from)
    .to_vec();
    keys(&mut state, "look at @loop");
    insta::assert_snapshot!(shot("file_picker", &mut state));
}

#[test]
fn screen_git_diff_view() {
    let mut state = fresh();
    update(
        &mut state,
        Msg::Key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL)),
    );
    update(&mut state, Msg::Diff(Some(PATCH.into())));
    insta::assert_snapshot!(shot("diff_view", &mut state));
}

#[test]
fn screen_todo_panel_after_the_todo_tool() {
    let mut state = fresh();
    user(&mut state, "plan the ledger work");
    let turn = start_turn(&mut state);
    let call = tool(&mut state, "todo", "3 items", Risk::ReadOnly, "");
    tool_done(
        &mut state,
        call,
        "[x] 1: read the cost ledger\n[~] 2: add the cache-write column\n[ ] 3: snapshot the status line",
    );
    reply(&mut state, "Three steps; starting the second.", true);
    end_turn(&mut state, turn);
    type_line(&mut state, "/todo");
    insta::assert_snapshot!(shot("todo_panel", &mut state));
}

#[test]
fn screen_running_bash_tool_with_background_tasks() {
    let mut state = fresh();
    for label in [
        "explore: where is the ledger written",
        "shell: cargo clippy",
    ] {
        ev(
            &mut state,
            Event::TaskCreated {
                task: TaskId::new(),
                label: label.into(),
                tier: Tier::Cheap,
            },
        );
    }
    user(&mut state, "build it");
    start_turn(&mut state);
    tool(
        &mut state,
        "bash",
        "cargo build --workspace",
        Risk::Exec,
        "   Compiling cox-protocol v0.1.0\n   Compiling cox-core v0.1.0\n   Compiling cox-tui v0.1.0\n",
    );
    for _ in 0..23 {
        update(&mut state, Msg::Tick);
    }
    insta::assert_snapshot!(shot("running_tool", &mut state));
}

#[test]
fn screen_security_banner_warning_and_error() {
    let mut state = fresh();
    user(&mut state, "append the host to /etc/hosts");
    let turn = start_turn(&mut state);
    ev(
        &mut state,
        Event::Notice {
            level: Level::Security,
            text: "sandbox denied a write outside the workspace: /etc/hosts".into(),
        },
    );
    ev(
        &mut state,
        Event::Notice {
            level: Level::Warn,
            text: "hook `lint` exited 1; skipped".into(),
        },
    );
    ev(
        &mut state,
        Event::Error {
            error: CoreError::Budget {
                spent: 4.2,
                cap: 4.0,
            },
            fatal: false,
        },
    );
    end_turn(&mut state, turn);
    insta::assert_snapshot!(shot("banner_and_error", &mut state));
}

#[test]
fn screen_light_theme_reply() {
    let mut state = fresh();
    state.dark = false;
    user(&mut state, "show me the provider trait");
    let turn = start_turn(&mut state);
    reply(
        &mut state,
        "Every provider implements one trait:\n\n```rust\n#[async_trait]\npub trait Provider: Send + Sync {\n    async fn stream(&self, req: Request) -> Result<EventStream, ProviderError>;\n}\n```\n\nThe `Replay` and `Scripted` providers are what the tests use.",
        true,
    );
    usage(&mut state, turn, 9_800, 9_000, 0.02);
    end_turn(&mut state, turn);
    insta::assert_snapshot!(shot("light_theme", &mut state));
}
