//! TEA state for the TUI (T5.1): `State`, `Msg`, `Cmd` and the pure
//! `update`. No async, no I/O, no terminal: the runtime (`app`) feeds it key
//! and core events and executes the `Cmd`s it returns, and a test feeds it
//! the same `Event`s a real session emits, so every screen is replayable.

use std::collections::VecDeque;

use cox_protocol::ids::{CallId, ItemId, TaskId};
use cox_protocol::types::{
    Content, Effort, Event, ItemKind, Level, PermissionMode, Presence, Role, SandboxMode,
    SlashCommand, StopReason, Submission, Tier, ToolCall, ToolResult,
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::banner::Banner;
use crate::cells::Look;
use crate::color::Depth;
use crate::commands::{self, Action, COMMANDS, Context};
use crate::composer::{Composer, Edit};
use crate::glyph::{self, Glyphs};
use crate::keymap::{self, Keymap};
use crate::markdown;
use crate::modal::{Approval, Question, QuestionAnswer};
use crate::picker::{self, Kind, Pick, Picker};
use crate::status::parse_todo;
use crate::tasks;
use crate::term::Caps;
use crate::theme::{Theme, ThemeFile};
use crate::vim::Mode;

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
    /// Session spend cap, in USD (T28.1); the binary sets it from
    /// `budget.session_usd`, so `$` names the spend over the cap.
    pub budget_cap_usd: f64,
    /// Fraction of the cap that warns (T28.1); the binary sets it from
    /// `budget.warn_at`, and the cost segment turns `theme.warn` past it.
    pub budget_warn_at: f64,
    /// `/effort` override for the session (T28.1); `SetEffort` keeps it here
    /// next to the mode the composer already shows, and the line badges it.
    pub effort: Option<Effort>,
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
    /// `?` on an empty composer (T24.6): `KEYMAP` grouped by context, drawn
    /// over the transcript like the diff view.
    Help,
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
    /// Recently finished tasks as `/tasks` lines: exit code and the
    /// `/expand` id of a shell task's output (T27.1).
    pub finished_tasks: Vec<String>,
    /// Lines scrolled up from the bottom of the transcript.
    pub scroll: usize,
    pub banner: Option<Banner>,
    /// Workspace-relative paths the `@` picker offers; the runtime walks them.
    pub files: Vec<String>,
    /// Local branch names for `git checkout <Tab>` (T15.4); the runtime
    /// lists them at start, like `files`.
    pub git_branches: Vec<String>,
    /// The `/` palette as `(name, usage, description)`: the built-in
    /// `COMMANDS` first, then T22.2's markdown file commands appended by the
    /// runtime. A name beyond `COMMANDS` submits `Submission::Command`.
    pub commands: Vec<(String, String, String)>,
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
    /// `cox_tui::term::Caps` (T23.0) resolved once at startup; `app.rs`
    /// reads `kitty_keyboard` to push/pop the Kitty keyboard protocol
    /// (T23.1), and later P23 tasks read the rest.
    pub caps: Caps,
    /// The semantic colour tokens (T24.1) every styled span picks from;
    /// `dark`/`light` by `tui.theme`, `mono` when `depth` is `NO_COLOR`.
    pub theme: Theme,
    /// `Ctrl+O`: diffs shown in full rather than as their `+n −m` header.
    pub show_diffs: bool,
    /// `tui.diff` (T24.5); the binary sets it from config.
    pub diff_mode: crate::diff::Mode,
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
    /// `/theme` candidates (T24.2), in picker order: every `theme_catalog`
    /// name, then every `syntax_names` entry under a `syntax: ` row prefix.
    /// The runtime builds it once at startup, like `files`.
    pub theme_rows: Vec<String>,
    /// Every colour theme `/theme` can preview or apply, `(name, parsed
    /// file)`; built-ins first, so a user file cannot shadow one.
    pub theme_catalog: Vec<(String, ThemeFile)>,
    /// `.tmTheme` names (T24.2 step 4), leaked once at startup like
    /// `syntax_theme` itself so a picker preview never leaks on a keystroke.
    pub syntax_names: Vec<&'static str>,
    /// `(dark, theme, syntax_theme)` saved when `/theme` opens the picker;
    /// `Esc` restores it, a chosen row drops it.
    pub theme_prev: Option<(bool, Theme, &'static str)>,
    /// Messages typed with `Enter` while a turn runs (T25.1), oldest first;
    /// `view.rs` shows them above the composer and each natural `TurnDone`
    /// pops one into the next turn.
    pub queue: VecDeque<String>,
    /// Set when `Ctrl+Enter`/`Alt+Enter` interrupts a running turn to send
    /// now (T25.1); the next `TurnDone{Interrupted}` consumes it and joins
    /// the whole queue into one turn instead of leaving it queued.
    pub send_now: bool,
    /// The keys (T25.5): `KEYMAP` with `~/.cox/keybindings.toml` and Claude
    /// Code's `keybindings.json` over it; the binary loads it.
    pub keymap: Keymap,
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
    /// `/fork [turn]` (T26.3): leave for a child session with the history
    /// up to `turn` (`None`: all of it); the binary builds it.
    Fork(Option<u32>),
    /// `/handoff <objective>` (T26.3): leave for a child session seeded
    /// with a cheap-tier summary of this one plus the objective.
    Handoff(String),
    Copy(String),
    Ask(Ask),
    /// `ask_user`'s answer for `call`; `None` is `Esc` (dismissed). The
    /// runtime looks up the matching reply sender itself — it drops it
    /// rather than sending empty text, so the tool call fails instead of
    /// succeeding silently.
    Answer(CallId, Option<String>),
    /// `/theme` (T24.2): a picker choice writes `key` in the user config
    /// with `cox config set` semantics. `cox-tui` has no `toml_edit`-editing
    /// path of its own (only `crates/cox` owns the config file); the
    /// runtime carries this to `config_cmd::set`.
    PersistConfig {
        key: String,
        value: String,
    },
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
                budget_cap_usd: 5.0,
                budget_warn_at: 0.8,
                effort: None,
            },
            modal: None,
            mode,
            tasks: Vec::new(),
            finished_tasks: Vec::new(),
            scroll: 0,
            banner: None,
            files: Vec::new(),
            git_branches: Vec::new(),
            commands: COMMANDS
                .iter()
                .map(|(n, u, d)| (n.to_string(), u.to_string(), d.to_string()))
                .collect(),
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
            caps: Caps::default(),
            theme: Theme::dark(),
            show_diffs: true,
            diff_mode: crate::diff::Mode::Auto,
            expanded_last: false,
            todo: Vec::new(),
            show_todo: false,
            marks: false,
            agents: Vec::new(),
            sessions: Vec::new(),
            git: None,
            worktree: None,
            theme_rows: Vec::new(),
            theme_catalog: Vec::new(),
            syntax_names: Vec::new(),
            theme_prev: None,
            queue: VecDeque::new(),
            send_now: false,
            keymap: Keymap::default(),
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
            diff: self.diff_mode,
            tick: self.tick,
            marks: self.marks,
            colors: self.theme,
            // The generic look shared by a whole render pass does not know
            // which cell is last; `view.rs` overrides it for that one index.
            expand_last: None,
        }
    }

    /// Which `KEYMAP` context the keys are in right now (T24.6).
    pub fn context(&self) -> Context {
        match &self.modal {
            Some(Modal::Diff { .. } | Modal::Help) => Context::Overlay,
            Some(_) => Context::Modal,
            None if self.status.busy => Context::Running,
            None => Context::Idle,
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

    /// The newest `bash` or `agent` call still waiting for its result.
    fn detachable_call(&self) -> Option<CallId> {
        self.transcript.iter().rev().find_map(|c| match c {
            Cell::Tool {
                call, result: None, ..
            } if matches!(call.name.as_str(), "bash" | "agent") => Some(call.id),
            _ => None,
        })
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
        Msg::Event(ev) => on_event(state, ev),
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
    // A held `Enter`/`Esc` must not repeat-submit or repeat-dismiss under the
    // Kitty keyboard protocol (T23.1); `Release` is already filtered in
    // `app.rs`'s select loop, before a `Msg::Key` ever reaches here.
    if key.kind == KeyEventKind::Repeat && matches!(key.code, KeyCode::Enter | KeyCode::Esc) {
        return Vec::new();
    }
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
    // A `git` line completes on `Tab` (T15.4) before `Tab` means anything else.
    if key.code == KeyCode::Tab && state.modal.is_none() {
        let line = state.composer.text();
        let found = picker::candidates(&line, state);
        if !found.is_empty() {
            let picker = Picker::open(Kind::Shell, found).with_query(picker::last_word(&line));
            state.modal = Some(Modal::Picker(picker));
            return Vec::new();
        }
        // T25.2: `Tab` completes the `@`/`/` token under the cursor with the
        // picker typing the sigil would have opened, its query pre-filled.
        if let Some((sigil, query)) = state.composer.take_token() {
            let picker = match sigil {
                '@' => Picker::open(Kind::Files, state.files.clone()),
                _ => Picker::open(
                    Kind::Commands,
                    state.commands.iter().map(|(n, ..)| n.clone()).collect(),
                ),
            };
            state.modal = Some(Modal::Picker(picker.with_query(&query)));
            return Vec::new();
        }
    }
    // T25.5: every other key the TUI owns goes through the keymap; a key it
    // does not claim falls through to the modal or the composer.
    let base = if state.status.busy {
        Context::Running
    } else {
        Context::Idle
    };
    if let Some(action) = state.keymap.resolve(key, base)
        && let Some(cmds) = run(state, action, key)
    {
        return cmds;
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
        // T24.2: every key that changes the selection previews the row
        // immediately, not only `Enter` — the same live-apply the picker's
        // other kinds do not need, since none of them redraws the screen
        // they came from.
        Some(Modal::Picker(mut picker)) if picker.kind == Kind::Themes => {
            match picker.key(key) {
                Pick::Closed => {
                    if let Some((dark, theme, syntax_theme)) = state.theme_prev.take() {
                        state.dark = dark;
                        state.theme = theme;
                        state.syntax_theme = syntax_theme;
                    }
                }
                Pick::Chosen(row) => {
                    state.theme_prev = None;
                    return apply_theme_choice(state, &row);
                }
                Pick::Nothing => {
                    preview_theme(state, &picker);
                    state.modal = Some(Modal::Picker(picker));
                }
            }
            Vec::new()
        }
        Some(Modal::Picker(mut picker)) => {
            match picker.key(key) {
                Pick::Nothing => state.modal = Some(Modal::Picker(picker)),
                // Backspacing out of the picker also removes the `@`/`/`
                // that opened it, as the user meant.
                Pick::Closed if key.code == KeyCode::Backspace && picker.kind != Kind::Shell => {
                    state.composer.key(key, state.status.busy);
                }
                // `Esc` gives back what was typed after the sigil, so a
                // `Tab` that found nothing costs no text (T25.2).
                Pick::Closed if matches!(picker.kind, Kind::Files | Kind::Commands) => {
                    state.composer.insert(&picker.query);
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
                    // `Themes` is intercepted by its own guarded arm above
                    // and never reaches this generic one.
                    Kind::Rewind | Kind::RewindWhat | Kind::Themes => {}
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
                KeyCode::Esc | KeyCode::Char('?') => return Vec::new(),
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
        Some(Modal::Help) => {
            if !matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
                state.modal = Some(Modal::Help);
            }
            Vec::new()
        }
        None => {
            // `Esc Esc` on an idle empty composer opens the rewind timeline
            // (T26.2); a lone Esc still reaches the composer for vim.
            if key.code == KeyCode::Esc && !state.status.busy && state.composer.is_empty() {
                let armed = state.esc_armed.take();
                if armed.is_some_and(|t| state.tick.saturating_sub(t) <= ESC_ESC_TICKS) {
                    return open_rewind(state);
                }
                state.esc_armed = Some(state.tick);
            } else {
                state.esc_armed = None;
            }
            // An `Enter` no binding claims (`send` moved elsewhere) is a
            // newline rather than the composer's own submit.
            if key.code == KeyCode::Enter {
                return compose(state, KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
            }
            compose(state, key)
        }
    }
}

/// A keymap action (T25.5); `None` leaves the key to the modal or the
/// composer. With a modal open only the view toggles and quit act, and a
/// binding on a plain character acts only on an empty composer.
fn run(state: &mut State, action: keymap::Action, key: KeyEvent) -> Option<Vec<Cmd>> {
    use keymap::Action as A;
    let plain = matches!(key.code, KeyCode::Char(_))
        && !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
    let global = matches!(action, A::Quit | A::Thinking | A::Transcript | A::Expand);
    if (plain && !state.composer.is_empty()) || (state.modal.is_some() && !global) {
        return None;
    }
    let enter = |modifiers| KeyEvent::new(KeyCode::Enter, modifiers);
    Some(match action {
        A::Quit => vec![Cmd::Quit],
        A::Thinking => toggle(&mut state.show_thinking),
        A::Transcript => toggle(&mut state.show_diffs),
        A::Expand => toggle(&mut state.expanded_last),
        A::Diff => vec![Cmd::Ask(Ask::GitDiff)],
        // `Ctrl+B` (T27.1): the newest pending `bash`/`agent` card becomes a
        // background task; the turn goes on without waiting for it.
        A::Background => {
            let call_id = state.detachable_call()?;
            vec![Cmd::Submit(Submission::Background { call_id })]
        }
        // The queue's tail back for editing (T25.1); a non-empty composer
        // keeps its usual line-kill.
        A::Unqueue => {
            if !state.composer.is_empty() {
                return None;
            }
            if let Some(text) = state.queue.pop_back() {
                state.composer.set_text(&text);
            }
            Vec::new()
        }
        A::Help => {
            state.modal = Some(Modal::Help);
            Vec::new()
        }
        A::ModeCycle => set_mode(state, commands::next_mode(state.mode)),
        // In vim's normal and visual modes `Esc` never interrupts (T25.4):
        // there it only cancels a pending command; `Ctrl+C` still does.
        A::Interrupt => {
            let vim_owns_esc = state.composer.vim_mode().is_some_and(|m| m != Mode::Insert);
            if key.code == KeyCode::Esc && vim_owns_esc {
                return None;
            }
            vec![Cmd::Submit(Submission::Interrupt)]
        }
        // The composer decides what an `Enter` does; these hand it the one
        // each action means, whatever key was bound.
        A::Send => compose(state, enter(KeyModifiers::NONE)),
        A::Newline => compose(state, enter(KeyModifiers::SHIFT)),
        A::SendNow => compose(state, enter(KeyModifiers::ALT)),
    })
}

fn toggle(flag: &mut bool) -> Vec<Cmd> {
    *flag = !*flag;
    Vec::new()
}

/// A key for the composer, and what its `Edit` means for the session.
fn compose(state: &mut State, key: KeyEvent) -> Vec<Cmd> {
    match state.composer.key(key, state.status.busy) {
        Edit::Submit(text) => {
            let tier = state.status.tier.unwrap_or(Tier::Code);
            // T22.2: a file command's name reaches the core as
            // `Submission::Command`; the T5.5 parser owns the
            // built-ins and would answer these with a notice.
            match file_command(&state.commands, &text).or_else(|| commands::parse(&text, tier)) {
                Some(action) => act(state, action),
                // A turn is running: queue instead of submitting
                // (T25.1). A slash command still runs immediately
                // above — `/clear` in particular must reach the
                // queue it is about to empty.
                None if state.status.busy => {
                    state.queue.push_back(text);
                    Vec::new()
                }
                None => vec![Cmd::Submit(Submission::UserTurn {
                    text,
                    attachments: Vec::new(),
                    confirm_think: false,
                })],
            }
        }
        Edit::SendNow(text) => send_now(state, text),
        Edit::OpenFiles => {
            state.modal = Some(Modal::Picker(Picker::open(
                Kind::Files,
                state.files.clone(),
            )));
            Vec::new()
        }
        Edit::OpenCommands => {
            let names = state.commands.iter().map(|(n, ..)| n.clone()).collect();
            state.modal = Some(Modal::Picker(Picker::open(Kind::Commands, names)));
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

fn set_mode(state: &mut State, mode: PermissionMode) -> Vec<Cmd> {
    state.mode = mode;
    vec![Cmd::Submit(Submission::SetPermissionMode { mode })]
}

/// `Ctrl+Enter`/`Alt+Enter` while a turn runs (T25.1): the composer already
/// cleared itself (`Edit::SendNow`); its text joins the queue's tail so the
/// interrupt this triggers, once `TurnDone{Interrupted}` acknowledges it,
/// flushes everything typed so far as one turn instead of leaving it queued
/// for the next natural finish.
fn send_now(state: &mut State, text: String) -> Vec<Cmd> {
    if !text.trim().is_empty() {
        state.queue.push_back(text);
    }
    state.send_now = true;
    vec![Cmd::Submit(Submission::Interrupt)]
}

/// T24.2: applies one `/theme` row to `State` without persisting it — the
/// live preview `Esc` (via `theme_prev`) can still undo. A `syntax: <name>`
/// row only ever touches `syntax_theme`; a bare name looks it up in
/// `theme_catalog` and follows the current background unless the file
/// itself pins one.
fn apply_row(state: &mut State, row: &str) {
    if let Some(name) = row.strip_prefix("syntax: ") {
        if let Some(&s) = state.syntax_names.iter().find(|n| **n == name) {
            state.syntax_theme = s;
        }
        return;
    }
    if let Some((_, file)) = state.theme_catalog.iter().find(|(n, _)| n == row) {
        let dark = file.variant.unwrap_or(state.dark);
        state.dark = dark;
        state.theme = file.theme(dark);
    }
}

/// Every key that moves the `/theme` picker's selection previews that row.
fn preview_theme(state: &mut State, picker: &Picker) {
    if let Some(row) = picker.matches.get(picker.selected).cloned() {
        apply_row(state, &row);
    }
}

/// `Enter` on a `/theme` row: applies it (in case `Enter` came before any
/// navigation ever previewed it) and persists it with `cox config set`
/// semantics — `Cmd::PersistConfig` carries the write to the runtime, which
/// alone has `config_cmd::set`.
fn apply_theme_choice(state: &mut State, row: &str) -> Vec<Cmd> {
    apply_row(state, row);
    let key = if row.starts_with("syntax: ") {
        "tui.syntax_theme"
    } else {
        "tui.theme"
    };
    let value = row.strip_prefix("syntax: ").unwrap_or(row).to_string();
    vec![Cmd::PersistConfig {
        key: key.into(),
        value,
    }]
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

/// T22.2: a `/name args` line naming a file command — something
/// `State.commands` carries beyond the built-in `COMMANDS`, which the T5.5
/// parser owns — submits `Submission::Command` for the core, the same shape
/// the parser already produces for built-ins without a dedicated arm.
fn file_command(commands: &[(String, String, String)], line: &str) -> Option<Action> {
    let mut words = line.strip_prefix('/')?.split_whitespace();
    let name = words.next()?;
    if COMMANDS.iter().any(|(n, ..)| *n == name) {
        return None;
    }
    commands.iter().any(|(n, ..)| n == name).then(|| {
        Action::Submit(Submission::Command {
            command: SlashCommand {
                name: name.to_string(),
                args: words.map(str::to_string).collect(),
            },
        })
    })
}

/// A slash command's effect; anything the core owns becomes a `Submit`.
fn act(state: &mut State, action: Action) -> Vec<Cmd> {
    match action {
        Action::Submit(sub) => {
            if let Submission::SetEffort { effort } = &sub {
                state.status.effort = *effort;
            }
            if let Submission::Command { command } = &sub
                && command.name == "clear"
            {
                state.queue.clear();
                return vec![Cmd::Clear];
            }
            return vec![Cmd::Submit(sub)];
        }
        Action::Quit => return vec![Cmd::Quit],
        Action::Mode(mode) => return set_mode(state, mode),
        Action::Help => {
            let text = commands::help(&state.keymap);
            notice(state, Level::Info, text);
        }
        Action::Cost => {
            let s = &state.status;
            let text = format!(
                "${:.2} this session · {} tokens in context",
                s.cost_usd, s.context_tokens
            );
            notice(state, Level::Info, text);
        }
        Action::Todo => state.show_todo = !state.show_todo,
        Action::Tasks => notice(
            state,
            Level::Info,
            tasks::list(&state.tasks, &state.finished_tasks),
        ),
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
        // A running turn would be cut mid-write, so both wait for it.
        Action::Fork(_) | Action::Handoff(_) if state.status.busy => {
            notice(state, Level::Warn, "interrupt the turn first".into());
        }
        Action::Fork(Some(turn)) if !state.turns.iter().any(|t| t.seq == turn) => {
            let text = format!("fork: no turn T{turn}; /rewind lists them");
            notice(state, Level::Warn, text);
        }
        Action::Fork(turn) => {
            state.queue.clear();
            return vec![Cmd::Fork(turn)];
        }
        Action::Handoff(objective) => {
            state.queue.clear();
            return vec![Cmd::Handoff(objective)];
        }
        Action::Theme(Some(name)) => {
            if state.theme_rows.contains(&name) {
                return apply_theme_choice(state, &name);
            }
            notice(
                state,
                Level::Warn,
                format!("unknown theme {name:?}; /theme lists them"),
            );
        }
        Action::Theme(None) => {
            state.theme_prev = Some((state.dark, state.theme, state.syntax_theme));
            state.modal = Some(Modal::Picker(Picker::open(
                Kind::Themes,
                state.theme_rows.clone(),
            )));
        }
    }
    Vec::new()
}

fn on_event(state: &mut State, ev: Event) -> Vec<Cmd> {
    if let Some(banner) = Banner::from_event(&ev) {
        state.banner = Some(banner);
        return Vec::new();
    }
    let mut cmds = Vec::new();
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
        Event::TurnDone { stop, .. } => {
            state.status.busy = false;
            cmds = turn_done_cmds(state, stop);
        }
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
        Event::TaskCompleted {
            task,
            exit_code,
            archive,
            ..
        } => {
            if let Some(i) = state.tasks.iter().position(|(t, _)| *t == task) {
                let (_, label) = state.tasks.remove(i);
                let line = tasks::finished_line(task, &label, exit_code, archive);
                state.finished_tasks.push(line);
                let over = state
                    .finished_tasks
                    .len()
                    .saturating_sub(tasks::FINISHED_KEPT);
                state.finished_tasks.drain(..over);
            }
        }
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
    cmds
}

/// `Event::TurnDone` (T25.1): a natural finish drains the queue's head as
/// the next turn; an interrupt from `send_now` instead joins everything
/// queued (composer text included, folded in by `send_now`) into the one
/// turn `Ctrl+Enter`/`Alt+Enter` asked for. A plain `Ctrl+C` interrupt
/// (`send_now` unset) leaves the queue untouched — the user cancelled, they
/// did not ask to send it.
fn turn_done_cmds(state: &mut State, stop: StopReason) -> Vec<Cmd> {
    let text = match stop {
        StopReason::Interrupted if state.send_now => {
            state.send_now = false;
            let joined = state.queue.drain(..).collect::<Vec<_>>().join("\n\n");
            (!joined.is_empty()).then_some(joined)
        }
        StopReason::Interrupted => None,
        _ => state.queue.pop_front(),
    };
    match text {
        Some(text) => vec![Cmd::Submit(Submission::UserTurn {
            text,
            attachments: Vec::new(),
            confirm_think: false,
        })],
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme;
    use cox_core::{History, HistoryTurn};
    use cox_protocol::ids::TurnId;
    use cox_protocol::types::{Content, Message, PermissionMode, Role, SandboxMode};
    use crossterm::event::{KeyCode, KeyEvent};

    /// Types `text` into the composer and submits it with a plain `Enter`,
    /// the way a user queues or sends a message.
    fn type_line(state: &mut State, text: &str) {
        for c in text.chars() {
            update(state, Msg::Key(KeyEvent::from(KeyCode::Char(c))));
        }
        update(state, Msg::Key(KeyEvent::from(KeyCode::Enter)));
    }

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

    /// T24.2 step 3: moving the `/theme` picker's cursor previews a theme
    /// immediately, and `Esc` restores whatever was active before it opened
    /// — a browse that changes nothing must be free to abandon.
    #[test]
    fn theme_picker_preview_reverts_on_esc() {
        let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
        state.theme_rows = vec!["cox-dark".into(), "cox-light".into()];
        state.theme_catalog = theme::BUILT_IN_THEMES[..2]
            .iter()
            .map(|(name, src)| ((*name).to_string(), theme::parse_theme_file(src).unwrap()))
            .collect();
        let before = (state.dark, state.theme, state.syntax_theme);

        // `/theme` submitted from the composer, same as `clear_command_emits_cmd_clear`.
        update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Char('/'))));
        update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Esc)));
        for c in "theme".chars() {
            update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Char(c))));
        }
        update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Enter)));
        assert!(
            matches!(&state.modal, Some(Modal::Picker(p)) if p.kind == Kind::Themes),
            "/theme with no argument opens the picker"
        );

        update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Down)));
        assert_ne!(
            (state.dark, state.theme, state.syntax_theme),
            before,
            "moving the cursor previews the selected theme"
        );

        update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Esc)));
        assert_eq!(
            (state.dark, state.theme, state.syntax_theme),
            before,
            "Esc restores the theme active before the picker opened"
        );
        assert!(state.modal.is_none());
    }

    /// T25.1 step 1/3: `Enter` while a turn runs queues instead of
    /// submitting, and each natural `TurnDone` drains the queue's head in
    /// the order the messages were typed.
    #[test]
    fn queued_messages_drain_in_order() {
        let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
        state.status.busy = true;
        type_line(&mut state, "first");
        type_line(&mut state, "second");
        assert_eq!(
            state.queue,
            VecDeque::from(["first".to_string(), "second".to_string()])
        );
        assert!(state.composer.is_empty());

        let done = |stop| {
            Msg::Event(Event::TurnDone {
                turn: TurnId::new(),
                stop,
            })
        };
        let cmds = update(&mut state, done(StopReason::EndTurn));
        assert_eq!(
            cmds,
            vec![Cmd::Submit(Submission::UserTurn {
                text: "first".into(),
                attachments: Vec::new(),
                confirm_think: false,
            })]
        );
        assert_eq!(state.queue, VecDeque::from(["second".to_string()]));

        let cmds = update(&mut state, done(StopReason::EndTurn));
        assert_eq!(
            cmds,
            vec![Cmd::Submit(Submission::UserTurn {
                text: "second".into(),
                attachments: Vec::new(),
                confirm_think: false,
            })]
        );
        assert!(state.queue.is_empty());
    }

    /// T25.1 step 4: `Ctrl+Enter` while a turn runs interrupts it and, once
    /// the interrupt lands, joins the queue with whatever was still in the
    /// composer into one turn.
    #[test]
    fn send_now_interrupts_and_flushes() {
        let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
        state.status.busy = true;
        type_line(&mut state, "first");
        for c in "second".chars() {
            update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Char(c))));
        }

        let cmds = update(
            &mut state,
            Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL)),
        );
        assert_eq!(cmds, vec![Cmd::Submit(Submission::Interrupt)]);
        assert!(state.composer.is_empty());
        assert_eq!(
            state.queue,
            VecDeque::from(["first".to_string(), "second".to_string()])
        );

        let cmds = update(
            &mut state,
            Msg::Event(Event::TurnDone {
                turn: TurnId::new(),
                stop: StopReason::Interrupted,
            }),
        );
        assert_eq!(
            cmds,
            vec![Cmd::Submit(Submission::UserTurn {
                text: "first\n\nsecond".into(),
                attachments: Vec::new(),
                confirm_think: false,
            })]
        );
        assert!(state.queue.is_empty());
    }

    /// T25.1 step 1: `Ctrl+U` on an empty composer pops the queue's tail
    /// back for editing.
    #[test]
    fn ctrl_u_unqueues_last() {
        let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
        state.status.busy = true;
        type_line(&mut state, "first");
        type_line(&mut state, "second");

        let cmds = update(
            &mut state,
            Msg::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)),
        );
        assert!(cmds.is_empty());
        assert_eq!(state.composer.text(), "second");
        assert_eq!(state.queue, VecDeque::from(["first".to_string()]));
    }

    /// T22.2: a markdown file command joins the `/` palette after the
    /// built-ins, a chosen row inserts `/name `, and `Enter` submits it as
    /// `Submission::Command { name, args }` — args tokenized like any line.
    #[test]
    fn palette_lists_file_commands() {
        let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
        state.commands.push((
            "review".into(),
            "/review [pr]".into(),
            "review a pull request".into(),
        ));
        assert_eq!(
            state.commands.first().map(|(n, ..)| n.as_str()),
            Some("model"),
            "built-in COMMANDS come first"
        );
        assert_eq!(
            state.commands.last().map(|(n, ..)| n.as_str()),
            Some("review"),
            "file commands are appended after them"
        );

        update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Char('/'))));
        for c in "rev".chars() {
            update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Char(c))));
        }
        let rows = match &state.modal {
            Some(Modal::Picker(p)) => p.matches.clone(),
            other => panic!("the palette is open, got {other:?}"),
        };
        assert!(rows.contains(&"review".to_string()), "{rows:?}");

        update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Enter)));
        assert_eq!(state.composer.text(), "/review ");
        for c in "pr-1".chars() {
            update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Char(c))));
        }
        let cmds = update(&mut state, Msg::Key(KeyEvent::from(KeyCode::Enter)));
        assert_eq!(
            cmds,
            vec![Cmd::Submit(Submission::Command {
                command: SlashCommand {
                    name: "review".into(),
                    args: vec!["pr-1".into()],
                },
            })]
        );
    }

    /// T27.1: `Ctrl+B` backgrounds the newest pending `bash` card; with no
    /// such card it is not swallowed as a background request.
    #[test]
    fn ctrl_b_backgrounds_the_pending_bash_card() {
        use cox_protocol::types::{Risk, ToolCall};
        let mut state = State::new(PermissionMode::Default, SandboxMode::WorkspaceWrite);
        let ctrl_b = || Msg::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL));
        // A pending call only exists inside a running turn, and `Ctrl+B` is a
        // running-turn key (T25.5).
        state.status.busy = true;
        assert_eq!(update(&mut state, ctrl_b()), Vec::new());
        let call_id = CallId::new();
        update(
            &mut state,
            Msg::Event(Event::ToolCallRequested {
                call: ToolCall {
                    id: call_id,
                    name: "bash".into(),
                    input: serde_json::json!({"command": "sleep 5"}),
                    risk: Risk::Exec,
                    subject: "sleep 5".into(),
                },
            }),
        );
        assert_eq!(
            update(&mut state, ctrl_b()),
            vec![Cmd::Submit(Submission::Background { call_id })]
        );
    }
}
