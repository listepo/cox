//! `cox-ffi` (DT§4.4, §4.5): the macOS app's surface — the fifth, beside the
//! TUI, `run -p`, ACP and `cox mcp` (D11). UniFFI exports over `cox-app`,
//! linked into the app as a static library; the one tokio runtime every
//! session runs on; the Swift-implemented [`AppHost`]. A forwarder only:
//! sessions are `cox-app`'s (T37.39), so its sole workspace dependencies are
//! `cox-app` and `cox-protocol`, and it alone uses `uniffi` (`deps.rs`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use cox_app::WorkspaceError;
use cox_app::app::{App as Owner, AppError as OwnerError};
use cox_app::onboarding::CheckRow;
use cox_app::remote::RemoteError;
use cox_app::terminal::TerminalError;
use cox_app::{
    Activity, DaySummary, Holder, InboxItem, ModelChoice, Project, SearchHit, SessionEntry,
};
use cox_app::{RuleKind, SessionGrant, SettingsView};
use cox_protocol::ids::SessionId;
use cox_protocol::traits::WorktreeInfo;
use tokio::runtime::Runtime;
use tokio_util::task::AbortOnDropHandle;

pub mod host;
pub mod remote;
pub mod session;
pub mod types;

pub use host::AppHost;
pub use remote::{RemoteHandle, RemoteSessionHandle};
pub use session::{SessionHandle, TerminalHandle};
pub use types::{BrowserFailure, OpenRequest};

uniffi::setup_scaffolding!();

/// Every failure the app sees, from the crates' own `thiserror` enums.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum AppError {
    /// T37.34: another process drives it; follow it read-only or fork it.
    #[error("session {id} is open in {holder}")]
    Busy { id: SessionId, holder: Holder },
    #[error("{message}")]
    Session { message: String },
    #[error("{message}")]
    Intent { message: String },
    #[error("{message}")]
    Workspace { message: String },
    #[error("{message}")]
    Settings { message: String },
    #[error("{message}")]
    Runtime { message: String },
}

impl From<OwnerError> for AppError {
    fn from(e: OwnerError) -> Self {
        let message = e.to_string();
        match e {
            OwnerError::Busy { id, holder } => Self::Busy { id, holder },
            OwnerError::Intent(_) => Self::Intent { message },
            OwnerError::Workspace(_) => Self::Workspace { message },
            OwnerError::Settings(_) | OwnerError::McpLogin(_) => Self::Settings { message },
            _ => Self::Session { message },
        }
    }
}

impl From<WorkspaceError> for AppError {
    fn from(e: WorkspaceError) -> Self {
        OwnerError::from(e).into()
    }
}

impl From<TerminalError> for AppError {
    fn from(e: TerminalError) -> Self {
        OwnerError::from(e).into()
    }
}

/// T52.20: a remote failure is shown as the session's own would be.
impl From<RemoteError> for AppError {
    fn from(e: RemoteError) -> Self {
        Self::Session {
            message: e.to_string(),
        }
    }
}

/// The one runtime (DT§4.5), made on first use; nothing blocks on it.
static RUNTIME: OnceLock<std::io::Result<Runtime>> = OnceLock::new();

fn runtime() -> Result<&'static Runtime, AppError> {
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("cox")
                .build()
        })
        .as_ref()
        .map_err(|e| AppError::Runtime {
            message: e.to_string(),
        })
}

/// Runs `task` on the runtime for whichever executor polls the result —
/// Swift's, through UniFFI. Dropping the wait (a cancelled Swift `Task`)
/// aborts the task.
async fn on_runtime<T: Send + 'static>(
    task: impl Future<Output = T> + Send + 'static,
) -> Result<T, AppError> {
    AbortOnDropHandle::new(runtime()?.spawn(task))
        .await
        .map_err(|e| AppError::Runtime {
            message: e.to_string(),
        })
}

/// DT§4.8: reads the login shell's environment into this process, so tools
/// find `cargo` and `mise` and env-var keys resolve as in a terminal.
/// Call once at launch, before [`App::new`]. Returns why it fell back to
/// the inherited environment, if it did.
#[uniffi::export]
pub async fn load_login_env() -> Result<Option<String>, AppError> {
    on_runtime(cox_app::app::load_login_env()).await
}

/// Review's line comments as the one prompt "Send to agent" posts
/// (T37.28.4); `None` when no comment has text.
#[uniffi::export]
pub fn review_message(comments: Vec<cox_app::review::LineComment>) -> Option<String> {
    cox_app::review::message(&comments)
}

/// T51.10: the browser pane's typed address as the URL to load, or `None`
/// when it is not an `http`/`https` page; `browser_open`'s rule.
#[uniffi::export]
pub fn web_address(text: String) -> Option<String> {
    cox_app::browser::web_address(&text)
}

/// One per process: the workspace, the inbox across sessions, the host.
#[derive(uniffi::Object)]
pub struct App {
    owner: Arc<Owner>,
}

#[uniffi::export]
impl App {
    /// `home` is `COX_HOME`; `None` means `~/.cox`.
    #[uniffi::constructor]
    pub fn new(home: Option<String>, host: Arc<dyn AppHost>) -> Result<Arc<Self>, AppError> {
        Ok(Arc::new(Self {
            owner: Owner::new(home.map(PathBuf::from), Arc::new(host::Bridge(host)))?,
        }))
    }

    pub fn projects(&self, limit: u32) -> Result<Vec<Project>, AppError> {
        Ok(self.owner.workspace().projects(i64::from(limit))?)
    }

