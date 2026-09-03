//! `Session`: Submission in, Event out. The only type `cox-tui` / `cox run`
//! / ACP should talk to; they never call a provider or tool themselves.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};

use cox_protocol::errors::{CoreError, ProviderError, StoreError};
use cox_protocol::ids::{CallId, ItemId, SessionId, TaskId, TurnId};
use cox_protocol::traits::{Archive, ArchivePut, Hook, Provider, Store, Tool};
use cox_protocol::types::{
    ArchiveRef, Content, Decision, Event, HookEvent, HookOutcome, ItemKind, Job, Level, Message,
    ModelId, PermissionMode, Role, SandboxMode, StopReason, Submission, Tier, ToolCall,
};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::budget;
use crate::cache_diag::CacheTracker;
use crate::compact::{self, TurnMark};
use crate::context::assemble_with;
use crate::dedup::Dedup;
use crate::hooks;
use crate::permission::{Engine, Outcome};
use crate::router::{Overrides, Route, RouteError, Router};
use crate::turn::{consume_provider, results_message, run_tools};

/// Loop states from plan.md §1.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// No turn in flight.
    Idle,
    /// Building the next provider request.
    Assembling,
    /// A provider stream is open.
    Streaming,
    /// Tools from the last assistant message are running.
    RunningTools,
    /// Waiting on `Submission::Approve`.
    AwaitingApproval,
    /// Compaction is running (wired in T8.1).
    #[allow(dead_code)]
    Compacting,
    /// Emitting `TurnDone` and flushing.
    Finishing,
    /// `Submission::Interrupt` is draining work.
    Interrupted,
}

enum Step {
    Continue,
    Done,
}

pub(crate) struct Inner {
    pub(crate) state: State,
    pub(crate) history: Vec<Message>,
    provider_calls: u32,
    spent_usd: f64,
    budget_warned: bool,
    permission_mode: PermissionMode,
    /// `AllowForSession` grants as `(tool, subject prefix)`.
    grants: Vec<(String, String)>,
    /// Calls parked in `AwaitingApproval`, answered by `Submission::Approve`.
    pending: HashMap<CallId, oneshot::Sender<Decision>>,
    /// Provider rounds so far in this session; the dedup window counts these.
    round: u32,
    dedup: Dedup,
    /// Deferred tools found through `tool_search`, in discovery order.
    discovered: Vec<String>,
    /// Where each turn starts in `history` (T8.1 compaction cuts on these).
    pub(crate) turn_marks: Vec<TurnMark>,
    /// `call_id` → archived payload for microcompaction (T8.2): the request
    /// replaces old results with `Pointer`s, the stored history keeps them.
    pub(crate) archives: HashMap<CallId, ArchiveRef>,
    /// Last request's prefix hashes + whether it hit the cache (T8.3).
    pub(crate) cache: CacheTracker,
    /// Last call's cache share, for the status line (T8.3 step 1).
    pub(crate) cache_ratio: f64,
    /// Session routing overrides from `/model` (T9.1).
    pub(crate) overrides: Overrides,
    /// Running background tasks: label and tier by id (T9.2).
    pub(crate) tasks: HashMap<TaskId, (String, Tier)>,
    /// Facts `extract_memory` saved, awaiting surface drain (T10.2).
    pub(crate) extracted: Vec<crate::memory_extract::Fact>,
    /// Monotonic turn counter for the FTS index (T10.3).
    turn_seq: u32,
    /// Context size of the last main call, for the §1.10 auto trigger.
    pub(crate) last_context_tokens: u32,
    /// Whether this turn already compacted after a context-length error.
    retried_after_too_long: bool,
}

/// One conversation: a provider, tools, a store, and an event stream.
#[derive(Clone)]
pub struct Session {
    pub(crate) id: SessionId,
    pub(crate) config: cox_protocol::Config,
    pub(crate) provider: Arc<dyn Provider>,
    pub(crate) tools: Vec<Arc<dyn Tool>>,
    pub(crate) store: Arc<dyn Store>,
    pub(crate) archive: Arc<dyn Archive>,
    pub(crate) engine: Arc<Engine>,
    pub(crate) cwd: PathBuf,
    /// What this session's turns are for; `Main` unless it is a subagent.
    pub(crate) job: Job,
    /// The tier every provider call in this session is routed to.
    pub(crate) tier: Tier,
    pub(crate) cancel: Arc<StdMutex<CancellationToken>>,
    /// The hook runner, installed once by the surface and shared with
    /// children so a subagent's calls run the same hooks.
    hook: Arc<OnceLock<Arc<dyn Hook>>>,
    tx: mpsc::Sender<Event>,
    rx: Arc<StdMutex<Option<mpsc::Receiver<Event>>>>,
    pub(crate) inner: Arc<Mutex<Inner>>,
}

