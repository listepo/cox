//! The workspace (DT§4.3): what the sidebar lists before any session is
//! open — projects (git roots) with their sessions, full-text search over
//! every past session, and each project's worktrees with their disk size,
//! and the best-of-n groups launched here (T52.9). Read from `cox.db`
//! through `cox-store`; separate from the controller because it spans every
//! session and drives none.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cox_protocol::traits::{FileStat, Store as _, Worktree, WorktreeInfo, Worktrees};
use cox_protocol::{SessionId, StoreError, WorktreeError};
use cox_store::Store;
use cox_store::fts::SessionInfo;
use cox_store::lock::Holder;
use serde::{Deserialize, Serialize};

use crate::best_of::{BestOf, BestOfError, BestOfId, Groups};

/// How many prompts, across every session, [`Workspace::prompts`] reads:
/// as many as the TUI's `Ctrl+R` search.
const PROMPT_SCAN: i64 = 5000;

/// What a workspace query can fail with.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Worktree(#[from] WorktreeError),
    #[error(transparent)]
    BestOf(#[from] BestOfError),
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
    /// The external ACP agent that drove it (T52.6); `None` for cox.
    #[serde(default)]
    pub agent: Option<String>,
    /// The best-of-n group it was launched in (T52.9), which the sidebar
    /// shows as one group; `None` for every other session.
    #[serde(default)]
    pub best_of: Option<String>,
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
    groups: Groups,
}

impl Workspace {
    /// The workspace over `COX_HOME` = `home`.
    pub fn open(home: &Path, worktrees: Arc<dyn Worktrees>) -> Result<Self, WorkspaceError> {
        Ok(Self {
            store: Store::open(home)?,
            worktrees,
            groups: Groups::default(),
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
            let (held_by, agent) = match info.id.parse::<SessionId>() {
                Ok(id) => (
                    self.store.session_holder(&id)?,
                    self.store.session_agent(&id)?.map(|a| a.agent),
                ),
                Err(_) => (None, None),
            };
            let best_of = info
                .id
                .parse::<SessionId>()
                .ok()
                .and_then(|id| self.groups.group_of(&id))
                .map(|g| g.0);
            out.push(SessionEntry {
                info,
                held_by,
                agent,
                best_of,
            });
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

    /// `session`'s own prompts, newest first, at most `limit` (T37.24.6):
    /// what ↑ in an empty composer walks. The TUI's `Ctrl+R` query
    /// (`user_prompts`) kept to one session; that query spans every
    /// session, so this scans as far back as the TUI's search does.
    pub fn prompts(&self, session: SessionId, limit: usize) -> Result<Vec<String>, WorkspaceError> {
        let session = session.to_string();
        Ok(self
            .store
            .user_prompts(PROMPT_SCAN)?
            .into_iter()
            .filter(|p| p.session_id == session)
            .map(|p| p.text)
            .take(limit)
            .collect())
    }

    /// `project`'s checkouts with branch, lock, merged/stale state and disk
    /// size; the size walk runs off the async runtime (`worktree_list`).
    pub async fn worktrees(&self, project: &Path) -> Result<Vec<WorktreeInfo>, WorkspaceError> {
        Ok(self.worktrees.list(project).await?)
    }

    /// The worktree `name` of `project`'s repository, locked for `owner`
    /// (T52.9): made through the injected git side, as every worktree is.
    pub(crate) async fn add_worktree(
        &self,
        project: &Path,
        name: &str,
        owner: &str,
    ) -> Result<Worktree, WorkspaceError> {
        Ok(self.worktrees.add(project, name, owner).await?)
    }

    /// What the worktree at `path` changed against its base (T52.10).
    pub(crate) async fn diffstat(&self, path: &Path) -> Result<Vec<FileStat>, WorkspaceError> {
        Ok(self.worktrees.diffstat(path).await?)
    }

    /// Removes the worktree at `path` locked for `owner`; `discard` only
    /// after the person confirmed a second time (T52.10).
    pub(crate) async fn remove_worktree(
        &self,
        path: &Path,
        owner: &str,
        discard: bool,
    ) -> Result<(), WorkspaceError> {
        Ok(self.worktrees.remove(path, owner, discard).await?)
    }

    pub(crate) fn groups(&self) -> &Groups {
        &self.groups
    }

    /// The best-of-n group `id` as launched (T52.9).
    pub fn best_of(&self, id: &BestOfId) -> Result<BestOf, WorkspaceError> {
        self.groups
            .get(id)
            .ok_or_else(|| BestOfError::Unknown(id.clone()).into())
    }

    /// What group `id` cost: the sum of its candidates' own ledger rows
    /// (T52.9). An external agent's session has none; its billing is the
    /// agent's.
    pub fn best_of_cost(&self, id: &BestOfId) -> Result<f64, WorkspaceError> {
        let mut total = 0.0;
        for session in self
            .best_of(id)?
            .candidates
            .iter()
            .filter_map(|c| c.session)
        {
            total += self.session_cost(&session)?;
        }
        Ok(total)
    }

    /// One session's cost from its ledger rows.
    pub(crate) fn session_cost(&self, session: &SessionId) -> Result<f64, WorkspaceError> {
        Ok(self
            .store
            .usage_ledger(session)?
            .iter()
            .map(|row| row.usage.usage.cost_usd)
            .sum())
    }
}

/// The project a session belongs to: its cwd's git root, else the cwd.
fn project_of(cwd: &Path) -> PathBuf {
    cox_config::load::find_git_root(cwd).unwrap_or_else(|| cwd.to_path_buf())
}
