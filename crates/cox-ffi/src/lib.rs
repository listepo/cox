//! `cox-ffi` (DT§4.4, §4.5): the macOS app's surface — the fifth, beside the
//! TUI, `run -p`, ACP and `cox mcp` (D11). UniFFI exports over `cox-app`,
//! linked into the app as a static library; the one tokio runtime every
//! session runs on; the Swift-implemented [`AppHost`]. A forwarder only:
//! sessions are `cox-app`'s (T37.39), so its sole workspace dependencies are
//! `cox-app` and `cox-protocol`, and it alone uses `uniffi` (`deps.rs`).

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use cox_app::app::{App as Owner, AppError as OwnerError};
use cox_app::{Activity, Holder, InboxItem, SearchHit, SessionEntry, WorkspaceError};
use cox_protocol::ids::SessionId;
use cox_protocol::traits::WorktreeInfo;
use tokio::runtime::Runtime;
use tokio_util::task::AbortOnDropHandle;

pub mod host;
pub mod session;
pub mod types;

pub use host::AppHost;
pub use session::SessionHandle;

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
    Runtime { message: String },
}

impl From<OwnerError> for AppError {
    fn from(e: OwnerError) -> Self {
        let message = e.to_string();
        match e {
            OwnerError::Busy { id, holder } => Self::Busy { id, holder },
            OwnerError::Intent(_) => Self::Intent { message },
            OwnerError::Workspace(_) => Self::Workspace { message },
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
fn runtime() -> Result<&'static Runtime, AppError> {
    static RUNTIME: OnceLock<std::io::Result<Runtime>> = OnceLock::new();
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

/// One sidebar project (`cox_app::ProjectRow`, its count as `u64`).
#[derive(uniffi::Record)]
pub struct Project {
    pub root: PathBuf,
    pub name: String,
    pub sessions: u64,
    pub cost_usd: f64,
    pub updated_at: String,
}

/// What [`App::open`] opens: a new session in `cwd`, or `resume`'s.
/// `theme` is the syntect theme code blocks are highlighted with.
#[derive(uniffi::Record)]
pub struct OpenRequest {
    pub cwd: String,
    pub resume: Option<SessionId>,
    pub theme: String,
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
        let owner = Owner::new(home.map(PathBuf::from), Arc::new(host::Bridge(host)))?;
        Ok(Arc::new(Self { owner }))
    }

    pub fn projects(&self, limit: u32) -> Result<Vec<Project>, AppError> {
        let rows = self.owner.workspace().projects(i64::from(limit))?;
        Ok(rows
            .into_iter()
            .map(|row| Project {
                root: row.root,
                name: row.name,
                sessions: u64::try_from(row.sessions).unwrap_or(u64::MAX),
                cost_usd: row.cost_usd,
                updated_at: row.updated_at,
            })
            .collect())
    }

    pub fn sessions(&self, project: String, limit: u32) -> Result<Vec<SessionEntry>, AppError> {
        let project = PathBuf::from(project);
        Ok(self
            .owner
            .workspace()
            .sessions(&project, i64::from(limit))?)
    }

    pub fn search(&self, query: String, limit: u32) -> Result<Vec<SearchHit>, AppError> {
        Ok(self.owner.workspace().search(&query, i64::from(limit))?)
    }

    pub async fn worktrees(&self, project: String) -> Result<Vec<WorktreeInfo>, AppError> {
        let owner = Arc::clone(&self.owner);
        let project = PathBuf::from(project);
        Ok(on_runtime(async move { owner.workspace().worktrees(&project).await }).await??)
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

    pub async fn open(&self, request: OpenRequest) -> Result<Arc<SessionHandle>, AppError> {
        let owner = Arc::clone(&self.owner);
        let OpenRequest { cwd, resume, theme } = request;
        let live = on_runtime(async move { owner.open(cwd.into(), resume, theme).await });
        Ok(SessionHandle::new(live.await??))
    }
}