impl Session {
    /// Constructs a session and emits `SessionStarted`. The `agent` tool is
    /// added here rather than by the caller because it needs a handle to
    /// this very session; children (`spawn_child`) do not get one, so a
    /// subagent cannot spawn subagents.
    pub fn new(
        config: cox_protocol::Config,
        provider: Arc<dyn Provider>,
        tools: Vec<Arc<dyn Tool>>,
        store: Arc<dyn Store>,
        archive: Arc<dyn Archive>,
        cwd: PathBuf,
    ) -> Result<Self, CoreError> {
        let mut session = Self::build(
            config,
            provider,
            tools,
            store,
            archive,
            cwd,
            None,
            Job::Main,
            Tier::Code,
        )?;
        let parent = session.clone();
        session
            .tools
            .push(Arc::new(crate::subagent::AgentTool::new(parent)));
        Ok(session)
    }

    /// A child session sharing this one's provider, store and archive
    /// (plan.md T3.9): its own rollout and budget, `parent_id` set.
    pub(crate) fn spawn_child(
        &self,
        config: cox_protocol::Config,
        tools: Vec<Arc<dyn Tool>>,
        job: Job,
        tier: Tier,
    ) -> Result<Self, CoreError> {
        let mut child = Self::build(
            config,
            self.provider.clone(),
            tools,
            self.store.clone(),
            self.archive.clone(),
            self.cwd.clone(),
            Some(self.id),
            job,
            tier,
        )?;
        child.hook = self.hook.clone();
        Ok(child)
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        config: cox_protocol::Config,
        provider: Arc<dyn Provider>,
        tools: Vec<Arc<dyn Tool>>,
        store: Arc<dyn Store>,
        archive: Arc<dyn Archive>,
        cwd: PathBuf,
        parent_id: Option<SessionId>,
        job: Job,
        tier: Tier,
    ) -> Result<Self, CoreError> {
        let id = SessionId::new();
        let (tx, rx) = mpsc::channel(256);
        let home = std::env::home_dir();
        let engine = Engine::compile(&config.permissions, home.as_deref(), &cwd)?;
        let permission_mode = config.permissions.mode;
        let dedup = Dedup::new(config.context.dedup_window_turns);
        let session = Self {
            id,
            config,
            provider,
            tools,
            store,
            archive,
            engine: Arc::new(engine),
            cwd: cwd.clone(),
            job,
            tier,
            cancel: Arc::new(StdMutex::new(CancellationToken::new())),
            hook: Arc::new(OnceLock::new()),
            tx,
            rx: Arc::new(StdMutex::new(Some(rx))),
            inner: Arc::new(Mutex::new(Inner {
                state: State::Idle,
                history: Vec::new(),
                provider_calls: 0,
                spent_usd: 0.0,
                budget_warned: false,
                permission_mode,
                grants: Vec::new(),
                pending: HashMap::new(),
                round: 0,
                dedup,
                discovered: Vec::new(),
                turn_marks: Vec::new(),
                archives: HashMap::new(),
                cache: CacheTracker::new(),
                cache_ratio: 0.0,
                overrides: Overrides::default(),
                tasks: HashMap::new(),
                extracted: Vec::new(),
                turn_seq: 0,
                last_context_tokens: 0,
                retried_after_too_long: false,
            })),
        };
        let started = Event::SessionStarted {
            session: id,
            config_digest: String::new(),
            cwd,
        };
        session
            .store
            .session_create(&cox_protocol::SessionRow {
                id,
                created_at: String::new(),
                cwd: session.cwd.clone(),
                project_slug: String::new(),
                title: None,
                parent_id,
                rollout_path: PathBuf::new(),
            })
            .map_err(|error| CoreError::Store { error })?;
        session.store.rollout_append(&id, &started).ok();
        let _ = session.tx.try_send(started);
        // T4.3: the sandbox being off is loud on every surface — one line
        // after `SessionStarted` in stream-json, a pinned banner in the TUI.
        if session.config.sandbox.mode == SandboxMode::DangerFullAccess {
            let _ = session.tx.try_send(Event::Notice {
                level: Level::Security,
                text: crate::permission::policy::DANGER_FULL_ACCESS.into(),
            });
        }
        Ok(session)
    }

