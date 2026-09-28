//! One live session (DT§4.5): built through `cox-session` with the host's
//! keys and login prompt, its events folded into the app's inbox and fed to
//! a [`Controller`], and each intent run the way [`dispatch`] says — a turn
//! spawned and never awaited, a queued turn after the running one, a fork
//! or handoff opened as a new session. Separate from `app.rs`, which owns
//! what outlives one session.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use cox_core::{History, Session};
use cox_protocol::ids::SessionId;
use cox_protocol::traits::Store as _;
use cox_protocol::types::{Event, Submission};
use cox_session::SessionSpec;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::app::{App, AppError};
use crate::{Block, Completer, Completion, Controller, Dispatch, Intent, Timeline};
use crate::{TimelinePatch, dispatch};

/// The core's own bound (DT§4.5).
const EVENTS: usize = 256;

pub struct LiveSession {
    app: Arc<App>,
    session: Session,
    controller: Controller,
    completer: Completer,
    cwd: PathBuf,
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
        let mut timeline = Timeline::new(&theme);
        if let Some((id, _)) = &resume {
            for event in cox_store::Store::open(&app.home)?.rollout_read(id)? {
                timeline.apply(&event);
            }
        }
        let config = app.config(&cwd)?;
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
            controller: Controller::spawn(timeline, events),
            warnings: opened.warnings.iter().map(ToString::to_string).collect(),
            turn: Mutex::new(None),
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

    /// `/` commands and `@` files for the composer's token.
    pub fn complete(&self, token: &str, limit: usize) -> Vec<Completion> {
        self.completer.complete(token, limit)
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
