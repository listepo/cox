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
use crate::cells::Look;
use crate::composer::{Composer, Edit};
use crate::picker::{BUILTIN_COMMANDS, Kind, Pick, Picker};

/// One transcript entry. A finished cell leaves the viewport for the
/// terminal's own scrollback (`State::take_finished`).
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    User {
        text: String,
        /// Attachment names; the bytes stay with the item.
        attachments: Vec<String>,
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
        /// `State::tick` when the call was requested; elapsed time is ticks.
        started: u64,
    },
    Notice {
        level: Level,
        text: String,
    },
    Error {
        text: String,
        fatal: bool,
    },
    /// A compaction summary standing in for the turns it replaced.
    Summary {
        text: String,
    },
}

impl Cell {
    pub fn done(&self) -> bool {
        match self {
            Cell::User { .. } | Cell::Notice { .. } | Cell::Error { .. } | Cell::Summary { .. } => {
                true
            }
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
    Picker(Picker),
}

#[derive(Debug, Clone)]
pub struct State {
    pub transcript: Vec<Cell>,
    pub composer: Composer,
    pub status: Status,
    pub modal: Option<Modal>,
    pub mode: PermissionMode,
    pub tasks: Vec<(TaskId, String)>,
    /// Lines scrolled up from the bottom of the transcript.
    pub scroll: usize,
    pub banner: Option<Banner>,
    /// Workspace-relative paths the `@` picker offers; the runtime walks them.
    pub files: Vec<String>,
    /// Names the `/` palette offers; T7.3 appends markdown commands.
    pub commands: Vec<String>,
    /// A first idle `Ctrl+C` arms; the second quits.
    pub ctrl_c_armed: bool,
    /// 100 ms ticks since start; spinners and elapsed times read it.
    pub tick: u64,
    /// `Ctrl+T`: thinking cells expanded rather than a one-line count.
    pub show_thinking: bool,
    /// `tui.theme` resolved: dark unless the user chose light.
    pub dark: bool,
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
            composer: Composer::new(),
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
            files: Vec::new(),
            commands: BUILTIN_COMMANDS.iter().map(|c| c.to_string()).collect(),
            ctrl_c_armed: false,
            tick: 0,
            show_thinking: false,
            dark: true,
        }
    }

    /// What `cells::cell_lines` needs for a `width`-column render.
    pub fn look(&self, width: u16) -> Look {
        Look {
            width,
            dark: self.dark,
            show_thinking: self.show_thinking,
            tick: self.tick,
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
            state.composer.insert(&text);
            Vec::new()
        }
        Msg::Event(ev) => {
            on_event(state, ev);
            Vec::new()
        }
        Msg::Tick => {
            state.tick += 1;
            Vec::new()
        }
        Msg::Resize(..) => Vec::new(),
    }
}

fn on_key(state: &mut State, key: KeyEvent) -> Vec<Cmd> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    // `Ctrl+C` interrupts a running turn; when idle it must be pressed twice.
    if ctrl && key.code == KeyCode::Char('c') {
        if state.status.busy {
            return vec![Cmd::Submit(Submission::Interrupt)];
        }
        if state.ctrl_c_armed {
            return vec![Cmd::Quit];
        }
        state.ctrl_c_armed = true;
        return Vec::new();
    }
    state.ctrl_c_armed = false;
    if ctrl && key.code == KeyCode::Char('d') {
        return vec![Cmd::Quit];
    }
    if ctrl && key.code == KeyCode::Char('t') {
        state.show_thinking = !state.show_thinking;
        return Vec::new();
    }
    match state.modal.take() {
        Some(Modal::Approval { call, why }) => {
            let decision = match key.code {
                KeyCode::Char('y') | KeyCode::Enter => Decision::Allow,
                KeyCode::Char('a') => Decision::AllowForSession,
                KeyCode::Char('n') | KeyCode::Esc => Decision::Deny {
                    reason: "denied by user".into(),
                },
                _ => {
                    state.modal = Some(Modal::Approval { call, why });
                    return Vec::new();
                }
            };
            vec![Cmd::Submit(Submission::Approve {
                call_id: call.id,
                decision,
            })]
        }
        Some(Modal::Picker(mut picker)) => {
            match picker.key(key) {
                Pick::Nothing => state.modal = Some(Modal::Picker(picker)),
                // Backspacing out of the picker also removes the `@`/`/`
                // that opened it, as the user meant.
                Pick::Closed if key.code == KeyCode::Backspace => {
                    state.composer.key(key);
                }
                Pick::Closed => {}
                Pick::Chosen(choice) => match picker.kind {
                    Kind::Files | Kind::Commands => state.composer.insert(&format!("{choice} ")),
                    Kind::History => state.composer.set_text(&choice),
                },
            }
            Vec::new()
        }
        None => {
            if key.code == KeyCode::Esc {
                return if state.status.busy {
                    vec![Cmd::Submit(Submission::Interrupt)]
                } else {
                    Vec::new()
                };
            }
            match state.composer.key(key) {
                Edit::Submit(text) => vec![Cmd::Submit(Submission::UserTurn {
                    text,
                    attachments: Vec::new(),
                    confirm_think: false,
                })],
                Edit::OpenFiles => {
                    state.modal = Some(Modal::Picker(Picker::open(
                        Kind::Files,
                        state.files.clone(),
                    )));
                    Vec::new()
                }
                Edit::OpenCommands => {
                    state.modal = Some(Modal::Picker(Picker::open(
                        Kind::Commands,
                        state.commands.clone(),
                    )));
                    Vec::new()
                }
                Edit::OpenHistory => {
                    // Newest first: the entry wanted is usually the last one.
                    let mut history = state.composer.history().to_vec();
                    history.reverse();
                    state.modal = Some(Modal::Picker(Picker::open(Kind::History, history)));
                    Vec::new()
                }
                Edit::Nothing => Vec::new(),
            }
        }
    }
}

fn on_event(state: &mut State, ev: Event) {
    if let Some(banner) = Banner::from_event(&ev) {
        state.banner = Some(banner);
        return;
    }
    match ev {
        Event::ItemStarted { item, kind } => match kind {
            ItemKind::UserMessage { text, attachments } => state.transcript.push(Cell::User {
                text,
                attachments: attachments.into_iter().map(|a| a.name).collect(),
            }),
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
            ItemKind::Summary { text } => state.transcript.push(Cell::Summary { text }),
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
            started: state.tick,
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
        Event::Error { error, fatal } => state.transcript.push(Cell::Error {
            text: error.to_string(),
            fatal,
        }),
        Event::SessionStarted { .. } | Event::Compacted { .. } => {}
    }
}
