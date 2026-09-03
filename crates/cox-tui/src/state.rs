//! TEA state for the TUI (T5.1): `State`, `Msg`, `Cmd` and the pure
//! `update`. No async, no I/O, no terminal: the runtime (`app`) feeds it key
//! and core events and executes the `Cmd`s it returns, and a test feeds it
//! the same `Event`s a real session emits, so every screen is replayable.

use cox_protocol::ids::{CallId, ItemId, TaskId};
use cox_protocol::types::{
    Decision, Event, ItemKind, Level, PermissionMode, SandboxMode, Submission, Tier, ToolCall,
    ToolResult, Why,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::banner::Banner;

/// One transcript entry. A finished cell leaves the viewport for the
/// terminal's own scrollback (`State::take_finished`).
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    User {
        text: String,
    },
    Assistant {
        item: ItemId,
        text: String,
        done: bool,
    },
    Thinking {
        item: ItemId,
        text: String,
        done: bool,
    },
    Tool {
        call: Box<ToolCall>,
        output: String,
        result: Option<ToolResult>,
    },
    Notice {
        level: Level,
        text: String,
    },
}

impl Cell {
    pub fn done(&self) -> bool {
        match self {
            Cell::User { .. } | Cell::Notice { .. } => true,
            Cell::Assistant { done, .. } | Cell::Thinking { done, .. } => *done,
            Cell::Tool { result, .. } => result.is_some(),
        }
    }
}

/// What the status line shows; filled from `TurnStarted`/`Usage`.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub model: String,
    pub tier: Option<Tier>,
    pub context_tokens: u32,
    pub cost_usd: f64,
    pub sandbox: SandboxMode,
    pub busy: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Modal {
    Approval { call: ToolCall, why: Why },
}

#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub transcript: Vec<Cell>,
    pub composer: String,
    pub status: Status,
    pub modal: Option<Modal>,
    pub mode: PermissionMode,
    pub tasks: Vec<(TaskId, String)>,
    /// Lines scrolled up from the bottom of the transcript.
    pub scroll: usize,
    pub banner: Option<Banner>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    Key(KeyEvent),
    Paste(String),
    Event(Event),
    Tick,
    Resize(u16, u16),
}

/// The only effects `update` may request; the runtime performs them.
#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    Submit(Submission),
    Quit,
    Copy(String),
}

impl State {
    pub fn new(mode: PermissionMode, sandbox: SandboxMode) -> Self {
        Self {
            transcript: Vec::new(),
            composer: String::new(),
            status: Status {
                model: String::new(),
                tier: None,
                context_tokens: 0,
                cost_usd: 0.0,
                sandbox,
                busy: false,
            },
            modal: None,
            mode,
            tasks: Vec::new(),
            scroll: 0,
            banner: None,
        }
    }

    /// Finished cells at the head of the transcript, removed so the runtime
    /// can push them into scrollback in order; a streaming cell holds
    /// everything behind it in the viewport.
    pub fn take_finished(&mut self) -> Vec<Cell> {
        let n = self.transcript.iter().take_while(|c| c.done()).count();
        self.transcript.drain(..n).collect()
    }

    fn tool_mut(&mut self, id: CallId) -> Option<&mut Cell> {
        self.transcript
            .iter_mut()
            .rev()
            .find(|c| matches!(c, Cell::Tool { call, .. } if call.id == id))
    }

    fn item_mut(&mut self, id: ItemId) -> Option<&mut Cell> {
        self.transcript.iter_mut().rev().find(|c| {
            matches!(c, Cell::Assistant { item, .. } | Cell::Thinking { item, .. } if *item == id)
        })
    }
}

pub fn update(state: &mut State, msg: Msg) -> Vec<Cmd> {
    match msg {
        Msg::Key(key) => on_key(state, key),
        Msg::Paste(text) => {
            state.composer.push_str(&text);
            Vec::new()
        }
        Msg::Event(ev) => {
            on_event(state, ev);
            Vec::new()
        }
        Msg::Tick | Msg::Resize(..) => Vec::new(),
    }
}

