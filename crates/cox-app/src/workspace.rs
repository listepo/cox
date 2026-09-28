//! The workspace (DT§4.3): what the sidebar lists before any session is
//! open — projects (git roots) with their sessions, full-text search over
//! every past session, and each project's worktrees with their disk size.
//! Read from `cox.db` through `cox-store`; separate from the controller
//! because it spans every session and drives none.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cox_protocol::traits::{Store as _, WorktreeInfo, Worktrees};
use cox_protocol::{SessionId, StoreError, WorktreeError};
use cox_store::Store;
use cox_store::fts::SessionInfo;
use cox_store::lock::Holder;
use serde::{Deserialize, Serialize};

/// What a workspace query can fail with.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Worktree(#[from] WorktreeError),
}

/// One sidebar project: a git root (or a bare cwd outside git). The count
/// is a `u64` so `cox-ffi` exports the type as it is (D11).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub root: PathBuf,
    /// The root's last path component.
    pub name: String,
    pub sessions: u64,
    pub cost_usd: f64,
    /// RFC 3339, the newest session's last write.
    pub updated_at: String,
}

/// One session row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionEntry {
    pub info: SessionInfo,
    /// Another process drives it (T37.34): open it read-only or fork it.
    pub held_by: Option<Holder>,
}

/// One full-text hit, with the session it belongs to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub session: SessionInfo,
    pub turn: i64,
    pub snippet: String,
}

/// One per process. `worktrees` is the git side (`cox_tools::git::GitWorktrees`
/// in the app), injected so this crate never runs a process itself.
pub struct Workspace {
    store: Store,
    worktrees: Arc<dyn Worktrees>,
}

impl Workspace {
    /// The workspace over `COX_HOME` = `home`.
    pub fn open(home: &Path, worktrees: Arc<dyn Worktrees>) -> Result<Self, WorkspaceError> {
        Ok(Self {
            store: Store::open(home)?,
            worktrees,
        })
    }

    /// The store the workspace reads; a live session reads its checkpoint
    /// rows from it too (T37.29.1).
    pub(crate) fn store(&self) -> &Store {
        &self.store
    }

    /// Projects of the `limit` most recent sessions, most recently active
    /// first, as the TUI's `/resume` groups them (`find_git_root`).
    pub fn projects(&self, limit: i64) -> Result<Vec<Project>, WorkspaceError> {
        let mut rows: Vec<Project> = Vec::new();
        for s in self.store.list_sessions(limit)? {
            let root = project_of(Path::new(&s.cwd));
            match rows.iter_mut().find(|r| r.root == root) {
                Some(row) => {
                    row.sessions += 1;
                    row.cost_usd += s.cost_usd;
                }
                None => rows.push(Project {
                    name: root.file_name().map_or_else(
                        || root.display().to_string(),
                        |n| n.to_string_lossy().into_owned(),
                    ),
                    root,
                    sessions: 1,
                    cost_usd: s.cost_usd,
                    updated_at: s.updated_at,
                }),
            }
        }
        Ok(rows)
    }

    /// `project`'s sessions among the `limit` most recent, newest first,
    /// each with the process that drives it when that is another one.
    pub fn sessions(
        &self,
        project: &Path,
        limit: i64,
    ) -> Result<Vec<SessionEntry>, WorkspaceError> {
        let mut out = Vec::new();
        for info in self.store.list_sessions(limit)? {
            if project_of(Path::new(&info.cwd)) != project {
                continue;
            }
            let held_by = match info.id.parse::<SessionId>() {
                Ok(id) => self.store.session_holder(&id)?,
                Err(_) => None,
            };
            out.push(SessionEntry { info, held_by });
        }
        Ok(out)
    }

    /// Full-text search over every session (`rollout_fts`), best first; a
    /// hit whose session row is gone is dropped.
    pub fn search(&self, query: &str, limit: i64) -> Result<Vec<SearchHit>, WorkspaceError> {
        let mut out = Vec::new();
        for hit in self.store.rollout_search(query, limit)? {
            let Ok(id) = hit.session_id.parse::<SessionId>() else {
                continue;
            };
            match self.store.session_info(&id) {
                Ok(session) => out.push(SearchHit {
                    session,
                    turn: hit.turn,
                    snippet: hit.snippet,
                }),
                Err(StoreError::NotFound) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(out)
    }

    /// `project`'s checkouts with branch, lock, merged/stale state and disk
    /// size; the size walk runs off the async runtime (`worktree_list`).
    pub async fn worktrees(&self, project: &Path) -> Result<Vec<WorktreeInfo>, WorkspaceError> {
        Ok(self.worktrees.list(project).await?)
    }
}

/// The project a session belongs to: its cwd's git root, else the cwd.
fn project_of(cwd: &Path) -> PathBuf {
    cox_config::load::find_git_root(cwd).unwrap_or_else(|| cwd.to_path_buf())
}