    pub fn sessions(&self, project: String, limit: u32) -> Result<Vec<SessionEntry>, AppError> {
        Ok(self
            .owner
            .workspace()
            .sessions(Path::new(&project), i64::from(limit))?)
    }

    /// The menu bar's "Today" footer (T51.12).
    pub fn today(&self) -> Result<DaySummary, AppError> {
        Ok(self.owner.workspace().today()?)
    }

    pub fn search(&self, query: String, limit: u32) -> Result<Vec<SearchHit>, AppError> {
        Ok(self.owner.workspace().search(&query, i64::from(limit))?)
    }

    pub async fn worktrees(
        self: Arc<Self>,
        project: String,
    ) -> Result<Vec<WorktreeInfo>, AppError> {
        Ok(
            on_runtime(async move { self.owner.workspace().worktrees(Path::new(&project)).await })
                .await??,
        )
    }

    /// Most urgent first, oldest first within a rank.
    pub fn inbox(&self) -> Vec<InboxItem> {
        self.owner.inbox()
    }

    /// The Dock badge.
    pub fn badge(&self) -> u32 {
        self.owner.badge()
    }

    pub fn activity(&self, session: SessionId) -> Activity {
        self.owner.activity(session)
    }

    pub fn dismiss(&self, session: SessionId, seq: u64) {
        self.owner.dismiss(session, seq);
    }

    /// The Settings screen for a session in `cwd` (DT§5.7).
    pub async fn settings(self: Arc<Self>, cwd: String) -> Result<SettingsView, AppError> {
        Ok(on_runtime(async move { self.owner.settings(Path::new(&cwd)).await }).await??)
    }

    /// Sets `key` to the JSON `value` in the user file; the new view.
    pub async fn set_setting(
        self: Arc<Self>,
        cwd: String,
        key: String,
        value: String,
    ) -> Result<SettingsView, AppError> {
        Ok(
            on_runtime(async move { self.owner.set_setting(Path::new(&cwd), &key, &value).await })
                .await??,
        )
    }

    /// Adds (`old` none), replaces or removes (`new` none) one permission
    /// rule in the user file (T37.45.3); the new view.
    pub async fn set_permission_rule(
        self: Arc<Self>,
        cwd: String,
        kind: RuleKind,
        old: Option<String>,
        new: Option<String>,
    ) -> Result<SettingsView, AppError> {
        Ok(on_runtime(async move {
            (self.owner)
                .set_rule(Path::new(&cwd), kind, old.as_deref(), new.as_deref())
                .await
        })
        .await??)
    }

    /// Revokes a session grant through its session's core (T37.45.3).
    pub async fn revoke_grant(
        self: Arc<Self>,
        cwd: String,
        grant: SessionGrant,
    ) -> Result<SettingsView, AppError> {
        Ok(
            on_runtime(async move { self.owner.revoke_grant(Path::new(&cwd), &grant).await })
                .await??,
        )
    }

    /// Logs in to (`login`) or out of the MCP server `name` (T37.30.3).
    pub async fn mcp_login(
        self: Arc<Self>,
        cwd: String,
        name: String,
        login: bool,
    ) -> Result<(), AppError> {
        Ok(
            on_runtime(async move { self.owner.mcp_login(Path::new(&cwd), &name, login).await })
                .await??,
        )
    }

    /// Renames a session no window here has open (A113); `false` when the
    /// title has no text.
    pub fn rename(&self, session: SessionId, title: String) -> Result<bool, AppError> {
        Ok(self.owner.rename(session, &title)?)
    }

    /// The toolbar's model popover for a session in `cwd` (T37.22.6).
    pub fn models(&self, cwd: String) -> Result<Vec<ModelChoice>, AppError> {
        Ok(self.owner.models(Path::new(&cwd))?)
    }

    /// The providers a turn in `cwd` could run on now (A110); it probes
    /// local servers, so it runs off the caller's thread.
    pub async fn usable_providers(self: Arc<Self>, cwd: String) -> Result<Vec<String>, AppError> {
        Ok(on_runtime(async move { self.owner.usable_providers(Path::new(&cwd)).await }).await??)
    }

    /// Returns once the session list may read differently (T37.22.6): the
    /// sidebar reads it again then, not on a timer.
    pub async fn workspace_changed(self: Arc<Self>) -> Result<(), AppError> {
        Ok(on_runtime(async move { self.owner.workspace_changed().await }).await??)
    }

    /// The first-run checklist for a session in `cwd` (DT§5.8, T37.31); it
    /// runs `git` and probes the sandbox, so it runs off the caller's thread.
    pub async fn checklist(self: Arc<Self>, cwd: String) -> Result<Vec<CheckRow>, AppError> {
        Ok(on_runtime(async move { self.owner.checklist(Path::new(&cwd)) }).await??)
    }

    /// A remote host's workspace over the person's own ssh (T52.20).
    pub async fn connect_remote(
        self: Arc<Self>,
        host: String,
    ) -> Result<Arc<RemoteHandle>, AppError> {
        Ok(RemoteHandle::new(
            on_runtime(async move { self.owner.connect_remote(&host).await }).await??,
        ))
    }

    pub async fn open(
        self: Arc<Self>,
        request: OpenRequest,
    ) -> Result<Arc<SessionHandle>, AppError> {
        Ok(SessionHandle::new(
            on_runtime(async move {
                self.owner
                    .open(request.cwd.into(), request.resume, request.theme)
                    .await
            })
            .await??,
        ))
    }
}
