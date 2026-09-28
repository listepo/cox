//! One live session (DT§4.5): built through `cox-session` with the host's
//! keys and login prompt, its events folded into the app's inbox and fed to
//! a [`Controller`], and each intent run the way [`dispatch`] says — a turn
//! spawned and never awaited, a queued turn after the running one, a fork
//! or handoff opened as a new session. Separate from `app.rs`, which owns
//! what outlives one session.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use cox_core::{History, Session};
use cox_protocol::Checkpointer as _;
use cox_protocol::ids::{SessionId, TaskId};
use cox_protocol::traits::Store as _;
use cox_protocol::types::{Event, Submission, TodoItem};
use cox_render::diffmodel::DiffModel;
use cox_session::SessionSpec;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::app::{App, AppError};
use crate::changes::{self, Changes};
use crate::costs::{self, TurnCosts};
use crate::info::{self, Info};
use crate::review;
use crate::status::StatusFold;
use crate::tasks::{self, TaskTarget};
use crate::{Block, Completer, Completion, Controller, Dispatch, Intent, Timeline};
use crate::{TimelinePatch, dispatch};

/// The core's own bound (DT§4.5).
const EVENTS: usize = 256;

pub struct LiveSession {
    app: Arc<App>,
    session: Session,
    /// Shared with each queued turn, which reports when it starts.
    controller: Arc<Controller>,
    completer: Completer,
    cwd: PathBuf,
    /// The workspace roots a path Review reads is confined to.
    roots: Vec<PathBuf>,
    theme: String,
    warnings: Vec<String>,
    /// The turn spawned last; a queued one starts after it.
    turn: Mutex<Option<JoinHandle<()>>>,
}

impl LiveSession {
    /// Opens a session in `cwd` (resuming `resume`) and starts draining it.
    pub(crate) async fn open(
        app: Arc<App>,
        cwd: PathBuf,
        resume: Option<(SessionId, History)>,
        theme: String,
    ) -> Result<Arc<Self>, AppError> {
        let config = app.config(&cwd)?;
        let mut timeline = Timeline::new(&theme);
        let mut status = StatusFold::open(&config);
        if let Some((id, _)) = &resume {
            for event in cox_store::Store::open(&app.home)?.rollout_read(id)? {
                timeline.apply(&event);
                status.apply(&event);
            }
        }
        let (login, keys) = (Arc::clone(&app.host), Arc::clone(&app.host));
        let spec = SessionSpec {
            config,
            cwd: cwd.clone(),
            home: app.home.clone(),
            worktree: false,
            answer: None,
            questions: true,
            resume,
            mcp_login: Some(Arc::new(move |url: &str| login.open_url(url))),
            plugin_ui: None,
            client: None,
            surface: "app".into(),
        };
        let keys: cox_session::Keys = Arc::new(move |section: &str| keys.secret(section));
        let opened = cox_session::open_with_keys(spec, Some(keys)).await?;
        let session = opened.session;
        let events = session.events().ok_or(AppError::EventsTaken)?;
        let events = tee(Arc::clone(&app), session.id(), events);
        let claude_home = cox_config::load::home_dir().join(".claude");
        Ok(Arc::new(Self {
            completer: Completer::load(&cwd, &app.home, &claude_home),
            controller: Arc::new(Controller::open(timeline, status, events)),
            warnings: opened.warnings.iter().map(ToString::to_string).collect(),
            turn: Mutex::new(None),
            roots: opened.config.core.workspace_roots,
            app,
            session,
            cwd,
            theme,
        }))
    }

    pub fn id(&self) -> SessionId {
        self.session.id()
    }

    /// What was skipped while the session was built (D14).
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Every block; the next pull continues from here.
    pub fn snapshot(&self) -> Vec<Block> {
        self.controller.snapshot()
    }

    /// The next batch, at most one per frame; `None` once closed.
    pub async fn next_patches(&self) -> Option<Vec<TimelinePatch>> {
        self.controller.next_patches().await
    }

    /// Returns at once for a turn; a fork or handoff returns its child.
    pub async fn send(&self, intent: Intent) -> Result<Option<Arc<Self>>, AppError> {
        let parent = self.session.id();
        let home = &self.app.home;
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
        let (app, cwd) = (Arc::clone(&self.app), self.cwd.clone());
        Self::open(app, cwd, Some(child), self.theme.clone())
            .await
            .map(Some)
    }

    /// What the inspector's Changes tab lists (T37.29.1): the blocks, the
    /// checkpoint rows, the rollout's `write` inputs (T37.29.7) and, through
    /// git, the linked worktree.
    pub async fn changes(&self) -> Result<Changes, AppError> {
        let store = self.app.workspace().store();
        let rows = store.checkpoint_rows(&self.id())?;
        let written = changes::written(&store.rollout_read(&self.id())?);
        let worktree = cox_tools::git::linked(&self.cwd).await;
        Ok(changes::build(
            &self.snapshot(),
            &rows,
            &written,
            &self.canonical_cwd(),
            worktree,
        ))
    }

