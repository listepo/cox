//! TEA state for the TUI (T5.1): `State`, `Msg`, `Cmd` and the pure
//! `update`. No async, no I/O, no terminal: the runtime (`app`) feeds it key
//! and core events and executes the `Cmd`s it returns, and a test feeds it
//! the same `Event`s a real session emits, so every screen is replayable.

use cox_protocol::ids::{CallId, ItemId, TaskId};
use cox_protocol::types::{
    Content, Event, ItemKind, Level, PermissionMode, Presence, Role, SandboxMode, Submission, Tier,
    ToolCall, ToolResult,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::banner::Banner;
use crate::cells::Look;
use crate::color::Depth;
use crate::commands::{self, Action, COMMANDS};
use crate::composer::{Composer, Edit};
use crate::glyph::{self, Glyphs};
use crate::markdown;
use crate::modal::{Approval, Question, QuestionAnswer};
use crate::picker::{self, Kind, Pick, Picker};
use crate::status::parse_todo;
use crate::tasks;
use crate::theme::Theme;

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
    /// What `ctx N%` is a share of; the binary sets it from the provider.
    pub context_window: u32,
    pub cost_usd: f64,
    pub sandbox: SandboxMode,
    pub busy: bool,
    /// Last call's cache share 0..=1 (T8.3), shown as `cache N%`.
    pub cache_ratio: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Modal {
    Approval(Approval),
    /// `ask_user` (T22.1): blocks the turn until a key answers or dismisses it.
    Question(Question),
    Picker(Picker),
    /// `Ctrl+G` (T15.3): the working tree's `git diff HEAD`, drawn over the
    /// transcript; `scroll` is lines from the top.
    Diff {
        text: String,
        scroll: usize,
    },
}

/// Lines a `PageUp`/`PageDown` moves the diff view (the inline viewport is
/// 15 rows, so a page is a little less).
const DIFF_PAGE: usize = 10;

/// What the status line shows of the working tree; a mirror of
/// `cox_tools::git::Status` so this crate keeps no `cox-tools` dependency
/// (T15.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatus {
    pub branch: String,
    pub added: usize,
    pub removed: usize,
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
    /// Local branch names for `git checkout <Tab>` (T15.4); the runtime
    /// lists them at start, like `files`.
    pub git_branches: Vec<String>,
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
    /// `tui.glyphs`/`[tui.icons]` resolved; the binary sets it from config.
    pub glyphs: Glyphs,
    /// `tui.syntax_theme`; empty, or a name syntect does not know, means the
    /// default for `dark`.
    pub syntax_theme: &'static str,
    /// `tui.color` resolved: what the terminal can show, applied to the
    /// finished buffer rather than at each render site.
    pub depth: Depth,
    /// The semantic colour tokens (T24.1) every styled span picks from;
    /// `dark`/`light` by `tui.theme`, `mono` when `depth` is `NO_COLOR`.
    pub theme: Theme,
    /// `Ctrl+O`: diffs shown in full rather than as their `+n −m` header.
    pub show_diffs: bool,
    /// `Ctrl+E` (T24.4): the last tool cell still in the viewport, expanded
    /// past its fold rather than head/tail. `view.rs` is the only reader
    /// that knows which cell is last, so it turns this into `Look.expand_last`.
    pub expanded_last: bool,
    /// The `todo` tool's latest list as `(mark, text)`; `/todo` shows it.
    pub todo: Vec<(String, String)>,
    pub show_todo: bool,
    /// `-v`: show a glyph where `text::sanitize` removed something.
    pub marks: bool,
    /// The other live sessions of this project (T16.3); the runtime feeds them.
    pub agents: Vec<Presence>,
    /// This project's recent sessions as `(id, picker row)`, newest first;
    /// the runtime fills them like `files` (T16.5).
    pub sessions: Vec<(String, String)>,
    /// The branch and line counts the runtime polls; `None` outside a
    /// repository, so the line is unchanged there.
    pub git: Option<GitStatus>,
    /// The `--worktree` name (T27.3); the status line shows it after the
    /// branch, and only then.
    pub worktree: Option<String>,
    /// The `/rewind` timeline (T26.2): one row per user turn, oldest first.
    pub turns: Vec<TurnRow>,
    /// The `seq` of the turn in flight, from `TurnStarted`.
    pub current_seq: u32,
    /// The turn chosen in the rewind picker, awaiting the what-to-restore row.
    pub rewind_to: Option<u32>,
    /// The tick of a first `Esc` on an empty composer; a second within
    /// `ESC_ESC_TICKS` opens the rewind timeline.
    pub esc_armed: Option<u64>,
}

