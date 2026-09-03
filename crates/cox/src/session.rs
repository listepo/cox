//! Builds a live [`Session`] for the interactive surfaces: config, provider,
//! store and the built-in tool set. Kept out of `main.rs` so the TUI and
//! `cox run -p` (T6.1) assemble the same session the same way.

use std::path::Path;
use std::sync::Arc;

use cox_core::Session;
use cox_protocol::Config;
use cox_protocol::config::LocalProviderConfig;
use cox_protocol::traits::{Provider, Store as _, Tool};
use cox_provider::anthropic::{AnthropicProvider, CacheTtl};
use cox_provider::openai::chat::OpenAiChatProvider;
use cox_store::Store;
use cox_tools::ask_user::{Answers, AskUserTool};
use cox_tools::bash::BashTool;
use cox_tools::edit::EditTool;
use cox_tools::expand::ExpandTool;
use cox_tools::glob::GlobTool;
use cox_tools::grep::GrepTool;
use cox_tools::read::ReadTool;
use cox_tools::todo::TodoTool;
use cox_tools::tool_search::ToolSearchTool;
use cox_tools::v4a::ApplyPatchTool;
use cox_tools::web_fetch::WebFetchTool;
use cox_tools::write::WriteTool;
use cox_tui::state::State;

use crate::cli::Cli;
use crate::config_load::{self, LoadedConfig};

/// Loads config, picks the provider (`COX_PROVIDER` test doubles first) and
/// opens the store under `COX_HOME`. `answer` is what `ask_user` returns
/// when no one is there to ask; `tweak` lets a surface adjust the effective
/// config before the session locks it in.
pub async fn open(
    cli: &Cli,
    cwd: &Path,
    answer: Option<String>,
    tweak: impl FnOnce(&mut Config),
) -> anyhow::Result<(Session, LoadedConfig)> {
    let mut loaded = config_load::load(cwd, cli)?;
    tweak(&mut loaded.config);
    let config = loaded.config.clone();
    let provider = provider_for(&config)?;
    let home = cli.home.clone().unwrap_or_else(config_load::cox_home);
    let store = Arc::new(Store::open(&home)?);
    let mut all = tools(answer);
    if config.mcp.enabled {
        all.extend(mcp_tools(&config, cwd).await);
    }
    let session = Session::new(
        config,
        provider,
        all,
        store.clone(),
        store,
        cwd.to_path_buf(),
    )?;
    if loaded.config.hooks.enabled {
        session.set_hook(Arc::new(cox_ext::hooks::ShellHooks::new(
            &loaded.config.hooks,
            cwd.to_path_buf(),
        )));
    }
    Ok((session, loaded))
}

/// T7.6: every discovered MCP server's tools, connected on the runtime the
/// session will run on (the sessions live in the tools). A server that will
/// not start is a warning and no tools (D14).
async fn mcp_tools(config: &Config, cwd: &Path) -> Vec<Arc<dyn Tool>> {
    let project = config_load::find_git_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let home = config_load::home_dir();
    let found = cox_mcp::discovery::discover(&config.mcp.servers, Some(&project), Some(&home));
    let timeout = std::time::Duration::from_secs(u64::from(config.mcp.timeout_s));
    let (_clients, tools, notices) =
        cox_mcp::client::connect_all(&found.servers, timeout, config.mcp.deferred).await;
    for notice in found.notices.iter().chain(&notices) {
        eprintln!("cox: warning: {notice}");
    }
    tools
}

/// Runs the interactive TUI until the user quits.
pub fn run_tui(cli: &Cli, cwd: &Path) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let (session, loaded) = rt.block_on(open(cli, cwd, None, |_| {}))?;
    let config = &loaded.config;
    let mut state = State::new(config.permissions.mode, config.sandbox.mode);
    state.files = cox_tools::glob::workspace_files(cwd);
    state.composer.set_vim(config.tui.vim);
    state.dark = config.tui.theme != "light";
    state.show_thinking = config.tui.show_thinking == "full";
    state.marks = cli.verbose > 0;
    rt.block_on(cox_tui::app::run(session, state))?;
    Ok(())
}

/// The `tiers.code` provider decides which real client to build; every tier
/// of a session goes through the same provider object (routing picks models).
fn provider_for(config: &Config) -> anyhow::Result<Arc<dyn Provider>> {
    if let Some(double) = cox_provider::from_env()? {
        return Ok(Arc::from(double));
    }
    match config.tiers.code.provider.as_str() {
        "anthropic" => {
            let a = &config.providers.anthropic;
            let ttl = match a.cache_ttl.as_str() {
                "1h" => CacheTtl::OneHour,
                _ => CacheTtl::FiveMinutes,
            };
            let provider = AnthropicProvider::new(
                a.base_url.clone(),
                ttl,
                a.fallbacks,
                u64::from(a.timeout_s),
                a.max_retries,
            )?;
            Ok(Arc::new(provider))
        }
        // No OpenAI Responses client exists yet; the Chat client speaks to
        // the same endpoint family and takes the key from the configured env.
        "openai" => {
            let o = &config.providers.openai;
            let cfg = LocalProviderConfig {
                base_url: o.base_url.clone(),
                ..LocalProviderConfig::default()
            };
            Ok(Arc::new(match std::env::var(&o.api_key_env) {
                Ok(key) => OpenAiChatProvider::with_key(&cfg, key),
                Err(_) => OpenAiChatProvider::new(&cfg),
            }))
        }
        "local" => Ok(Arc::new(OpenAiChatProvider::new(&config.providers.local))),
        other => anyhow::bail!("unknown provider `{other}` in tiers.code"),
    }
}

/// Every built-in tool except `agent`, which the session adds itself.
pub(crate) fn tools(answer: Option<String>) -> Vec<Arc<dyn Tool>> {
    let mut tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(ReadTool),
        Arc::new(EditTool),
        Arc::new(WriteTool),
        Arc::new(ApplyPatchTool),
        Arc::new(BashTool),
        Arc::new(GrepTool),
        Arc::new(GlobTool),
        Arc::new(TodoTool),
        Arc::new(ExpandTool),
        Arc::new(WebFetchTool::new()),
        // ponytail: the TUI has no question surface yet; `--answer` or nothing.
        Arc::new(AskUserTool::new(Answers::Fixed(answer))),
    ];
    let specs: Vec<_> = tools.iter().map(|t| t.spec()).collect();
    tools.push(Arc::new(ToolSearchTool::new(specs)));
    tools
}
