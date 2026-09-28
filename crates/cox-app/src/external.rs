//! A top-level session driven by an external ACP agent (T52.4, DT§3.3.1):
//! which agent a name means, how it is started, and what each intent does
//! to it. The process, the ACP connection and the event fold are
//! `cox_session::acp_session`'s; `live.rs` runs the result like any other
//! session, so the timeline, inbox and controller do not know the
//! difference (DT-7). The agent's permission asks reach the inbox the
//! same way, and an `Approve` intent answers them (T52.5).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cox_protocol::Config;
use cox_protocol::errors::{CoreError, ProviderError};
use cox_protocol::ids::SessionId;
use cox_session::acp_session::{self, AcpOpenError, AcpSession, OpenedAcp};

use crate::app::{App, AppError};
use crate::intent::{AgentDispatch, Intent, agent_dispatch};

/// Starts the agent `name` in `cwd`; the session and the roots it may
/// write. A name no entry resolves to is one warning, with the reason its
/// entry was refused when there is one.
pub(crate) async fn open(
    app: &App,
    config: &Config,
    cwd: &Path,
    name: &str,
) -> Result<(OpenedAcp, Vec<PathBuf>), AppError> {
    let roots = writable_roots(config, cwd);
    let (agents, warnings) = acp_session::agents(config, &app.home, cwd, &roots);
    let Some(agent) = agents.into_iter().find(|a| a.name() == name) else {
        let why: Vec<String> = warnings.into_iter().filter(|w| w.contains(name)).collect();
        let why = match why.is_empty() {
            true => format!("no external agent is named {name}"),
            false => why.join("; "),
        };
        return Err(AcpOpenError::Unavailable(why).into());
    };
    let host = Arc::clone(&app.host);
    // The entry's own variable wins, as for a provider key; then the key
    // the app stored under the agent's name. Never cox's provider keys.
    let key = move |env: &str, section: &str| {
        std::env::var(env)
            .ok()
            .filter(|v| !v.is_empty())
            .or_else(|| host.secret(section))
            .ok_or(ProviderError::Auth)
    };
    let path = std::env::var_os("PATH");
    let opened = acp_session::open(agent, config, cwd, &roots, path.as_deref(), key).await?;
    Ok((opened, roots))
}

/// Runs `intent` against the agent's session.
pub(crate) fn send(
    app: &App,
    id: SessionId,
    acp: &AcpSession,
    intent: Intent,
) -> Result<(), AppError> {
    let delivered = match agent_dispatch(intent)? {
        AgentDispatch::Prompt(text) => acp.prompt(text),
        AgentDispatch::Cancel => acp.cancel(),
        // A stale answer (the ask timed out, or was answered from another
        // window) finds nothing waiting; the inbox already shows it decided.
        AgentDispatch::Approve { call, decision } => {
            acp.approve(call, decision);
            true
        }
        AgentDispatch::Rename(title) => return app.rename(id, &title).map(|_| ()),
        AgentDispatch::Refused(intent) => {
            return Err(AppError::Unsupported {
                agent: acp.agent().to_string(),
                intent,
            });
        }
    };
    match delivered {
        true => Ok(()),
        false => Err(AppError::Core(CoreError::ExternalAgent {
            agent: acp.agent().to_string(),
            message: String::from("the agent has exited"),
        })),
    }
}

/// §1.6, as `cox-session` resolves it for a cox session: the configured
/// workspace roots, else the git root of `cwd`, else `cwd`.
fn writable_roots(config: &Config, cwd: &Path) -> Vec<PathBuf> {
    match config.core.workspace_roots.is_empty() {
        true => vec![cox_config::load::find_git_root(cwd).unwrap_or_else(|| cwd.to_path_buf())],
        false => config.core.workspace_roots.clone(),
    }
}
