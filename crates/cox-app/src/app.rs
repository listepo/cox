//! The session owner (DT§4.3, §4.5): one [`App`] per process holds the
//! workspace, the inbox across sessions and the [`Host`], and opens and
//! resumes sessions through `cox-session`; [`crate::live`] runs each one.
//! Here rather than in `cox-ffi` (T37.39, A86) so the FFI only forwards and
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
use crate::mcp_login::{LoginError, McpAuth};
use crate::{Activity, Inbox, InboxItem, IntentError, SettingsError, SettingsView};
use crate::{Workspace, WorkspaceError};

/// DT§4.8: how long the login shell may take before its env is skipped.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(10);

/// How often [`App::workspace_changed`] asks `cox.db` whether another
/// connection committed: one `PRAGMA data_version`, so cheap enough to ask
/// often, and quick enough that a TUI's new session shows at once.
const WATCH_INTERVAL: Duration = Duration::from_millis(250);

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
    #[error(transparent)]
    McpLogin(#[from] LoginError),
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
    let warning = warning.map(|w| w.to_string());
    crate::onboarding::remember_login(warning.as_deref());
    warning
}

/// One per process.
pub struct App {
    pub(crate) home: PathBuf,
    pub(crate) host: Arc<dyn Host>,
    inbox: Mutex<Inbox>,
    workspace: Workspace,
    mcp: McpAuth,
    /// Woken when a session here changes what the session list shows.
    listed: tokio::sync::Notify,
}

impl App {
    /// `home` is `COX_HOME`; `None` means `~/.cox`.
    pub fn new(home: Option<PathBuf>, host: Arc<dyn Host>) -> Result<Arc<Self>, AppError> {
        Self::with_mcp(home, host, McpAuth::default())
    }

    /// [`App::new`] with MCP tokens in `mcp`'s store and its login flow; a
    /// test passes `cox_mcp::auth::Memory` and a scripted callback (A49).
    pub fn with_mcp(
        home: Option<PathBuf>,
        host: Arc<dyn Host>,
        mcp: McpAuth,
    ) -> Result<Arc<Self>, AppError> {
        let home = home.unwrap_or_else(cox_config::load::cox_home);
        let workspace = Workspace::open(&home, Arc::new(cox_tools::git::GitWorktrees))?;
        Ok(Arc::new(Self {
            home,
            host,
            inbox: Mutex::default(),
            workspace,
            mcp,
            listed: tokio::sync::Notify::new(),
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

    /// Returns once the session list may read differently: another
    /// connection — a session here, a TUI, `cox run` — committed to
    /// `cox.db`, or a session here started, stopped or began to wait. The
    /// sidebar reads the list again after each return, instead of on a
    /// timer. A change between two calls that commits nothing is missed.
    pub async fn workspace_changed(&self) -> Result<(), AppError> {
        let store = self.workspace.store();
        let mut token = store.change_token()?;
        let listed = self.listed.notified();
        tokio::pin!(listed);
        loop {
            tokio::select! {
                () = &mut listed => return Ok(()),
                () = tokio::time::sleep(WATCH_INTERVAL) => {
                    if store.changes(&mut token)? {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Renames a session no window here has open (A113): the same user
    /// title a `Rename` intent sets, written to `cox.db` directly because
    /// no core runs it. Returns whether a title was stored; one without
    /// text is not.
    pub fn rename(&self, session: SessionId, title: &str) -> Result<bool, AppError> {
        let Some(title) = cox_core::title::user_title(title) else {
            return Ok(false);
        };
        let stored = self.workspace.store().session_title_set(
            &session,
            &title,
            cox_store::TitleSource::User,
        )?;
        // Written on the workspace's own connection, which its change
        // token does not see.
        self.listed.notify_waiters();
        Ok(stored)
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

    /// The Settings screen for a session in `cwd` (DT§5.7), with each MCP
    /// server's login read from the token store.
    pub async fn settings(&self, cwd: &Path) -> Result<SettingsView, AppError> {
        let user = self.user_config();
        let loaded = crate::settings::load(&user, cwd)?;
        let mut view = crate::settings::view_of(&loaded, &user, cwd)?;
        view.mcp = crate::mcp_login::servers(&loaded.config, cwd, &*self.mcp.secrets).await;
        Ok(view)
    }

    /// Sets `key` to `json` in this home's `config.toml`; the new view.
    pub async fn set_setting(
        &self,
        cwd: &Path,
        key: &str,
        json: &str,
    ) -> Result<SettingsView, AppError> {
        crate::settings::set(&self.user_config(), cwd, key, json)?;
        self.settings(cwd).await
    }

    /// Logs in to (`log_in`) or out of the MCP server `name` (T37.30.3); a
    /// login's page opens through [`Host::open_url`] and waits up to
    /// `cox_mcp::auth::LOGIN_TIMEOUT` for the browser to come back.
    pub async fn mcp_login(&self, cwd: &Path, name: &str, log_in: bool) -> Result<(), AppError> {
        let loaded = crate::settings::load(&self.user_config(), cwd)?;
        let open = |url: &str| self.host.open_url(url);
        crate::mcp_login::set_login(&loaded.config, cwd, name, log_in, &self.mcp, &open).await?;
        Ok(())
    }

    /// The config a session in `cwd` runs with. The Claude-settings layer
    /// is read only by `crates/cox`.
    pub(crate) fn config(&self, cwd: &Path) -> Result<cox_protocol::Config, AppError> {
        let flags = serde_json::Value::Object(serde_json::Map::new());
        Ok(cox_config::load::load_in(&self.user_config(), cwd, &flags, |_| None)?.config)
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
        let (fresh, badge, fell, moved) = {
            let mut inbox = self.lock_inbox();
            let last = inbox.items().iter().map(|i| i.seq).max();
            let before = inbox.badge();
            let was = inbox.activity(session);
            inbox.apply(session, event);
            let fresh: Vec<InboxItem> = inbox
                .items()
                .into_iter()
                .filter(|i| last.is_none_or(|last| i.seq > last))
                .cloned()
                .collect();
            let moved = inbox.activity(session) != was;
            (fresh, count(inbox.badge()), inbox.badge() < before, moved)
        };
        // A113: a title the core just stored renames the sidebar's row.
        let titled = matches!(event, Event::TitleSet { .. });
        if moved || fell || titled || !fresh.is_empty() {
            self.listed.notify_waiters();
        }
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
        self.listed.notify_waiters();
        if fell {
            self.host.badge(badge);
        }
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}
