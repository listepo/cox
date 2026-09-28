//! `cox-ffi` (DT§4.4, §4.5): the macOS app's surface — the fifth, beside the
//! TUI, `run -p`, ACP and `cox mcp` (D11). UniFFI exports over `cox-app`
//! and `cox-session`, linked into the app as a static library; the one
//! tokio runtime every session runs on; the Swift-implemented [`AppHost`].
//! Thin on purpose: the view model is `cox-app`'s and the session build is
//! `cox-session`'s. Alone in depending on `uniffi` (`crates/cox/tests/deps.rs`).

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Duration;

use cox_app::{Activity, Inbox, InboxItem, IntentError, SearchHit, SessionEntry};
use cox_app::{Workspace, WorkspaceError};
use cox_protocol::StoreError;
use cox_protocol::errors::CoreError;
use cox_protocol::ids::SessionId;
use cox_protocol::traits::WorktreeInfo;
use cox_protocol::types::Event;
use cox_session::SessionError;
use cox_store::lock::Holder;
use tokio::runtime::Runtime;
use tokio_util::task::AbortOnDropHandle;

pub mod host;
pub mod session;
pub mod types;

pub use host::AppHost;
pub use session::SessionHandle;

uniffi::setup_scaffolding!();

/// DT§4.8: how long the login shell may take before its env is skipped.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(10);

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

impl From<SessionError> for AppError {
    fn from(e: SessionError) -> Self {
        match e {
            SessionError::SessionBusy { id, holder } => Self::Busy { id, holder },
            e => Self::Session {
                message: e.to_string(),
            },
        }
    }
}

macro_rules! message_from {
    ($($from:ty => $variant:ident),*) => {$(
        impl From<$from> for AppError {
            fn from(e: $from) -> Self {
                Self::$variant { message: e.to_string() }
            }
        }
    )*};
}
message_from!(CoreError => Session, StoreError => Session, IntentError => Intent,
    WorkspaceError => Workspace);

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
    let (env, warning) = on_runtime(cox_session::env::login_env(LOGIN_TIMEOUT)).await?;
    for (key, value) in env {
        // SAFETY: the one hazard is another thread reading the environment
        // meanwhile. This runs once at launch, before any session exists,
        // so cox's own threads are idle; Rust's env reads share std's lock.
        unsafe { std::env::set_var(key, value) };
    }
    Ok(warning.map(|w| w.to_string()))
}

/// What the app and its sessions share.
pub(crate) struct Shared {
    pub home: PathBuf,
    pub host: Arc<dyn AppHost>,
    inbox: Mutex<Inbox>,
}

impl Shared {
    fn inbox(&self) -> MutexGuard<'_, Inbox> {
        self.inbox.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Folds `event` into the inbox and tells the host about each new item,
    /// outside the lock so Swift may read the inbox back.
    pub fn apply(&self, session: SessionId, event: &Event) {
        let (fresh, badge) = {
            let mut inbox = self.inbox();
            let last = inbox.items().iter().map(|i| i.seq).max();
            inbox.apply(session, event);
            let fresh: Vec<InboxItem> = inbox
                .items()
                .into_iter()
                .filter(|i| last.is_none_or(|last| i.seq > last))
                .cloned()
                .collect();
            (fresh, count(inbox.badge()))
        };
        for item in fresh {
            self.host.notify(item, badge);
        }
    }

    pub fn expire(&self, session: SessionId) {
        self.inbox().expire(session);
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
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
    shared: Arc<Shared>,
    workspace: Arc<Workspace>,
}

#[uniffi::export]
impl App {
    /// `home` is `COX_HOME`; `None` means `~/.cox`.
    #[uniffi::constructor]
    pub fn new(home: Option<String>, host: Arc<dyn AppHost>) -> Result<Arc<Self>, AppError> {
        let home = home.map_or_else(cox_config::load::cox_home, PathBuf::from);
        let workspace = Workspace::open(&home, Arc::new(cox_tools::git::GitWorktrees))?;
        let shared = Shared {
            home,
            host,
            inbox: Mutex::default(),
        };
        Ok(Arc::new(Self {
            shared: Arc::new(shared),
            workspace: Arc::new(workspace),
        }))
    }

    pub fn projects(&self, limit: u32) -> Result<Vec<Project>, AppError> {
        let rows = self.workspace.projects(i64::from(limit))?;
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
        Ok(self.workspace.sessions(&project, i64::from(limit))?)
    }

    pub fn search(&self, query: String, limit: u32) -> Result<Vec<SearchHit>, AppError> {
        Ok(self.workspace.search(&query, i64::from(limit))?)
    }

    pub async fn worktrees(&self, project: String) -> Result<Vec<WorktreeInfo>, AppError> {
        let workspace = Arc::clone(&self.workspace);
        let listed = on_runtime(async move { workspace.worktrees(&PathBuf::from(project)).await });
        Ok(listed.await??)
    }

    /// Most urgent first, oldest first within a rank.
    pub fn inbox(&self) -> Vec<InboxItem> {
        self.shared.inbox().items().into_iter().cloned().collect()
    }

    /// The Dock badge.
    pub fn badge(&self) -> u32 {
        count(self.shared.inbox().badge())
    }

    pub fn activity(&self, session: SessionId) -> Activity {
        self.shared.inbox().activity(session)
    }

    pub fn dismiss(&self, session: SessionId, seq: u64) {
        self.shared.inbox().dismiss(session, seq);
    }

    pub async fn open(&self, request: OpenRequest) -> Result<Arc<SessionHandle>, AppError> {
        let shared = Arc::clone(&self.shared);
        on_runtime(async move {
            let resume = match request.resume {
                Some(id) => Some((id, cox_session::resume(&shared.home, id)?)),
                None => None,
            };
            session::open(shared, PathBuf::from(request.cwd), resume, request.theme).await
        })
        .await?
    }
}