/// Ticks (100 ms each) two `Esc`s may be apart to count as `Esc Esc`.
pub const ESC_ESC_TICKS: u64 = 5;

/// One user turn as the rewind timeline shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRow {
    pub seq: u32,
    /// The user's text.
    pub text: String,
    /// Files checkpointed during the turn.
    pub files: usize,
    /// Where the turn's user cell sits in `transcript`, so a conversation
    /// rewind cuts there.
    pub cell_at: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    Key(KeyEvent),
    Paste(String),
    Event(Event),
    Tick,
    Resize(u16, u16),
    /// The live sessions of this project, polled by the runtime (T16.3).
    Agents(Vec<Presence>),
    /// The working tree's branch and counts (T15.2); `None` outside a repo.
    Git(Option<GitStatus>),
    /// The runtime's answer to `Ask::GitDiff` (T15.3); `None` outside a repo.
    Diff(Option<String>),
    /// A model's `ask_user` call, surfaced by the runtime (T22.1). The
    /// reply's `oneshot` sender stays in `app.rs`, not here: `State` and
    /// `Modal` must stay `Clone`/`PartialEq` for tests and snapshots, and a
    /// `oneshot::Sender` is neither.
    Question {
        call: CallId,
        question: String,
        options: Vec<String>,
    },
}

/// What the TUI asks the runtime to fetch off-screen; the answer comes back
/// on the feed channel. Kept apart from `Submission`: the core never sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    GitDiff,
}

