//! One open session behind the FFI (DT§4.4, §4.5): builds it through
//! `cox-session`, feeds its events to the inbox and to `cox-app`'s
//! controller, and runs each intent the way `dispatch` says — a turn
//! spawned and never awaited, a queued turn after the running one, and a
//! fork or handoff as a new session. Separate from `lib.rs`, which owns
//! what outlives one session.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use cox_app::{Block, Completer, Completion, Controller, Dispatch, Intent};
use cox_app::{Timeline, TimelinePatch, dispatch};
use cox_core::{History, Session};
use cox_protocol::ids::SessionId;
use cox_protocol::traits::Store as _;
use cox_protocol::types::{Event, Submission};
use cox_session::SessionSpec;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::{AppError, Shared, host, on_runtime};

/// The core's own bound (DT§4.5).
const EVENTS: usize = 256;

#[derive(uniffi::Object)]
pub struct SessionHandle {
    shared: Arc<Shared>,
    session: Session,
    controller: Arc<Controller>,
    completer: Completer,
    cwd: PathBuf,
    theme: String,
    warnings: Vec<String>,
    /// The turn spawned last; a queued one starts after it.
    turn: Mutex<Option<JoinHandle<()>>>,
}

/// Opens a session in `cwd` (resuming `resume`) and starts draining it.
/// Runs on the runtime: the controller spawns its drain there.
pub(crate) async fn open(
    shared: Arc<Shared>,
    cwd: PathBuf,
    resume: Option<(SessionId, History)>,
    theme: String,
) -> Result<Arc<SessionHandle>, AppError> {
    let mut timeline = Timeline::new(&theme);
    if let Some((id, _)) = &resume {
        for event in cox_store::Store::open(&shared.home)?.rollout_read(id)? {
            timeline.apply(&event);
        }
    }
    // The Claude-settings layer is read only by `crates/cox`.
    let flags = serde_json::Value::Object(serde_json::Map::new());
    let config = cox_config::load::load(&cwd, &flags, |_| None)?.config;
    let spec = SessionSpec {
        config,
        cwd: cwd.clone(),
        home: shared.home.clone(),
        worktree: false,
        answer: None,
        questions: true,
        resume,
        mcp_login: Some(host::login(&shared.host)),
        plugin_ui: None,
        client: None,
        surface: "app".into(),
    };
    let opened = cox_session::open_with_keys(spec, Some(host::keys(&shared.host))).await?;
    let session = opened.session;
    let events = session.events().ok_or_else(|| AppError::Session {
        message: "the session's events were already taken".into(),
    })?;
    let events = tee(Arc::clone(&shared), session.id(), events);
    let claude_home = cox_config::load::home_dir().join(".claude");
    Ok(Arc::new(SessionHandle {
        completer: Completer::load(&cwd, &shared.home, &claude_home),
        controller: Arc::new(Controller::spawn(timeline, events)),
        warnings: opened.warnings.iter().map(ToString::to_string).collect(),
        turn: Mutex::new(None),
        shared,
        session,
        cwd,
        theme,
    }))
}

/// The inbox folds every event before the timeline sees it. A closed
/// timeline refuses at once, so the core never waits on a view.
fn tee(
    shared: Arc<Shared>,
    id: SessionId,
    mut from: mpsc::Receiver<Event>,
) -> mpsc::Receiver<Event> {
    let (tx, rx) = mpsc::channel(EVENTS);
    tokio::spawn(async move {
        while let Some(event) = from.recv().await {
            shared.apply(id, &event);
            // Err only once the view closed; the inbox keeps following.
            let _ = tx.send(event).await;
        }
        shared.expire(id);
    });
    rx
}

#[uniffi::export]
impl SessionHandle {
    pub fn id(&self) -> SessionId {
        self.session.id()
    }

    /// What was skipped while the session was built (D14).
    pub fn warnings(&self) -> Vec<String> {
        self.warnings.clone()
    }

    /// Every block; the next pull continues from here.
    pub fn snapshot(&self) -> Vec<Block> {
        self.controller.snapshot()
    }

    /// The next batch, at most one per frame; `None` once closed.
    pub async fn next_patches(&self) -> Option<Vec<TimelinePatch>> {
        let controller = Arc::clone(&self.controller);
        on_runtime(async move { controller.next_patches().await })
            .await
            .ok()
            .flatten()
    }

    /// Returns at once for a turn; a fork or handoff returns its child.
    pub async fn send(
        self: Arc<Self>,
        intent: Intent,
    ) -> Result<Option<Arc<SessionHandle>>, AppError> {
        on_runtime(async move { self.run(intent).await }).await?
    }

    /// `/` commands and `@` files for the composer's token.
    pub fn complete(&self, token: String, limit: u32) -> Vec<Completion> {
        let limit = usize::try_from(limit).unwrap_or(usize::MAX);
        self.completer.complete(&token, limit)
    }

    /// Stops the pull; the session keeps running (DT§4.5).
    pub fn close(&self) {
        self.controller.close();
    }

    /// Quitting: ends every turn and kills what it detached (T38.2).
    pub fn end(&self) {
        self.session.end();
        self.close();
    }
}

impl SessionHandle {
    async fn run(&self, intent: Intent) -> Result<Option<Arc<SessionHandle>>, AppError> {
        let parent = self.session.id();
        let home = &self.shared.home;
        let child = match dispatch(intent)? {
            Dispatch::Submit {
                submission,
                spawn: false,
            } => {
                self.session.submit(submission).await?;
                return Ok(None);
            }
            Dispatch::Submit { submission, .. } => {
                self.spawn(submission, false);
                return Ok(None);
            }
            Dispatch::Queue(submission) => {
                self.spawn(submission, true);
                return Ok(None);
            }
            Dispatch::Fork { turn } => cox_session::fork(home, &self.cwd, parent, turn)?,
            Dispatch::Handoff { objective } => {
                let summary = self.session.handoff_summary(&objective).await;
                cox_session::handoff(home, &self.cwd, parent, &objective, summary.as_deref())?
            }
        };
        let (shared, cwd) = (Arc::clone(&self.shared), self.cwd.clone());
        open(shared, cwd, Some(child), self.theme.clone())
            .await
            .map(Some)
    }

    /// Spawns a turn (R9.4.3); `queued` waits for the one spawned last.
    fn spawn(&self, submission: Submission, queued: bool) {
        let session = self.session.clone();
        let mut last = self.turn.lock().unwrap_or_else(PoisonError::into_inner);
        let before = last.take().filter(|_| queued);
        *last = Some(tokio::spawn(async move {
            if let Some(before) = before {
                let _ = before.await;
            }
            // A turn that fails says so in the event stream.
            let _ = session.submit(submission).await;
        }));
    }
}