fn on_key(state: &mut State, key: KeyEvent) -> Vec<Cmd> {
    if let Some(Modal::Approval { call, .. }) = &state.modal {
        let decision = match key.code {
            KeyCode::Char('y') | KeyCode::Enter => Decision::Allow,
            KeyCode::Char('a') => Decision::AllowForSession,
            KeyCode::Char('n') | KeyCode::Esc => Decision::Deny {
                reason: "denied by user".into(),
            },
            _ => return Vec::new(),
        };
        let call_id = call.id;
        state.modal = None;
        return vec![Cmd::Submit(Submission::Approve { call_id, decision })];
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match (key.code, ctrl) {
        (KeyCode::Char('c'), true) if state.status.busy => {
            vec![Cmd::Submit(Submission::Interrupt)]
        }
        (KeyCode::Char('c' | 'd'), true) => vec![Cmd::Quit],
        (KeyCode::Enter, _) => {
            let text = std::mem::take(&mut state.composer);
            if text.trim().is_empty() {
                return Vec::new();
            }
            vec![Cmd::Submit(Submission::UserTurn {
                text,
                attachments: Vec::new(),
                confirm_think: false,
            })]
        }
        (KeyCode::Backspace, _) => {
            state.composer.pop();
            Vec::new()
        }
        (KeyCode::Char(c), false) => {
            state.composer.push(c);
            Vec::new()
        }
        _ => Vec::new(),
    }
}

fn on_event(state: &mut State, ev: Event) {
    if let Some(banner) = Banner::from_event(&ev) {
        state.banner = Some(banner);
        return;
    }
    match ev {
        Event::ItemStarted { item, kind } => match kind {
            ItemKind::UserMessage { text, .. } => state.transcript.push(Cell::User { text }),
            ItemKind::AssistantMessage { text } => state.transcript.push(Cell::Assistant {
                item,
                text,
                done: false,
            }),
            ItemKind::Thinking { text, .. } => state.transcript.push(Cell::Thinking {
                item,
                text,
                done: false,
            }),
            ItemKind::Summary { text } => state.transcript.push(Cell::Notice {
                level: Level::Info,
                text,
            }),
            ItemKind::Notice { level, text } => state.transcript.push(Cell::Notice { level, text }),
            // Tool items arrive as `ToolCallRequested`/`ToolCallDone` too;
            // those carry the streamed output, so they own the cell.
            ItemKind::ToolCall { .. } | ItemKind::ToolResult { .. } => {}
        },
        Event::TextDelta { item, text } | Event::ThinkingDelta { item, text } => {
            if let Some(Cell::Assistant { text: t, .. } | Cell::Thinking { text: t, .. }) =
                state.item_mut(item)
            {
                t.push_str(&text);
            }
        }
        Event::ItemDone { item } => {
            if let Some(Cell::Assistant { done, .. } | Cell::Thinking { done, .. }) =
                state.item_mut(item)
            {
                *done = true;
            }
        }
        Event::ToolCallRequested { call } => state.transcript.push(Cell::Tool {
            call: Box::new(call),
            output: String::new(),
            result: None,
        }),
        Event::ToolCallOutput { call_id, delta } => {
            if let Some(Cell::Tool { output, .. }) = state.tool_mut(call_id) {
                output.push_str(&delta);
            }
        }
        Event::ToolCallDone { call_id, result } => {
            if let Some(Cell::Tool { result: r, .. }) = state.tool_mut(call_id) {
                *r = Some(result);
            }
        }
        Event::ApprovalRequired { call, why } => state.modal = Some(Modal::Approval { call, why }),
        Event::ApprovalDecided { .. } => state.modal = None,
        Event::TurnStarted { tier, model, .. } => {
            state.status.busy = true;
            state.status.tier = Some(tier);
            state.status.model = model.to_string();
        }
        Event::TurnDone { .. } => state.status.busy = false,
        Event::Usage { usage, .. } => {
            state.status.cost_usd += usage.cost_usd;
            state.status.context_tokens =
                usage.input_tokens + usage.cache_read_tokens + usage.cache_write_tokens;
        }
        Event::ModelSwitched { tier, to, .. } => {
            if state.status.tier == Some(tier) {
                state.status.model = to.to_string();
            }
        }
        Event::TaskCreated { task, label, .. } => state.tasks.push((task, label)),
        Event::TaskCompleted { task, .. } => state.tasks.retain(|(t, _)| *t != task),
        Event::Notice { level, text } => state.transcript.push(Cell::Notice { level, text }),
        Event::Error { error, .. } => state.transcript.push(Cell::Notice {
            level: Level::Warn,
            text: error.to_string(),
        }),
        Event::SessionStarted { .. } | Event::Compacted { .. } => {}
    }
}
