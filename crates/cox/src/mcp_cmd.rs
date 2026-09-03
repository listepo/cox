//! `cox mcp`: picks which built-in tools to serve and plugs the permission
//! engine in as the gate (T6.2). The wire side lives in `cox_mcp::server`;
//! this is the only place the flags, the engine and the store meet.

use std::path::Path;
use std::sync::Arc;

use cox_core::permission::{Engine, Outcome, why_text};
use cox_mcp::server::{CxTemplate, Gate, ToolServer};
use cox_protocol::ids::SessionId;
use cox_protocol::traits::Store as _;
use cox_protocol::types::{ApprovalPolicy, PermissionMode, SandboxMode, SandboxPolicy, ToolCall};
use cox_store::Store;

use crate::cli::{Cli, McpArgs};
use crate::{config_load, session};

const READ_ONLY: &[&str] = &["read", "grep", "glob", "outline"];
const WRITE: &[&str] = &["edit", "write", "apply_patch"];

/// `policy = never`: an ask becomes a deny, since no one is there to answer.
struct EngineGate {
    engine: Engine,
    mode: PermissionMode,
    sandbox: SandboxMode,
}

impl Gate for EngineGate {
    fn check(&self, call: &ToolCall) -> Result<(), String> {
        match self
            .engine
            .decide(call, self.mode, ApprovalPolicy::Never, self.sandbox, &[])
        {
            Outcome::Allow { .. } => Ok(()),
            Outcome::Deny { reason, .. } => Err(reason),
            Outcome::Ask(why) => Err(why_text(&why)),
        }
    }
}

/// The tool names the flags select, in the order the plan lists them.
fn selected(args: &McpArgs) -> Vec<String> {
    match &args.tools {
        Some(list) => list
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
        None => READ_ONLY
            .iter()
            .chain(args.allow_write.then_some(WRITE).into_iter().flatten())
            .map(|s| (*s).to_string())
            .collect(),
    }
}

pub fn run(cli: &Cli, args: &McpArgs, cwd: &Path) -> anyhow::Result<()> {
    let config = config_load::load(cwd, cli)?.config;
    let home = cli.home.clone().unwrap_or_else(config_load::cox_home);
    let store = Arc::new(Store::open(&home)?);
    let names = selected(args);
    let mdir = session::memory_dir_for(&config, &home, cwd);
    let tools = session::tools(None, &store, mdir)
        .into_iter()
        .filter(|t| names.contains(&t.spec().name))
        .collect();
    let gate = EngineGate {
        engine: Engine::compile(&config.permissions, Some(&home), cwd)?,
        mode: config.permissions.mode,
        sandbox: config.sandbox.mode,
    };
    let mut roots = config.core.workspace_roots.clone();
    if roots.is_empty() {
        roots.push(cwd.to_path_buf());
    }
    let cx = CxTemplate {
        roots,
        cwd: cwd.to_path_buf(),
        sandbox: SandboxPolicy {
            mode: config.sandbox.mode,
            network: config.sandbox.network,
            writable: config.sandbox.writable.clone(),
            readonly_in_workspace: config.sandbox.readonly_in_workspace.clone(),
            linux_backend: config.sandbox.linux_backend,
        },
        archive: store,
        session: SessionId::new(),
    };
    let server = ToolServer::new(tools, Arc::new(gate), cx);
    tokio::runtime::Runtime::new()?.block_on(server.serve_stdio())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_selection_is_read_only_and_write_is_opt_in() {
        assert_eq!(
            selected(&McpArgs::default()),
            ["read", "grep", "glob", "outline"]
        );
        let with_write = selected(&McpArgs {
            allow_write: true,
            tools: None,
        });
        assert_eq!(with_write.len(), 7);
        assert!(with_write.contains(&"apply_patch".to_string()));
        assert!(!with_write.contains(&"bash".to_string()));
        let explicit = selected(&McpArgs {
            allow_write: false,
            tools: Some("bash, read".into()),
        });
        assert_eq!(explicit, ["bash", "read"]);
    }
}
