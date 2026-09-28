//! `cox-ffi` (DT§4.4, §4.5): the macOS app's surface — the fifth, beside the
//! TUI, `run -p`, ACP and `cox mcp` (D11). UniFFI exports over `cox-app`,
//! linked into the app as a static library; the one tokio runtime every
//! session runs on; the Swift-implemented [`AppHost`]. A forwarder only:
//! sessions are `cox-app`'s (T37.39), so its sole workspace dependencies are
//! `cox-app` and `cox-protocol`, and it alone uses `uniffi` (`deps.rs`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use cox_app::app::{App as Owner, AppError as OwnerError};
use cox_app::{Activity, Holder, InboxItem, SearchHit, SessionEntry, SettingsView, WorkspaceError};
use cox_protocol::ids::SessionId;
use cox_protocol::traits::WorktreeInfo;
use tokio::runtime::Runtime;
use tokio_util::task::AbortOnDropHandle;

pub mod host;
pub mod session;
pub mod types;

pub use host::AppHost;
pub use session::SessionHandle;
pub use types::{OpenRequest, Project};

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
            OwnerError::Settings(_) => Self::Settings { message },
            _ => Self::Session { message },
        }
    }
}

impl From<WorkspaceError> for AppError {
    fn from(e: WorkspaceError) -> Self {
        OwnerError::from(e).into()
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
        Ok(self
            .owner
            .workspace()
            .projects(i64::from(limit))?
            .into_iter()
            .map(Project::from)
            .collect())
    }

    pub fn sessions(&self, project: String, limit: u32) -> Result<Vec<SessionEntry>, AppError> {
        Ok(self
            .owner
            .workspace()
            .sessions(Path::new(&project), i64::from(limit))?)
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
    pub fn settings(&self, cwd: String) -> Result<SettingsView, AppError> {
        Ok(self.owner.settings(Path::new(&cwd))?)
    }

    /// Sets `key` to the JSON `value` in the user file; the new view.
    pub fn set_setting(
        &self,
        cwd: String,
        key: String,
        value: String,
    ) -> Result<SettingsView, AppError> {
        Ok(self.owner.set_setting(Path::new(&cwd), &key, &value)?)
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