    /// Review's diff of a Changes-tab `path` (T37.28.2, A101): the net
    /// change from the session's first checkpoint copy to the file on disk,
    /// read the way a checkpoint reads it, confined to the workspace roots.
    /// `None` for a path the session never changed or a side over the cap.
    pub async fn review(&self, path: &str) -> Result<Option<DiffModel>, AppError> {
        let checkpointer = cox_tools::checkpoint::GitCheckpointer::new(self.app.home.clone());
        let paths = [path.to_owned()];
        let Some(now) = checkpointer
            .preimages(&self.roots, &self.cwd, &paths)
            .await
            .pop()
        else {
            return Ok(None);
        };
        let store = self.app.workspace().store();
        let rows = store.checkpoint_rows(&self.id())?;
        let Some(row) = review::base(&rows, &now.path) else {
            return Ok(None);
        };
        let before = review::kept(row, |id| store.archive_get(id))?;
        let cwd = self.canonical_cwd();
        let shown = now.path.strip_prefix(&cwd).unwrap_or(&now.path);
        Ok(review::diff(shown, &before, &now.before, &self.theme))
    }

    /// Checkpoint paths are confined, so canonical; paths are shown
    /// relative to this.
    fn canonical_cwd(&self) -> PathBuf {
        std::fs::canonicalize(&self.cwd).unwrap_or_else(|_| self.cwd.clone())
    }

    /// What the inspector's Plan tab lists (T37.29.2): the todo list the
    /// latest `todo` call left, as the timeline folded it.
    pub fn plan(&self) -> Vec<TodoItem> {
        self.controller.plan()
    }

    /// What a Tasks-tab click on `task` opens (T37.29.6): read from this
    /// session's rollout and its children in the store.
    pub fn open_task(&self, task: TaskId) -> Result<Option<TaskTarget>, AppError> {
        let store = self.app.workspace().store();
        let events = store.rollout_read(&self.id())?;
        let children = store.children(&self.id())?;
        Ok(tasks::open(&events, task, &children, |child| {
            // A child whose rollout cannot be read is paired with no task.
            store
                .rollout_read(child)
                .ok()
                .and_then(|e| tasks::first_prompt(&e))
        }))
    }

    /// The Context tab's cost history (T37.29.3.2): this session's ledger
    /// rows by turn, with its subagents' rows from their child sessions.
    pub fn turn_costs(&self) -> Result<TurnCosts, AppError> {
        let store = self.app.workspace().store();
        let own = store.usage_ledger(&self.id())?;
        let children = store.children(&self.id())?;
        let children = children
            .iter()
            .map(|child| store.usage_ledger(child))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(costs::build(&own, &children))
    }

    /// What the inspector's Info tab lists (T37.29.5): the id, cwd and
    /// rollout file, the linked worktree through git, and the config layers
    /// with `cox-config`'s provenance as Settings reads it.
    pub async fn info(&self) -> Result<Info, AppError> {
        let settings = crate::settings::view(&self.app.user_config(), &self.cwd)?;
        let worktree = cox_tools::git::linked(&self.cwd).await;
        let rollout = self.app.workspace().store().rollout_path(&self.id());
        Ok(info::build(
            self.id(),
            &self.cwd,
            worktree,
            &settings,
            rollout,
        ))
    }

    /// `/` commands and `@` files for the composer's token.
    pub fn complete(&self, token: &str, limit: usize) -> Vec<Completion> {
        self.completer.complete(token, limit)
    }

    /// This session's earlier prompts, newest first, for ↑ in an empty
    /// composer (T37.24.6).
    pub fn history(&self, limit: usize) -> Result<Vec<String>, AppError> {
        Ok(self.app.workspace().prompts(self.id(), limit)?)
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

    /// Spawns a turn (R9.4.3); `queued` waits for the one spawned last and
    /// counts in the status patch until it starts.
    fn spawn(&self, submission: Submission, queued: bool) {
        let session = self.session.clone();
        let controller = queued.then(|| Arc::clone(&self.controller));
        let mut last = self.turn.lock().unwrap_or_else(PoisonError::into_inner);
        let before = last.take().filter(|_| queued);
        if let Some(controller) = &controller {
            controller.enqueue();
        }
        *last = Some(tokio::spawn(async move {
            if let Some(before) = before {
                let _ = before.await;
            }
            if let Some(controller) = controller {
                controller.dequeue();
            }
            // A turn that fails says so in the event stream.
            let _ = session.submit(submission).await;
        }));
    }
}

/// The inbox folds every event before the timeline sees it. A closed
/// timeline refuses at once, so the core never waits on a view.
fn tee(app: Arc<App>, id: SessionId, mut from: mpsc::Receiver<Event>) -> mpsc::Receiver<Event> {
    let (tx, rx) = mpsc::channel(EVENTS);
    tokio::spawn(async move {
        while let Some(event) = from.recv().await {
            app.apply(id, &event);
            // Err only once the view closed; the inbox keeps following.
            let _ = tx.send(event).await;
        }
        app.expire(id);
    });
    rx
}