    /// Takes the event receiver once.
    pub fn events(&self) -> Option<mpsc::Receiver<Event>> {
        self.rx.lock().unwrap_or_else(|e| e.into_inner()).take()
    }

    /// The transcript the next `assemble` will send (loop tests).
    pub async fn history(&self) -> Vec<Message> {
        self.inner.lock().await.history.clone()
    }

    pub(crate) fn clone_handle(&self) -> Self {
        self.clone()
    }

    pub(crate) async fn emit(&self, ev: Event) -> Result<(), CoreError> {
        self.store
            .rollout_append(&self.id, &ev)
            .map_err(|error| CoreError::Store { error })?;
        let _ = self.tx.send(ev).await;
        Ok(())
    }

    /// Cancels the provider stream and running tools.
    pub fn interrupt(&self) {
        self.cancel
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cancel();
    }

    pub(crate) fn cancel_token(&self) -> CancellationToken {
        self.cancel
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Installs the hook runner (T7.4); a second call is ignored so the
    /// runner stays byte-stable for the session and its children.
    pub fn set_hook(&self, hook: Arc<dyn Hook>) {
        let _ = self.hook.set(hook);
    }

    pub(crate) fn hook(&self) -> Option<Arc<dyn Hook>> {
        self.hook.get().cloned()
    }

    /// Feeds one submission into the state machine.
    pub async fn submit(&self, sub: Submission) -> Result<(), CoreError> {
        match sub {
            Submission::UserTurn {
                text,
                confirm_think,
                attachments: _,
            } => self.run_turn(text, confirm_think).await,
            Submission::Interrupt => {
                self.interrupt();
                Ok(())
            }
            Submission::Approve { call_id, decision } => {
                let waiter = self.inner.lock().await.pending.remove(&call_id);
                match waiter {
                    Some(tx) => {
                        let _ = tx.send(decision);
                        Ok(())
                    }
                    None => {
                        self.emit(Event::Notice {
                            level: Level::Warn,
                            text: format!("no approval pending for call {call_id}"),
                        })
                        .await
                    }
                }
            }
            Submission::SetPermissionMode { mode } => {
                self.inner.lock().await.permission_mode = mode;
                self.emit(Event::Notice {
                    level: Level::Info,
                    text: format!("permission mode: {mode:?}"),
                })
                .await
            }
            Submission::Compact { focus } => self
                .compact(compact::Trigger::Manual, focus)
                .await
                .map(|_| ()),
            Submission::SwitchModel { tier, model } => self.switch_model(tier, model).await,
            Submission::Command { command } if command.name == "compact" => {
                let focus = (!command.args.is_empty()).then(|| command.args.join(" "));
                self.compact(compact::Trigger::Manual, focus)
                    .await
                    .map(|_| ())
            }
            Submission::Shutdown => {
                // T10.2: optional cheap extraction first; a failure warns but
                // never fails the shutdown. `SessionEnd` fires after, either way.
                if self.config.memory.extract
                    && let Err(error) = self.extract_memory().await
                {
                    self.emit(Event::Notice {
                        level: Level::Warn,
                        text: format!("memory extraction failed: {error}"),
                    })
                    .await?;
                }
                let _ = hooks::fire(self, HookEvent::SessionEnd, serde_json::json!({})).await;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// The engine's verdict for `call` under the session's current mode
    /// and grants (plan.md §1.8).
    pub(crate) async fn decide(&self, call: &ToolCall) -> Outcome {
        let inner = self.inner.lock().await;
        self.engine.decide(
            call,
            inner.permission_mode,
            self.config.permissions.approval,
            self.config.sandbox.mode,
            &inner.grants,
        )
    }

    /// The route for `job` under the session's overrides (T9.1). Pure
    /// except for the lock that reads the overrides.
    pub(crate) async fn route_for(
        &self,
        job: Job,
        confirm_think: bool,
    ) -> Result<Route, RouteError> {
        let (overrides, tier) = {
            let inner = self.inner.lock().await;
            (inner.overrides.clone(), self.tier)
        };
        Router::pick(&self.config, job, tier, &overrides, confirm_think)
    }

    /// `/model <tier> [model]` (T9.1 step 3): main turns run on `tier` with
    /// `model`, or the tier default when `None`; thinking blocks are
    /// stripped because their signatures bind to the previous model.
    pub(crate) async fn switch_model(
        &self,
        tier: Tier,
        model: Option<ModelId>,
    ) -> Result<(), CoreError> {
        let route_err = |e: RouteError| CoreError::Config {
            key: "tiers".into(),
            message: e.notice(),
        };
        let from = self
            .route_for(Job::Main, true)
            .await
            .map_err(route_err)?
            .model;
        {
            let mut inner = self.inner.lock().await;
            inner.overrides.main_tier = Some(tier);
            match model {
                Some(m) => {
                    inner.overrides.models.insert(tier, m);
                }
                None => {
                    inner.overrides.models.remove(&tier);
                }
            }
            inner.history = crate::router::strip_thinking(&inner.history);
        }
        let to = self
            .route_for(Job::Main, true)
            .await
            .map_err(route_err)?
            .model;
        self.emit(Event::ModelSwitched { tier, from, to }).await
    }

    /// Best-effort FTS index of one model-visible text (T10.3): empty
    /// texts are skipped by the store, and failures never fail turns.
    pub(crate) async fn index_text(&self, text: &str) {
        let seq = self.inner.lock().await.turn_seq;
        let _ = self.store.rollout_index(&self.id, seq, text);
    }

    /// Parks `call_id` until `Submission::Approve` answers it.
    pub(crate) async fn await_decision(&self, call_id: CallId) -> oneshot::Receiver<Decision> {
        let (tx, rx) = oneshot::channel();
        let mut inner = self.inner.lock().await;
        inner.state = State::AwaitingApproval;
        inner.pending.insert(call_id, tx);
        rx
    }

    /// Records an `AllowForSession` grant.
    pub(crate) async fn grant(&self, tool: String, subject: String) {
        self.inner.lock().await.grants.push((tool, subject));
    }

    /// Dedup bookkeeping for a read-only result; `Some` is the pointer text
    /// that replaces the payload.
    pub(crate) async fn dedup_observe(
        &self,
        tool: &str,
        input: &serde_json::Value,
        subject: &str,
        id: cox_protocol::ArchiveId,
        output: &[u8],
    ) -> Option<String> {
        let mut inner = self.inner.lock().await;
        let round = inner.round;
        inner.dedup.observe(tool, input, subject, id, round, output)
    }

    /// Forgets cached reads a write or command may have changed.
    pub(crate) async fn dedup_invalidate(&self, risk: cox_protocol::types::Risk, subject: &str) {
        self.inner.lock().await.dedup.invalidate(risk, subject);
    }

    /// Remembers where a tool result is archived for microcompaction (T8.2).
    pub(crate) async fn remember_archive(&self, call: CallId, archive: ArchiveRef) {
        self.inner.lock().await.archives.insert(call, archive);
    }

    /// What this session has spent so far, in USD.
    pub(crate) async fn spent(&self) -> f64 {
        self.inner.lock().await.spent_usd
    }

    /// Charges a subagent's (or side job's) cost to this session.
    pub(crate) async fn add_spend(&self, usd: f64) {
        self.inner.lock().await.spent_usd += usd;
    }

    /// Records deferred tools found by `tool_search`; returns the ones that
    /// are new, which is what the next request adds to its tool list.
    pub(crate) async fn discover(&self, names: impl IntoIterator<Item = String>) -> Vec<String> {
        let mut inner = self.inner.lock().await;
        let mut added = Vec::new();
        for name in names {
            if !inner.discovered.contains(&name) {
                inner.discovered.push(name.clone());
                added.push(name);
            }
        }
        added
    }

    async fn run_turn(&self, text: String, confirm_think: bool) -> Result<(), CoreError> {
        {
            let mut c = self.cancel.lock().unwrap_or_else(|e| e.into_inner());
            *c = CancellationToken::new();
        }
        let turn = TurnId::new();
        let user_item = ItemId::new();
        // §1.8 step 1: a hook may block or rewrite the prompt before it
        // touches history; a blocked prompt is still a (refused) turn so
        // every surface sees its `TurnDone`.
        let text = match hooks::fire(
            self,
            HookEvent::UserPromptSubmit,
            serde_json::json!({ "prompt": text }),
        )
        .await
        {
            HookOutcome::Block { reason } => {
                let tc = self.config.tiers.get(self.tier);
                self.emit_turn_started(turn, self.tier, ModelId(tc.model.clone()))
                    .await?;
                self.emit(Event::Notice {
                    level: Level::Warn,
                    text: format!("prompt blocked by hook: {reason}"),
                })
                .await?;
                return self
                    .finish(turn, StopReason::Refusal { detail: reason })
                    .await;
            }
            HookOutcome::Modify { input } => input.as_str().map_or(text, str::to_owned),
            _ => text,
        };
        // T9.1: the think tier needs `confirm_think`; without it the turn is
        // refused before any provider call, with the price in the notice.
        // An unknown provider name is a turn-fatal config error instead.
        let route = match self.route_for(Job::Main, confirm_think).await {
            Ok(route) => route,
            Err(RouteError::NeedsConfirm { tier, model }) => {
                self.emit_turn_started(turn, tier, model.clone()).await?;
                let detail = RouteError::NeedsConfirm { tier, model }.notice();
                self.emit(Event::Notice {
                    level: Level::Warn,
                    text: detail.clone(),
                })
                .await?;
                return self.finish(turn, StopReason::Refusal { detail }).await;
            }
            Err(e) => {
                let tc = self.config.tiers.get(self.tier);
                self.emit_turn_started(turn, self.tier, ModelId(tc.model.clone()))
                    .await?;
                self.emit(Event::Error {
                    error: CoreError::Config {
                        key: "tiers".into(),
                        message: e.notice(),
                    },
                    fatal: false,
                })
                .await?;
                return self.finish(turn, StopReason::Error).await;
            }
        };
        // §1.10 trigger, applied at the next turn's start rather than after
        // `TurnDone` so nothing follows a turn's last event (§1.3 rule 7).
        let (last, max_context) = (
            self.inner.lock().await.last_context_tokens,
            self.provider.capabilities().max_context,
        );
        if compact::needs_compaction(last, max_context, self.config.context.compact_at) {
            self.compact(compact::Trigger::Auto, None).await?;
        }
        let seq = {
            let mut inner = self.inner.lock().await;
            inner.state = State::Assembling;
            let start = inner.history.len();
            inner.history.push(Message {
                role: Role::User,
                content: vec![Content::Text { text: text.clone() }],
            });
            inner.turn_marks.push(TurnMark {
                item: user_item,
                start,
            });
            inner.provider_calls = 0;
            inner.retried_after_too_long = false;
            inner.turn_seq += 1;
            inner.turn_seq
        };
        // T10.3: index the user text under this turn's number; best-effort,
        // like every index write.
        let _ = self.store.rollout_index(&self.id, seq, &text);
        self.emit_turn_started(turn, route.tier, route.model.clone())
            .await?;
        self.emit(Event::ItemStarted {
            item: user_item,
            kind: ItemKind::UserMessage {
                text,
                attachments: vec![],
            },
        })
        .await?;
        self.emit(Event::ItemDone { item: user_item }).await?;

        loop {
            match self.step(turn).await? {
                Step::Continue => {}
                Step::Done => return Ok(()),
            }
        }
    }

    /// One provider call and its tool batch. The turn loop is just
    /// `while step() == Continue`; I/O happens only through traits.
    async fn step(&self, turn: TurnId) -> Result<Step, CoreError> {
        if self.cancel_token().is_cancelled() {
            self.set_state(State::Interrupted).await;
            self.finish(turn, StopReason::Interrupted).await?;
            return Ok(Step::Done);
        }
        let (history, calls_so_far, discovered, marks, archives) = {
            let inner = self.inner.lock().await;
            (
                inner.history.clone(),
                inner.provider_calls,
                inner.discovered.clone(),
                inner.turn_marks.iter().map(|m| m.start).collect::<Vec<_>>(),
                inner.archives.clone(),
            )
        };
        if calls_so_far >= self.config.core.max_turns {
            self.finish(turn, StopReason::MaxTurns).await?;
            return Ok(Step::Done);
        }
        {
            let mut inner = self.inner.lock().await;
            inner.state = State::Assembling;
            inner.provider_calls += 1;
            inner.round += 1;
        }
        let req_messages = crate::context::microcompact(
            &history,
            &marks,
            self.config.context.keep_turns,
            self.config.context.microcompact_after_turns,
            &archives,
        );
        // T9.1: every provider call routes through the Router; the gate
        // already passed in `run_turn`, so only a bad provider name can fail
        // here and it is turn-fatal, never silent.
        let route = match self.route_for(Job::Main, true).await {
            Ok(route) => route,
            Err(e) => {
                self.emit(Event::Error {
                    error: CoreError::Config {
                        key: "tiers".into(),
                        message: e.notice(),
                    },
                    fatal: false,
                })
                .await?;
                self.finish(turn, StopReason::Error).await?;
                return Ok(Step::Done);
            }
        };
        let mut req = assemble_with(
            &req_messages,
            &self.config,
            route.tier,
            &self.tools,
            &discovered,
            &self.cwd,
            "",
        );
        req.model = route.model.clone();
        // T8.3: hash the prefix before the request moves into the stream.
        let prefix_texts: Vec<String> = req.system.iter().map(|b| b.text.clone()).collect();
        let (spent, warned) = {
            let inner = self.inner.lock().await;
            (inner.spent_usd, inner.budget_warned)
        };
        match budget::decide(
            spent,
            self.config.budget.session_usd,
            self.config.budget.warn_at,
            warned,
        ) {
            budget::Decision::Stop => {
                self.finish(turn, StopReason::Budget).await?;
                return Ok(Step::Done);
            }
            budget::Decision::Warn => {
                {
                    self.inner.lock().await.budget_warned = true;
                }
                self.emit(Event::Notice {
                    level: Level::Budget,
                    text: format!(
                        "budget ${spent:.2} of ${:.2}",
                        self.config.budget.session_usd
                    ),
                })
                .await?;
            }
            budget::Decision::Proceed => {}
        }
        let assistant_item = ItemId::new();
        self.emit(Event::ItemStarted {
            item: assistant_item,
            kind: ItemKind::AssistantMessage {
                text: String::new(),
            },
        })
        .await?;
        self.set_state(State::Streaming).await;
        let (ptx, mut prx) = mpsc::channel(64);
        let provider = self.provider.clone();
        let cancel = self.cancel_token();
        let join = tokio::spawn(async move { provider.stream(req, ptx, cancel).await });
        let streamed = match consume_provider(self, &mut prx, assistant_item).await {
            Ok(s) => s,
            Err(e) => {
                let _ = join.await;
                self.emit(Event::Error {
                    error: e.clone(),
                    fatal: false,
                })
                .await?;
                self.finish(turn, StopReason::Error).await?;
                return Ok(Step::Done);
            }
        };
        let usage = match join.await {
            Ok(Ok(u)) => u,
            Ok(Err(error)) => {
                // §1.10: a too-long request compacts and retries once.
                let too_long = matches!(error, ProviderError::ContextTooLong { .. });
                let retried = self.inner.lock().await.retried_after_too_long;
                if too_long && !retried {
                    self.emit(Event::ItemDone {
                        item: assistant_item,
                    })
                    .await?;
                    self.inner.lock().await.retried_after_too_long = true;
                    if self.compact(compact::Trigger::ContextTooLong, None).await? {
                        return Ok(Step::Continue);
                    }
                }
                self.emit(Event::Error {
                    error: CoreError::Provider { error },
                    fatal: false,
                })
                .await?;
                self.finish(turn, StopReason::Error).await?;
                return Ok(Step::Done);
            }
            Err(_) => {
                self.set_state(State::Interrupted).await;
                self.finish(turn, StopReason::Interrupted).await?;
                return Ok(Step::Done);
            }
        };
        let usage = streamed.usage.unwrap_or(usage);
        self.inner.lock().await.last_context_tokens = usage
            .input_tokens
            .saturating_add(usage.cache_read_tokens)
            .saturating_add(usage.cache_write_tokens)
            .saturating_add(usage.output_tokens);
        // T8.3: cache share for the status line; a 0-read after a hit diffs
        // the prefix hashes and names the block that broke it.
        let miss = {
            let mut inner = self.inner.lock().await;
            inner.cache_ratio = crate::cache_diag::ratio_of(&usage);
            inner.cache.observe(&prefix_texts, &usage)
        };
        if let Some(text) = miss {
            self.emit(Event::Notice {
                level: Level::Info,
                text,
            })
            .await?;
        }
        self.store
            .usage_insert(&cox_protocol::UsageRow {
                session_id: self.id,
                turn: calls_so_far + 1,
                job: self.job,
                tier: route.tier,
                provider: self.provider.id(),
                model: route.model.clone(),
                usage,
            })
            .map_err(|error| CoreError::Store { error })?;
        if budget::counts(route.tier, self.config.budget.cheap_counts) {
            self.inner.lock().await.spent_usd += usage.cost_usd;
        }
        self.emit(Event::Usage { turn, usage }).await?;
        self.emit(Event::ItemDone {
            item: assistant_item,
        })
        .await?;

        if streamed.calls.is_empty() {
            if !streamed.text.is_empty() {
                let mut inner = self.inner.lock().await;
                inner.history.push(Message {
                    role: Role::Assistant,
                    content: vec![Content::Text {
                        text: streamed.text.clone(),
                    }],
                });
            }
            self.index_text(&streamed.text).await;
            self.finish(turn, StopReason::EndTurn).await?;
            return Ok(Step::Done);
        }

        {
            let mut inner = self.inner.lock().await;
            inner.history.push(Message {
                role: Role::Assistant,
                content: {
                    let mut blocks = Vec::new();
                    if !streamed.text.is_empty() {
                        blocks.push(Content::Text {
                            text: streamed.text.clone(),
                        });
                    }
                    for (id, name, input) in &streamed.calls {
                        blocks.push(Content::ToolUse {
                            id: *id,
                            name: name.clone(),
                            input: input.clone(),
                        });
                    }
                    blocks
                },
            });
            inner.state = State::RunningTools;
        }
        let results = run_tools(self, streamed.calls).await?;
        if self.cancel_token().is_cancelled() {
            self.set_state(State::Interrupted).await;
            self.finish(turn, StopReason::Interrupted).await?;
            return Ok(Step::Done);
        }
        let msg = results_message(results);
        // T10.3: tool results are user-role text the user will grep for.
        let joined: String = msg
            .content
            .iter()
            .filter_map(|c| match c {
                Content::ToolResult { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        {
            let mut inner = self.inner.lock().await;
            inner.history.push(msg);
        }
        self.index_text(&joined).await;
        Ok(Step::Continue)
    }

    async fn emit_turn_started(
        &self,
        turn: TurnId,
        tier: Tier,
        model: ModelId,
    ) -> Result<(), CoreError> {
        self.emit(Event::TurnStarted {
            turn,
            job: self.job,
            tier,
            model,
        })
        .await
    }

    pub(crate) async fn set_state(&self, state: State) {
        self.inner.lock().await.state = state;
    }

    async fn finish(&self, turn: TurnId, stop: StopReason) -> Result<(), CoreError> {
        {
            let mut inner = self.inner.lock().await;
            inner.state = State::Finishing;
        }
        if stop == StopReason::EndTurn {
            // §1.8 step 4: `Stop` is informational; its verdict is not applied.
            let _ = hooks::fire(self, HookEvent::Stop, serde_json::json!({})).await;
        }
        self.emit(Event::TurnDone { turn, stop }).await?;
        {
            let mut inner = self.inner.lock().await;
            inner.state = State::Idle;
        }
        Ok(())
    }
}

/// In-memory store for loop tests: no SQLite, same trait.
pub struct MemoryStore {
    events: StdMutex<Vec<Event>>,
    usage: StdMutex<Vec<cox_protocol::UsageRow>>,
    archive: StdMutex<HashMap<cox_protocol::ArchiveId, Vec<u8>>>,
    /// `(project, name)` → `(path, body)` for `memory_*` (T10.1).
    memory: StdMutex<HashMap<(String, String), (String, String)>>,
    /// `(session, turn, text)` FTS rows (T10.3).
    index: StdMutex<Vec<(String, u32, String)>>,
}

impl MemoryStore {
    /// Empty ledger and rollout.
    pub fn new() -> Self {
        Self {
            events: StdMutex::new(Vec::new()),
            usage: StdMutex::new(Vec::new()),
            archive: StdMutex::new(HashMap::new()),
            memory: StdMutex::new(HashMap::new()),
            index: StdMutex::new(Vec::new()),
        }
    }

    /// Ledger rows written for this session (test assertion).
    pub fn usage_rows(&self) -> Vec<cox_protocol::UsageRow> {
        self.usage.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Indexed texts (test assertion for T10.3 call sites).
    pub fn indexed_texts(&self) -> Vec<(String, u32, String)> {
        self.index.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Store for MemoryStore {
    fn open(_home: &std::path::Path) -> Result<Self, StoreError> {
        Ok(Self::new())
    }
    fn session_create(&self, _s: &cox_protocol::SessionRow) -> Result<(), StoreError> {
        Ok(())
    }
    fn rollout_append(&self, _id: &SessionId, ev: &Event) -> Result<u64, StoreError> {
        let mut events = self.events.lock().unwrap_or_else(|e| e.into_inner());
        events.push(ev.clone());
        Ok(events.len() as u64)
    }
    fn rollout_read(&self, _id: &SessionId) -> Result<Vec<Event>, StoreError> {
        Ok(self
            .events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone())
    }
    fn usage_insert(&self, row: &cox_protocol::UsageRow) -> Result<(), StoreError> {
        self.usage
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(row.clone());
        Ok(())
    }
    fn archive_put(&self, a: &ArchivePut) -> Result<cox_protocol::ArchiveId, StoreError> {
        let id = cox_protocol::ArchiveId::new();
        self.archive
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, a.bytes.clone());
        Ok(id)
    }
    fn archive_get(&self, id: &cox_protocol::ArchiveId) -> Result<Vec<u8>, StoreError> {
        self.archive
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .cloned()
            .ok_or(StoreError::NotFound)
    }
    fn memory_search(
        &self,
        q: &str,
        limit: usize,
    ) -> Result<Vec<cox_protocol::MemoryHit>, StoreError> {
        // Substring stand-in for FTS5: every term must appear in the name or
        // body, most hits first. The real ranking lives in `cox-store`.
        let terms: Vec<String> = q.split_whitespace().map(str::to_lowercase).collect();
        let mut hits: Vec<(usize, String, String, String)> = self
            .memory
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter_map(|((_, name), (path, body))| {
                if terms.is_empty() {
                    return None;
                }
                let hay = format!("{name}\n{body}").to_lowercase();
                if !terms.iter().all(|t| hay.contains(t)) {
                    return None;
                }
                let score = terms.iter().map(|t| hay.matches(t).count()).sum();
                let snippet: String = body.chars().take(200).collect();
                Some((score, name.clone(), path.clone(), snippet))
            })
            .collect();
        hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        Ok(hits
            .into_iter()
            .take(limit.max(1))
            .map(|(_, name, path, snippet)| cox_protocol::MemoryHit {
                name,
                path: path.into(),
                snippet,
            })
            .collect())
    }
    fn memory_upsert(
        &self,
        project: &str,
        name: &str,
        path: &str,
        _kind: &str,
        body: &str,
    ) -> Result<(), StoreError> {
        self.memory
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                (project.to_string(), name.to_string()),
                (path.to_string(), body.to_string()),
            );
        Ok(())
    }
    fn rollout_index(&self, session: &SessionId, turn: u32, text: &str) -> Result<(), StoreError> {
        if text.trim().is_empty() {
            return Ok(());
        }
        self.index.lock().unwrap_or_else(|e| e.into_inner()).push((
            session.to_string(),
            turn,
            text.to_string(),
        ));
        Ok(())
    }
}

#[async_trait::async_trait]
impl Archive for MemoryStore {
    async fn put(&self, put: ArchivePut) -> Result<cox_protocol::ArchiveId, StoreError> {
        self.archive_put(&put)
    }
    async fn get(&self, id: &cox_protocol::ArchiveId) -> Result<Vec<u8>, StoreError> {
        self.archive_get(id)
    }
}
