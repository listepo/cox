//! `Session`: Submission in, Event out. The only type `cox-tui` / `cox run`
//! / ACP should talk to; they never call a provider or tool themselves.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};

use cox_protocol::errors::{CoreError, StoreError};
use cox_protocol::ids::{CallId, ItemId, SessionId, TurnId};
use cox_protocol::traits::{Archive, ArchivePut, Provider, Store, Tool};
use cox_protocol::types::{
    Content, Decision, Event, ItemKind, Job, Level, Message, ModelId, PermissionMode, Role,
    StopReason, Submission, Tier, ToolCall,
};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::budget;
use crate::context::assemble;
use crate::dedup::Dedup;
use crate::permission::{Engine, Outcome};
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

struct Inner {
    state: State,
    history: Vec<Message>,
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
    pub(crate) cancel: Arc<StdMutex<CancellationToken>>,
    tx: mpsc::Sender<Event>,
    rx: Arc<StdMutex<Option<mpsc::Receiver<Event>>>>,
    inner: Arc<Mutex<Inner>>,
}

impl Session {
    /// Constructs a session and emits `SessionStarted`.
    pub fn new(
        config: cox_protocol::Config,
        provider: Arc<dyn Provider>,
        tools: Vec<Arc<dyn Tool>>,
        store: Arc<dyn Store>,
        archive: Arc<dyn Archive>,
        cwd: PathBuf,
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
            cancel: Arc::new(StdMutex::new(CancellationToken::new())),
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
                parent_id: None,
                rollout_path: PathBuf::new(),
            })
            .map_err(|error| CoreError::Store { error })?;
        session.store.rollout_append(&id, &started).ok();
        let _ = session.tx.try_send(started);
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

    /// Feeds one submission into the state machine.
    pub async fn submit(&self, sub: Submission) -> Result<(), CoreError> {
        match sub {
            Submission::UserTurn {
                text,
                confirm_think: _,
                attachments: _,
            } => self.run_turn(text).await,
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
            Submission::Shutdown => Ok(()),
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
            &inner.grants,
        )
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

    async fn run_turn(&self, text: String) -> Result<(), CoreError> {
        {
            let mut c = self.cancel.lock().unwrap_or_else(|e| e.into_inner());
            *c = CancellationToken::new();
        }
        let turn = TurnId::new();
        let user_item = ItemId::new();
        {
            let mut inner = self.inner.lock().await;
            inner.state = State::Assembling;
            inner.history.push(Message {
                role: Role::User,
                content: vec![Content::Text { text: text.clone() }],
            });
            inner.provider_calls = 0;
        }
        self.emit(Event::TurnStarted {
            turn,
            job: Job::Main,
            tier: Tier::Code,
            model: ModelId(self.config.tiers.code.model.clone()),
        })
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
        let (history, calls_so_far) = {
            let inner = self.inner.lock().await;
            (inner.history.clone(), inner.provider_calls)
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
        let req = assemble(&history, &self.config, &self.tools, &self.cwd, "");
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
        self.store
            .usage_insert(&cox_protocol::UsageRow {
                session_id: self.id,
                turn: calls_so_far + 1,
                job: Job::Main,
                tier: Tier::Code,
                provider: self.provider.id(),
                model: ModelId(self.config.tiers.code.model.clone()),
                usage,
            })
            .map_err(|error| CoreError::Store { error })?;
        if budget::counts(Tier::Code, self.config.budget.cheap_counts) {
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
                        text: streamed.text,
                    }],
                });
            }
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
        {
            let mut inner = self.inner.lock().await;
            inner.history.push(msg);
        }
        Ok(Step::Continue)
    }

    pub(crate) async fn set_state(&self, state: State) {
        self.inner.lock().await.state = state;
    }

    async fn finish(&self, turn: TurnId, stop: StopReason) -> Result<(), CoreError> {
        {
            let mut inner = self.inner.lock().await;
            inner.state = State::Finishing;
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
}

impl MemoryStore {
    /// Empty ledger and rollout.
    pub fn new() -> Self {
        Self {
            events: StdMutex::new(Vec::new()),
            usage: StdMutex::new(Vec::new()),
            archive: StdMutex::new(HashMap::new()),
        }
    }

    /// Ledger rows written for this session (test assertion).
    pub fn usage_rows(&self) -> Vec<cox_protocol::UsageRow> {
        self.usage.lock().unwrap_or_else(|e| e.into_inner()).clone()
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
        _q: &str,
        _limit: usize,
    ) -> Result<Vec<cox_protocol::MemoryHit>, StoreError> {
        Ok(vec![])
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
