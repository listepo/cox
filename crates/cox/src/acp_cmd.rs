//! `cox acp` (T11.1): Agent Client Protocol server on stdio for Zed,
//! JetBrains and neovim. Builds live sessions through the same config,
//! provider, store and tools as every other surface; only the file/shell
//! tools are client-backed when the client offers `fs`/`terminal`.

use std::path::Path;
use std::sync::Arc;

use cox_protocol::traits::Store as _;

use crate::cli::Cli;
use crate::{config_load, session};

/// A [`cox_acp::SessionFactory`] over this machine's config and store.
struct AcpFactory {
    cli: Cli,
    answer: Option<String>,
}

impl cox_acp::SessionFactory for AcpFactory {
    fn create(&self, req: cox_acp::FactoryRequest) -> anyhow::Result<cox_core::Session> {
        let mut loaded = config_load::load(&req.cwd, &self.cli)?;
        if loaded.config.core.workspace_roots.is_empty() {
            loaded.config.core.workspace_roots = vec![req.cwd.clone()];
        }
        loaded
            .config
            .core
            .workspace_roots
            .extend(req.roots.iter().cloned());
        let config = loaded.config.clone();
        let provider = session::provider_for(&config)?;
        let home = self.cli.home.clone().unwrap_or_else(config_load::cox_home);
        let store = Arc::new(cox_store::Store::open(&home)?);
        let mdir = session::memory_dir_for(&config, &home, &req.cwd);
        let tools = session::tools(self.answer.clone(), &store, mdir);
        let tools = session::with_client_tools(tools, req.link, req.client_fs, req.client_terminal);
        let session =
            cox_core::Session::new(config, provider, tools, store.clone(), store, req.cwd)?;
        Ok(session)
    }
}

/// Serves ACP on stdio until the client goes away.
pub fn run(cli: &Cli, _cwd: &Path) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(cox_acp::serve_stdio(Arc::new(AcpFactory {
        cli: cli.clone(),
        answer: None,
    })))?;
    Ok(())
}
