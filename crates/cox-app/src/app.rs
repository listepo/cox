//! The session owner (DT§4.3, §4.5): one [`App`] per process holds the
//! workspace, the inbox across sessions and the [`Host`], and opens and
//! resumes sessions through `cox-session`; [`crate::live`] runs each one.
//! Here rather than in `cox-ffi` (T37.39, A71) so the FFI only forwards and
//! this logic is tested in Rust once, without a foreign language.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use cox_protocol::StoreError;
use cox_protocol::errors::CoreError;
use cox_protocol::ids::SessionId;
use cox_protocol::types::Event;
use cox_session::SessionError;
use cox_store::lock::Holder;

use crate::live::LiveSession;
use crate::{Activity, Inbox, InboxItem, IntentError, SettingsError, SettingsView};
use crate::{Workspace, WorkspaceError};

/// DT§4.8: how long the login shell may take before its env is skipped.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(10);

/// What the app asks the platform for (DT§4.4's `Host`). Plain Rust, so a
/// test implements it in memory; `cox-ffi` adapts the Swift one.
pub trait Host: Send + Sync {
    /// A new inbox item arrived; `badge` is the count that blocks a turn.
    fn notify(&self, item: InboxItem, badge: u32);
    /// The badge fell with no new item to carry it: an approval or question
    /// was answered, or its session closed.
    fn badge(&self, badge: u32);
    /// An MCP server's login page, or a link the person asked to follow.
    fn open_url(&self, url: &str);
    /// The stored secret for a provider section (`anthropic`, `openai`, a
    /// `[providers.<name>]`); its env var, when set, wins.
    fn secret(&self, section: &str) -> Option<String>;
}

/// What opening, resuming or driving a session can fail with.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// T37.34: another process drives it; follow it read-only or fork it.
    #[error("session {id} is open in {holder}")]
    Busy { id: SessionId, holder: Holder },
    #[error(transparent)]
    Session(SessionError),
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Intent(#[from] IntentError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Settings(#[from] SettingsError),
    #[error("the session's events were already taken")]
    EventsTaken,
}

impl From<SessionError> for AppError {
    fn from(e: SessionError) -> Self {
        match e {
            SessionError::SessionBusy { id, holder } => Self::Busy { id, holder },
            e => Self::Session(e),
        }
    }
}

/// DT§4.8: reads the login shell's environment into this process, so tools
/// find `cargo` and `mise` and env-var keys resolve as in a terminal.
/// Call once at launch, before any session opens. Returns why it fell back
/// to the inherited environment, if it did.
pub async fn load_login_env() -> Option<String> {
    let (env, warning) = cox_session::env::login_env(LOGIN_TIMEOUT).await;
    for (key, value) in env {
        // SAFETY: the one hazard is another thread reading the environment
        // meanwhile. This runs once at launch, before any session exists,
        // so cox's own threads are idle; Rust's env reads share std's lock.
        unsafe { std::env::set_var(key, value) };
    }
    warning.map(|w| w.to_string())
}

/// One per process.
pub struct App {
    pub(crate) home: PathBuf,
    pub(crate) host: Arc<dyn Host>,
    inbox: Mutex<Inbox>,
    workspace: Workspace,
}

impl App {
    /// `home` is `COX_HOME`; `None` means `~/.cox`.
    pub fn new(home: Option<PathBuf>, host: Arc<dyn Host>) -> Result<Arc<Self>, AppError> {
        let home = home.unwrap_or_else(cox_config::load::cox_home);
        let workspace = Workspace::open(&home, Arc::new(cox_tools::git::GitWorktrees))?;
        Ok(Arc::new(Self {
            home,
            host,
            inbox: Mutex::default(),
            workspace,
        }))
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// Most urgent first, oldest first within a rank.
    pub fn inbox(&self) -> Vec<InboxItem> {
        self.lock_inbox().items().into_iter().cloned().collect()
    }

    /// The Dock badge.
    pub fn badge(&self) -> u32 {
        count(self.lock_inbox().badge())
    }

    pub fn activity(&self, session: SessionId) -> Activity {
        self.lock_inbox().activity(session)
    }

    pub fn dismiss(&self, session: SessionId, seq: u64) {
        self.lock_inbox().dismiss(session, seq);
    }

    /// A new session in `cwd`, or `resume`'s with its blocks; `theme` is the
    /// syntect theme code blocks are highlighted with. Call on a tokio
    /// runtime: the session's drain and inbox tasks are spawned there.
    pub async fn open(
        self: &Arc<Self>,
        cwd: PathBuf,
        resume: Option<SessionId>,
        theme: String,
    ) -> Result<Arc<LiveSession>, AppError> {
        let resume = match resume {
            Some(id) => Some((id, cox_session::resume(&self.home, id)?)),
            None => None,
        };
        LiveSession::open(Arc::clone(self), cwd, resume, theme).await
    }

    /// The Settings screen for a session in `cwd` (DT§5.7).
    pub fn settings(&self, cwd: &Path) -> Result<SettingsView, AppError> {
        Ok(crate::settings::view(&self.user_config(), cwd)?)
    }

    /// Sets `key` to `json` in this home's `config.toml`; the new view.
    pub fn set_setting(&self, cwd: &Path, key: &str, json: &str) -> Result<SettingsView, AppError> {
        Ok(crate::settings::set(&self.user_config(), cwd, key, json)?)
    }

    /// This home's `config.toml`, what sessions and Settings both read.
    pub(crate) fn user_config(&self) -> PathBuf {
        self.home.join("config.toml")
    }

    fn lock_inbox(&self) -> MutexGuard<'_, Inbox> {
        self.inbox.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Folds `event` into the inbox and tells the host about each new item,
    /// or the lower badge once one is answered, outside the lock so the host
    /// may read the inbox back.
    pub(crate) fn apply(&self, session: SessionId, event: &Event) {
        let (fresh, badge, fell) = {
            let mut inbox = self.lock_inbox();
            let last = inbox.items().iter().map(|i| i.seq).max();
            let before = inbox.badge();
            inbox.apply(session, event);
            let fresh: Vec<InboxItem> = inbox
                .items()
                .into_iter()
                .filter(|i| last.is_none_or(|last| i.seq > last))
                .cloned()
                .collect();
            (fresh, count(inbox.badge()), inbox.badge() < before)
        };
        for item in fresh {
            self.host.notify(item, badge);
        }
        if fell {
            self.host.badge(badge);
        }
    }

    pub(crate) fn expire(&self, session: SessionId) {
        let (badge, fell) = {
            let mut inbox = self.lock_inbox();
            let before = inbox.badge();
            inbox.expire(session);
            (count(inbox.badge()), inbox.badge() < before)
        };
        if fell {
            self.host.badge(badge);
        }
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}
