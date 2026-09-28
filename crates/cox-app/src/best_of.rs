//! Best of n (T52.9, DT§3.3.1, mockup 27): one prompt sent to n candidates
//! — cox on a model, or an external ACP agent — each in a worktree of its
//! own and a session of its own, grouped under one [`BestOfId`] the sidebar
//! shows as one group. The person chose the candidates in the UI, which is
//! A75's consent to the worktrees. A candidate that fails to start is
//! recorded with why and never stops the others (fail open). Separate from
//! `app.rs` because it spans several sessions and owns none of them.

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use cox_protocol::ids::SessionId;
use cox_protocol::traits::Worktree;
use cox_protocol::types::{ModelId, Tier};
use serde::{Deserialize, Serialize};

use crate::Intent;
use crate::app::{App, AppError};
use crate::live::LiveSession;

/// One best-of-n group: the ULID of its launch, lowercased so it can name
/// the candidates' worktrees and branches.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BestOfId(pub String);

impl fmt::Display for BestOfId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Who answers the prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Candidate {
    /// Cox itself; `model` is switched to on the code tier before the
    /// prompt, `None` keeps the config's.
    Cox { model: Option<String> },
    /// An external ACP agent by its `[external_agents.<name>]` or plugin
    /// name.
    Agent { name: String },
}

/// What [`App::best_of`] launches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BestOfRequest {
    /// The repository every worktree is cut from.
    pub project: PathBuf,
    pub prompt: String,
    pub candidates: Vec<Candidate>,
}

/// One candidate as it was launched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Launched {
    pub candidate: Candidate,
    /// `None` when the worktree could not be made.
    pub worktree: Option<Worktree>,
    /// `None` when no session opened.
    pub session: Option<SessionId>,
    /// Why it did not start; the others ran anyway.
    pub failed: Option<String>,
    /// When its launch began, in milliseconds since the Unix epoch.
    pub started_ms: u64,
}

/// A launched group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BestOf {
    pub id: BestOfId,
    pub project: PathBuf,
    pub prompt: String,
    pub candidates: Vec<Launched>,
}

/// What [`App::best_of`] returns: the group, and the live sessions of the
/// candidates that started, in candidate order, for the windows to show.
pub struct Launch {
    pub group: BestOf,
    pub sessions: Vec<Arc<LiveSession>>,
}

/// What a launch can fail with before any candidate runs.
#[derive(Debug, thiserror::Error)]
pub enum BestOfError {
    #[error("best of n needs at least one candidate")]
    NoCandidates,
    #[error("no best-of-n group {0}")]
    Unknown(BestOfId),
}

/// The groups launched by this process, by id. Kept in memory: a group
/// outlives no process, its sessions and worktrees do.
#[derive(Default)]
pub(crate) struct Groups(Mutex<HashMap<BestOfId, BestOf>>);

impl Groups {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<BestOfId, BestOf>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn insert(&self, group: BestOf) {
        self.lock().insert(group.id.clone(), group);
    }

    pub(crate) fn get(&self, id: &BestOfId) -> Option<BestOf> {
        self.lock().get(id).cloned()
    }

    /// The group `session` was launched in, if any.
    pub(crate) fn group_of(&self, session: &SessionId) -> Option<BestOfId> {
        self.lock()
            .values()
            .find(|g| {
                g.candidates
                    .iter()
                    .any(|c| c.session.as_ref() == Some(session))
            })
            .map(|g| g.id.clone())
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Launches every candidate in turn: its worktree, its session, the model
/// switch for a cox candidate, then the prompt, which starts a turn and
/// returns at once.
pub(crate) async fn launch(
    app: &Arc<App>,
    request: BestOfRequest,
    theme: String,
) -> Result<Launch, AppError> {
    if request.candidates.is_empty() {
        return Err(BestOfError::NoCandidates.into());
    }
    let id = BestOfId(SessionId::new().to_string().to_ascii_lowercase());
    // A cox owner, so the worktrees can be reused and pruned by cox.
    let owner = format!("{} best-of-{id}", cox_tools::git::OWNER_PREFIX);
    let mut group = BestOf {
        id: id.clone(),
        project: request.project.clone(),
        prompt: request.prompt.clone(),
        candidates: Vec::new(),
    };
    let mut sessions = Vec::new();
    for (n, candidate) in request.candidates.into_iter().enumerate() {
        let mut launched = Launched {
            candidate,
            worktree: None,
            session: None,
            failed: None,
            started_ms: now_ms(),
        };
        let name = format!("best-{id}-{}", n + 1);
        match app
            .workspace()
            .add_worktree(&request.project, &name, &owner)
            .await
        {
            Ok(tree) => {
                let cwd = tree.path.clone();
                launched.worktree = Some(tree);
                match start(app, &launched.candidate, cwd, &request.prompt, &theme).await {
                    Ok(live) => {
                        launched.session = Some(live.id());
                        sessions.push(live);
                    }
                    Err((session, why)) => {
                        launched.session = session;
                        launched.failed = Some(why);
                    }
                }
            }
            Err(e) => launched.failed = Some(e.to_string()),
        }
        group.candidates.push(launched);
    }
    app.workspace().groups().insert(group.clone());
    Ok(Launch { group, sessions })
}

/// Opens one candidate's session in `cwd` and sends it the prompt. A
/// session that opened but refused the model or the prompt is reported
/// with its id, so the group still lists it.
async fn start(
    app: &Arc<App>,
    candidate: &Candidate,
    cwd: PathBuf,
    prompt: &str,
    theme: &str,
) -> Result<Arc<LiveSession>, (Option<SessionId>, String)> {
    let opened = match candidate {
        Candidate::Cox { .. } => app.open(cwd, None, theme.to_string()).await,
        Candidate::Agent { name } => app.open_agent(cwd, name, theme.to_string()).await,
    };
    let live = opened.map_err(|e| (None, e.to_string()))?;
    let failed = |e: AppError| (Some(live.id()), e.to_string());
    if let Candidate::Cox { model: Some(model) } = candidate {
        live.send(Intent::SwitchModel {
            tier: Tier::Code,
            model: Some(ModelId(model.clone())),
        })
        .await
        .map_err(failed)?;
    }
    live.send(Intent::Send {
        text: prompt.to_string(),
        attachments: Vec::new(),
        confirm_think: false,
    })
    .await
    .map_err(failed)?;
    Ok(live)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launched(session: Option<SessionId>) -> Launched {
        Launched {
            candidate: Candidate::Cox { model: None },
            worktree: None,
            session,
            failed: None,
            started_ms: 0,
        }
    }

    #[test]
    fn best_of_group_of_finds_the_group_a_session_was_launched_in() {
        let (one, other) = (SessionId::new(), SessionId::new());
        let groups = Groups::default();
        groups.insert(BestOf {
            id: BestOfId("g".into()),
            project: PathBuf::from("/p"),
            prompt: "x".into(),
            candidates: vec![launched(None), launched(Some(one))],
        });
        assert_eq!(groups.group_of(&one), Some(BestOfId("g".into())));
        assert_eq!(groups.group_of(&other), None);
    }
}