/// The only effects `update` may request; the runtime performs them.
#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    Submit(Submission),
    Quit,
    Clear,
    Copy(String),
    Ask(Ask),
    /// `ask_user`'s answer for `call`; `None` is `Esc` (dismissed). The
    /// runtime looks up the matching reply sender itself — it drops it
    /// rather than sending empty text, so the tool call fails instead of
    /// succeeding silently.
    Answer(CallId, Option<String>),
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
                context_window: 200_000,
                cost_usd: 0.0,
                sandbox,
                busy: false,
                cache_ratio: 0.0,
            },
            modal: None,
            mode,
            tasks: Vec::new(),
            scroll: 0,
            banner: None,
            files: Vec::new(),
            git_branches: Vec::new(),
            commands: COMMANDS.iter().map(|(n, ..)| n.to_string()).collect(),
            ctrl_c_armed: false,
            turns: Vec::new(),
            current_seq: 0,
            rewind_to: None,
            esc_armed: None,
            tick: 0,
            show_thinking: false,
            dark: true,
            glyphs: glyph::UNICODE,
            syntax_theme: "",
            depth: Depth::True,
            theme: Theme::dark(),
            show_diffs: true,
            expanded_last: false,
            todo: Vec::new(),
            show_todo: false,
            marks: false,
            agents: Vec::new(),
            sessions: Vec::new(),
            git: None,
            worktree: None,
        }
    }

    /// Seeds the transcript from reconstructed session history so resume is not blank.
    pub fn transcript_from_history(&mut self, history: &cox_core::History) {
        for (message_index, message) in history.messages.iter().enumerate() {
            match message.role {
                Role::User => {
                    let text = message
                        .content
                        .iter()
                        .filter_map(|block| match block {
                            Content::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("");
                    if !text.is_empty() {
                        if let Some(mark) = history
                            .turn_marks
                            .iter()
                            .find(|mark| mark.message_index == message_index)
                        {
                            self.turns.push(TurnRow {
                                seq: mark.seq,
                                text: text.clone(),
                                files: mark.checkpoints,
                                cell_at: self.transcript.len(),
                            });
                        }
                        self.transcript.push(Cell::User {
                            text,
                            attachments: vec![],
                        });
                    }
                }
                Role::Assistant => {
                    for block in &message.content {
                        match block {
                            Content::Text { text } => {
                                self.transcript.push(Cell::Assistant {
                                    item: ItemId::new(),
                                    text: text.clone(),
                                    done: true,
                                });
                            }
                            Content::Thinking { text, .. } => {
                                self.transcript.push(Cell::Thinking {
                                    item: ItemId::new(),
                                    text: text.clone(),
                                    done: true,
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        self.current_seq = history.turns;
    }

    /// What `cells::cell_lines` needs for a `width`-column render.
    pub fn look(&self, width: u16) -> Look {
        Look {
            width,
            theme: markdown::theme_name(self.dark, self.syntax_theme),
            glyphs: self.glyphs,
            show_thinking: self.show_thinking,
            show_diffs: self.show_diffs,
            tick: self.tick,
            marks: self.marks,
            colors: self.theme,
            // The generic look shared by a whole render pass does not know
            // which cell is last; `view.rs` overrides it for that one index.
            expand_last: None,
        }
    }

    /// Finished cells at the head of the transcript, removed so the runtime
    /// can push them into scrollback in order; a streaming cell holds
    /// everything behind it in the viewport.
    pub fn take_finished(&mut self) -> Vec<Cell> {
        let n = self.transcript.iter().take_while(|c| c.done()).count();
        for turn in &mut self.turns {
            turn.cell_at = turn.cell_at.saturating_sub(n);
        }
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
        Msg::Agents(agents) => {
            state.agents = agents;
            Vec::new()
        }
        Msg::Git(git) => {
            state.git = git;
            Vec::new()
        }
        // git's output is a tool's: sanitised once, here at the boundary.
        Msg::Diff(text) => {
            let text = crate::text::sanitize(&text.unwrap_or_default());
            state.modal = Some(Modal::Diff { text, scroll: 0 });
            Vec::new()
        }
        Msg::Question {
            call,
            question,
            options,
        } => {
            state.modal = Some(Modal::Question(Question::new(call, question, options)));
            Vec::new()
        }
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
    if ctrl && key.code == KeyCode::Char('o') {
        state.show_diffs = !state.show_diffs;
        return Vec::new();
    }
    if ctrl && key.code == KeyCode::Char('e') {
        state.expanded_last = !state.expanded_last;
        return Vec::new();
    }
    if ctrl && key.code == KeyCode::Char('g') && state.modal.is_none() {
        return vec![Cmd::Ask(Ask::GitDiff)];
    }
    if key.code == KeyCode::Tab && state.modal.is_none() {
        // A `git` line completes (T15.4); any other Tab cycles the mode.
        let line = state.composer.text();
        let found = picker::candidates(&line, state);
        if found.is_empty() {
            return set_mode(state, commands::next_mode(state.mode));
        }
        let picker = Picker::open(Kind::Shell, found).with_query(picker::last_word(&line));
        state.modal = Some(Modal::Picker(picker));
        return Vec::new();
    }
    match state.modal.take() {
        Some(Modal::Approval(mut approval)) => match approval.key(key) {
            Some(decision) => vec![Cmd::Submit(Submission::Approve {
                call_id: approval.call.id,
                decision,
            })],
            None => {
                state.modal = Some(Modal::Approval(approval));
                Vec::new()
            }
        },
        Some(Modal::Question(mut question)) => match question.key(key) {
            Some(QuestionAnswer::Text(text)) => vec![Cmd::Answer(question.call, Some(text))],
            Some(QuestionAnswer::Dismissed) => vec![Cmd::Answer(question.call, None)],
            None => {
                state.modal = Some(Modal::Question(question));
                Vec::new()
            }
        },
        Some(Modal::Picker(mut picker)) => {
            match picker.key(key) {
                Pick::Nothing => state.modal = Some(Modal::Picker(picker)),
                // Backspacing out of the picker also removes the `@`/`/`
                // that opened it, as the user meant.
                Pick::Closed if key.code == KeyCode::Backspace && picker.kind != Kind::Shell => {
                    state.composer.key(key);
                }
                Pick::Closed => {}
                Pick::Chosen(choice) if picker.kind == Kind::Rewind => {
                    state.rewind_to = picker::turn_of_entry(&choice);
                    state.modal = Some(Modal::Picker(Picker::open(
                        Kind::RewindWhat,
                        picker::REWIND_WHAT.map(String::from).to_vec(),
                    )));
                }
                Pick::Chosen(choice) if picker.kind == Kind::RewindWhat => {
                    if let Some(to_turn) = state.rewind_to.take() {
                        let what = choice.split(' ').next().unwrap_or("");
                        return vec![Cmd::Submit(Submission::Rewind {
                            to_turn,
                            code: what != "talk",
                            conversation: what != "code",
                        })];
                    }
                }
                Pick::Chosen(choice) => match picker.kind {
                    Kind::Files | Kind::Commands => state.composer.insert(&format!("{choice} ")),
                    // Resuming in place needs `app::run` to return a request;
                    // until then the command is the answer (T16.5).
                    Kind::Sessions => {
                        let id = state
                            .sessions
                            .iter()
                            .find(|(_, row)| *row == choice)
                            .map_or(choice.as_str(), |(id, _)| id.as_str());
                        let text = format!("to resume: cox --resume {id}");
                        notice(state, Level::Info, text);
                    }
                    Kind::History => state.composer.set_text(&choice),
                    Kind::Rewind | Kind::RewindWhat => {}
                    Kind::Shell => {
                        let mut line = state.composer.text();
                        let keep = line.len() - picker::last_word(&line).len();
                        line.truncate(keep);
                        line.push_str(&choice);
                        line.push(' ');
                        state.composer.set_text(&line);
                    }
                },
            }
            Vec::new()
        }
        Some(Modal::Diff { text, scroll }) => {
            let scroll = match key.code {
                KeyCode::Esc => return Vec::new(),
                KeyCode::Char('g') if ctrl => return Vec::new(),
                KeyCode::PageDown => {
                    (scroll + DIFF_PAGE).min(text.lines().count().saturating_sub(1))
                }
                KeyCode::PageUp => scroll.saturating_sub(DIFF_PAGE),
                _ => scroll,
            };
            state.modal = Some(Modal::Diff { text, scroll });
            Vec::new()
        }
        None => {
            // Esc while a turn runs interrupts it; otherwise it reaches the
            // composer (vim's normal mode wants it).
            if key.code == KeyCode::Esc && state.status.busy {
                return vec![Cmd::Submit(Submission::Interrupt)];
            }
            // `Esc Esc` on an empty composer opens the rewind timeline
            // (T26.2); a lone Esc still reaches the composer for vim.
            if key.code == KeyCode::Esc && state.composer.is_empty() {
                let armed = state.esc_armed.take();
                if armed.is_some_and(|t| state.tick.saturating_sub(t) <= ESC_ESC_TICKS) {
                    return open_rewind(state);
                }
                state.esc_armed = Some(state.tick);
            } else {
                state.esc_armed = None;
            }
            match state.composer.key(key) {
                Edit::Submit(text) => {
                    let tier = state.status.tier.unwrap_or(Tier::Code);
                    match commands::parse(&text, tier) {
                        Some(action) => act(state, action),
                        None => vec![Cmd::Submit(Submission::UserTurn {
                            text,
                            attachments: Vec::new(),
                            confirm_think: false,
                        })],
                    }
                }
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

fn set_mode(state: &mut State, mode: PermissionMode) -> Vec<Cmd> {
    state.mode = mode;
    vec![Cmd::Submit(Submission::SetPermissionMode { mode })]
}

fn notice(state: &mut State, level: Level, text: String) {
    state.transcript.push(Cell::Notice { level, text });
}

/// `/rewind` and `Esc Esc`: the timeline, newest first.
fn open_rewind(state: &mut State) -> Vec<Cmd> {
    if state.status.busy {
        notice(
            state,
            Level::Warn,
            "rewind: interrupt the turn first".into(),
        );
        return Vec::new();
    }
    if state.turns.is_empty() {
        notice(state, Level::Info, "nothing to rewind yet".into());
        return Vec::new();
    }
    let rows = state
        .turns
        .iter()
        .rev()
        .map(|t| picker::turn_entry(t.seq, t.files, &t.text))
        .collect();
    state.modal = Some(Modal::Picker(Picker::open(Kind::Rewind, rows)));
    Vec::new()
}

/// `/agents`: one line per live session of this workspace. Its cwd and
/// paths are another process's input, so they go through `text::sanitize`.
fn agents_list(agents: &[Presence]) -> String {
    if agents.is_empty() {
        return "no other cox sessions in this workspace".into();
    }
    agents
        .iter()
        .map(|a| {
            let files = if a.touched.is_empty() {
                "no files edited yet".to_string()
            } else {
                format!("editing {}", a.touched.join(", "))
            };
            crate::text::sanitize(&format!(
                "{} pid {} · {} · turn {} · {} · {}",
                a.session,
                a.pid,
                a.status.name(),
                a.turn,
                a.cwd.display(),
                files
            ))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A slash command's effect; anything the core owns becomes a `Submit`.
fn act(state: &mut State, action: Action) -> Vec<Cmd> {
    match action {
        Action::Submit(sub) => {
            if let Submission::Command { command } = &sub
                && command.name == "clear"
            {
                return vec![Cmd::Clear];
            }
            return vec![Cmd::Submit(sub)];
        }
        Action::Quit => return vec![Cmd::Quit],
        Action::Mode(mode) => return set_mode(state, mode),
        Action::Help => notice(state, Level::Info, commands::help()),
        Action::Cost => {
            let s = &state.status;
            let text = format!(
                "${:.2} this session · {} tokens in context",
                s.cost_usd, s.context_tokens
            );
            notice(state, Level::Info, text);
        }
        Action::Todo => state.show_todo = !state.show_todo,
        Action::Tasks => notice(state, Level::Info, tasks::list(&state.tasks)),
        Action::Vim => {
            let on = state.composer.vim_mode().is_none();
            state.composer.set_vim(on);
        }
        Action::Agents => notice(state, Level::Info, agents_list(&state.agents)),
        Action::Sessions => {
            let text = if state.sessions.is_empty() {
                "no sessions for this project yet".to_string()
            } else {
                state
                    .sessions
                    .iter()
                    .map(|(id, row)| format!("{id} · {row}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            notice(state, Level::Info, text);
        }
        Action::Resume => {
            let rows = state.sessions.iter().map(|(_, row)| row.clone()).collect();
            state.modal = Some(Modal::Picker(Picker::open(Kind::Sessions, rows)));
        }
        Action::Notice(text) => notice(state, Level::Warn, text),
        Action::Rewind => return open_rewind(state),
    }
    Vec::new()
}

fn on_event(state: &mut State, ev: Event) {
    if let Some(banner) = Banner::from_event(&ev) {
        state.banner = Some(banner);
        return;
    }
    match ev {
        Event::ItemStarted { item, kind } => match kind {
            ItemKind::UserMessage { text, attachments } => {
                state.turns.push(TurnRow {
                    seq: state.current_seq,
                    text: text.clone(),
                    files: 0,
                    cell_at: state.transcript.len(),
                });
                state.transcript.push(Cell::User {
                    text,
                    attachments: attachments.into_iter().map(|a| a.name).collect(),
                });
            }
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
            let mut todo = None;
            if let Some(Cell::Tool {
                call, result: r, ..
            }) = state.tool_mut(call_id)
            {
                if call.name == "todo" && result.ok {
                    todo = Some(parse_todo(&result.visible));
                }
                *r = Some(result);
            }
            if let Some(todo) = todo {
                state.todo = todo;
            }
        }
        Event::ApprovalRequired { call, why } => {
            state.modal = Some(Modal::Approval(Approval::new(call, why)));
        }
        Event::ApprovalDecided { .. } => state.modal = None,
        Event::TurnStarted {
            seq, tier, model, ..
        } => {
            state.current_seq = seq;
            state.status.busy = true;
            state.status.tier = Some(tier);
            state.status.model = model.to_string();
        }
        Event::TurnDone { .. } => state.status.busy = false,
        Event::Usage { usage, .. } => {
            state.status.cost_usd += usage.cost_usd;
            state.status.context_tokens =
                usage.input_tokens + usage.cache_read_tokens + usage.cache_write_tokens;
            state.status.cache_ratio = cox_core::cache_diag::ratio_of(&usage);
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
        Event::Checkpoint { files, .. } => {
            if let Some(turn) = state.turns.last_mut() {
                turn.files += files.len();
            }
        }
        Event::Rewound {
            to_turn,
            conversation,
            ..
        } => {
            if conversation && let Some(at) = state.turns.iter().position(|t| t.seq >= to_turn) {
                let cut = state.turns[at].cell_at;
                state.transcript.truncate(cut);
                state.turns.truncate(at);
            }
        }
        Event::SessionStarted { .. } | Event::Compacted { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cox_core::{History, HistoryTurn};
    use cox_protocol::types::{Content, Message, PermissionMode, Role, SandboxMode};
    use crossterm::event::{KeyCode, KeyEvent};

    #[test]
    fn transcript_from_history_seeds_user_and_assistant() {
        let messages = [
            Message {
                role: Role::User,
                content: vec![Content::Text {
                    text: "hello".into(),
                }],
            },
            Message {
                role: Role::Assistant,
                content: vec![Content::Text {
                    text: "hi there".into(),
                }],
            },
        ];
        let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
        state.transcript_from_history(&History {
            messages: messages.to_vec(),
            permission_mode: PermissionMode::Default,
            grants: Vec::new(),
            truncated: false,
            turns: 4,
            turn_marks: vec![HistoryTurn {
                item: ItemId::new(),
                seq: 4,
                message_index: 0,
                checkpoints: 2,
            }],
        });
        assert_eq!(state.transcript.len(), 2);
        assert!(matches!(
            &state.transcript[0],
            Cell::User { text, .. } if text == "hello"
        ));
        assert!(matches!(
            &state.transcript[1],
            Cell::Assistant { text, done: true, .. } if text == "hi there"
        ));
        assert_eq!(state.turns[0].seq, 4);
        assert_eq!(state.turns[0].files, 2);
        assert_eq!(state.current_seq, 4);
    }

    #[test]
    fn take_finished_rebases_turn_cell_indices() {
        let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
        state.transcript.push(Cell::User {
            text: "one".into(),
            attachments: Vec::new(),
        });
        state.transcript.push(Cell::User {
            text: "two".into(),
            attachments: Vec::new(),
        });
        state.turns = vec![
            TurnRow {
                seq: 1,
                text: "one".into(),
                files: 0,
                cell_at: 0,
            },
            TurnRow {
                seq: 2,
                text: "two".into(),
                files: 0,
                cell_at: 1,
            },
        ];

        assert_eq!(state.take_finished().len(), 2);
        assert_eq!(state.turns[0].cell_at, 0);
        assert_eq!(state.turns[1].cell_at, 0);
    }

    #[test]
    fn clear_command_emits_cmd_clear() {
        let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
        // `/` at column 0 opens the palette; Esc leaves `/` in the composer.
        update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Char('/'))));
        update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Esc)));
        for c in "clear".chars() {
            update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Char(c))));
        }
        let cmds = update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Enter)));
        assert_eq!(cmds, vec![Cmd::Clear]);
    }
}
