//! Builds a live [`Session`] for the interactive surfaces: config, provider,
//! store and the built-in tool set. Kept out of `main.rs` so the TUI and
//! `cox run -p` (T6.1) assemble the same session the same way.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cox_core::Session;
use cox_protocol::Config;
use cox_protocol::traits::{Hook, Provider, Store as _, Tool};
use cox_protocol::types::Submission;
use cox_provider::anthropic::{AnthropicProvider, CacheTtl};
use cox_provider::openai::chat::OpenAiChatProvider;
use cox_provider::openai::responses::OpenAiResponsesProvider;
use cox_store::Store;
use cox_tools::ask_user::{Answers, AskUserTool};
use cox_tools::bash::BashTool;
use cox_tools::edit::EditTool;
use cox_tools::expand::ExpandTool;
use cox_tools::glob::GlobTool;
use cox_tools::grep::GrepTool;
use cox_tools::memory::{MemorySaveTool, MemorySearchTool};
use cox_tools::read::ReadTool;
use cox_tools::todo::TodoTool;
use cox_tools::tool_search::ToolSearchTool;
use cox_tools::v4a::ApplyPatchTool;
use cox_tools::web_fetch::WebFetchTool;
use cox_tools::write::WriteTool;
use cox_tui::state::{Msg, State};

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
    // §1.6: empty `workspace_roots` means the git root of cwd, else cwd.
    if loaded.config.core.workspace_roots.is_empty() {
        loaded.config.core.workspace_roots =
            vec![config_load::find_git_root(cwd).unwrap_or_else(|| cwd.to_path_buf())];
    }
    // T9.1 step 4 (generalised): a non-first-party `tiers.code.provider`
    // maps every tier to the same server; the router then pins each tier to
    // that provider's section model, so a `--provider deepseek` flip works
    // without editing every tier model.
    if !["anthropic", "openai"].contains(&loaded.config.tiers.code.provider.as_str()) {
        for tier in [
            &mut loaded.config.tiers.cheap,
            &mut loaded.config.tiers.think,
        ] {
            tier.provider = loaded.config.tiers.code.provider.clone();
        }
    }
    let config = loaded.config.clone();
    let provider = provider_for(&config)?;
    let home = cli.home.clone().unwrap_or_else(config_load::cox_home);
    let store = Arc::new(Store::open(&home)?);
    let mdir = memory_dir_for(&loaded.config, &home, cwd);
    let mut all = tools(answer, &store, mdir);
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
    // A14: the presence hook wraps the user's shell hooks so the other
    // sessions of this workspace see every surface, `--no-hooks` or not.
    let shell: Option<Arc<dyn Hook>> = loaded.config.hooks.enabled.then(|| {
        Arc::new(cox_ext::hooks::ShellHooks::new(
            &loaded.config.hooks,
            cwd.to_path_buf(),
        )) as Arc<dyn Hook>
    });
    session.set_hook(Arc::new(cox_ext::presence::PresenceHook::new(
        home.clone(),
        session.id(),
        cwd.to_path_buf(),
        config_load::find_git_root(cwd).unwrap_or_else(|| cwd.to_path_buf()),
        shell,
    )));
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

/// `/sessions` and `/resume` rows: this project's sessions, newest first.
/// A store that will not open is an empty list, not a failed start.
fn project_sessions(home: &Path, cwd: &Path) -> Vec<(String, String)> {
    let project = config_load::find_git_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let now = crate::sessions::now_secs();
    Store::open(home)
        .and_then(|store| store.list_sessions(200))
        .unwrap_or_default()
        .into_iter()
        .filter(|info| Path::new(&info.cwd).starts_with(&project))
        .map(|info| {
            let row = cox_tui::picker::session_entry(
                info.title.as_deref(),
                &info.cwd,
                &crate::sessions::age_of(&info.updated_at, now),
                info.cost_usd,
            );
            (info.id, row)
        })
        .collect()
}

/// Runs the interactive TUI until the user quits.
pub fn run_tui(cli: &Cli, cwd: &Path) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let (session, loaded) = rt.block_on(open(cli, cwd, None, |_| {}))?;
    let config = &loaded.config;
    let mut state = State::new(config.permissions.mode, config.sandbox.mode);
    state.files = cox_tools::glob::workspace_files(cwd);
    let home = cli.home.clone().unwrap_or_else(config_load::cox_home);
    state.sessions = project_sessions(&home, cwd);
    state.composer.set_vim(config.tui.vim);
    state.dark = config.tui.theme != "light";
    state.glyphs = cox_tui::glyph::resolve(&config.tui);
    state.depth = cox_tui::color::resolve(&config.tui);
    // The theme name outlives every render; one leak per process buys a
    // `Copy` `Look` instead of a clone on each line.
    state.syntax_theme = String::leak(config.tui.syntax_theme.clone());
    if !state.syntax_theme.is_empty()
        && cox_tui::markdown::theme_name(state.dark, state.syntax_theme) != state.syntax_theme
    {
        state.transcript.push(cox_tui::state::Cell::Notice {
            level: cox_protocol::types::Level::Warn,
            text: format!(
                "unknown tui.syntax_theme {:?}; using the default. Available: {}",
                config.tui.syntax_theme,
                cox_tui::markdown::themes().join(", ")
            ),
        });
    }
    state.show_thinking = config.tui.show_thinking == "full";
    state.marks = cli.verbose > 0;
    let (feed, feed_rx) = tokio::sync::mpsc::channel(4);
    // The poller lives here, not in cox-tui: the TUI never touches the disk.
    let poll = {
        let home = home.clone();
        let project = config_load::find_git_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
        let me = session.id();
        rt.spawn(async move {
            let mut every = tokio::time::interval(std::time::Duration::from_secs(2));
            loop {
                every.tick().await;
                let now = cox_ext::presence::now_secs();
                let agents = cox_ext::presence::others(&home, &project, &me, now);
                if feed.send(Msg::Agents(agents)).await.is_err() {
                    break;
                }
            }
        })
    };
    let quit = session.clone();
    let ran = rt.block_on(cox_tui::app::run(session, state, feed_rx));
    poll.abort();
    ran?;
    // The TUI never shut the core down, so `SessionEnd` hooks and the
    // presence record outlived the window (T16.2).
    rt.block_on(quit.submit(Submission::Shutdown))?;
    Ok(())
}

/// The `tiers.code` provider decides which real client to build; every tier
/// of a session goes through the same provider object (routing picks models).
pub(crate) fn provider_for(config: &Config) -> anyhow::Result<Arc<dyn Provider>> {
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
        "openai" => {
            let o = &config.providers.openai;
            Ok(openai_shaped(
                "openai",
                &o.base_url,
                std::env::var(&o.api_key_env).ok(),
                o.models.clone(),
                400_000,
                &o.api,
            )?)
        }
        "local" => Ok(Arc::new(OpenAiChatProvider::new(&config.providers.local))),
        // Type-2 providers: no code per vendor — the section's `api` picks
        // the wire client, the section's base URL/key/models configure it.
        other => {
            let c = config
                .providers
                .custom
                .get(other)
                .ok_or_else(|| anyhow::anyhow!("unknown provider `{other}` in tiers.code"))?;
            Ok(openai_shaped(
                other,
                &c.base_url,
                std::env::var(&c.api_key_env).ok(),
                c.models.clone(),
                c.context_window,
                &c.api,
            )?)
        }
    }
}

/// Builds the OpenAI-shaped client the `api` string names for `owner`:
/// `"responses"` speaks the Responses API, `"chat"` the Chat Completions
/// subset every compatible vendor speaks. Anything else is a config error
/// at startup, not a mid-turn 404.
fn openai_shaped(
    owner: &str,
    base_url: &str,
    api_key: Option<String>,
    models: Vec<cox_protocol::config::ProviderModel>,
    context_window: u32,
    api: &str,
) -> anyhow::Result<Arc<dyn Provider>> {
    match api {
        "responses" => Ok(Arc::new(OpenAiResponsesProvider::new(
            base_url,
            api_key,
            models,
            context_window,
        ))),
        "chat" => Ok(Arc::new(OpenAiChatProvider::from_parts(
            base_url,
            api_key,
            models,
            context_window,
        ))),
        _ => anyhow::bail!(
            "unknown api `{api}` for provider `{owner}` (want \"chat\" or \"responses\")"
        ),
    }
}

/// Every built-in tool except `agent`, which the session adds itself.
pub(crate) fn tools(
    answer: Option<String>,
    store: &Arc<Store>,
    mdir: PathBuf,
) -> Vec<Arc<dyn Tool>> {
    let mem: Arc<dyn cox_protocol::Store> = store.clone();
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
        Arc::new(MemorySaveTool::new(mem.clone(), mdir.clone())),
        Arc::new(MemorySearchTool::new(mem, mdir)),
    ];
    let specs: Vec<_> = tools.iter().map(|t| t.spec()).collect();
    tools.push(Arc::new(ToolSearchTool::new(specs)));
    tools
}

/// Swaps local file/shell tools for client-backed ones where the ACP
/// client offers `fs`/`terminal` (T11.1 step 4), so the editor's buffers
/// stay authoritative. Names, subjects and risk classes are unchanged.
pub(crate) fn with_client_tools(
    tools: Vec<Arc<dyn Tool>>,
    link: cox_acp::ClientLink,
    fs: bool,
    terminal: bool,
) -> Vec<Arc<dyn Tool>> {
    use cox_acp::client_tools::{FsEditTool, FsReadTool, FsWriteTool, TerminalBashTool};
    let link = std::sync::Arc::new(link);
    tools
        .into_iter()
        .map(|t| match t.spec().name.as_str() {
            "read" if fs => Arc::new(FsReadTool::new(link.clone())) as Arc<dyn Tool>,
            "edit" if fs => Arc::new(FsEditTool::new(link.clone())) as Arc<dyn Tool>,
            "write" if fs => Arc::new(FsWriteTool::new(link.clone())) as Arc<dyn Tool>,
            "bash" if terminal => Arc::new(TerminalBashTool::new(link.clone())) as Arc<dyn Tool>,
            _ => t,
        })
        .collect()
}
/// Where a session's memory facts live: `config.memory.dir` wins, else
/// `<home>/projects/<slug>/memory` (T10.1).
pub(crate) fn memory_dir_for(config: &Config, home: &Path, cwd: &Path) -> PathBuf {
    if config.memory.dir.is_empty() {
        cox_ext::memory::memory_dir(home, cwd)
    } else {
        PathBuf::from(&config.memory.dir)
    }
}

#[cfg(test)]
mod tests {
    use cox_protocol::config::CompatibleProviderConfig;
    use cox_protocol::types::ProviderId;

    use super::*;

    fn deepseek_config(api: &str) -> Config {
        let mut cfg = Config::default();
        cfg.tiers.code.provider = "deepseek".into();
        cfg.providers.custom.insert(
            "deepseek".into(),
            CompatibleProviderConfig {
                base_url: "https://api.deepseek.com".into(),
                // Deliberately unset in the test environment: the client
                // builds keyless without touching the network.
                api_key_env: "COX_TEST_MISSING_KEY_DEEPSEEK".into(),
                api: api.into(),
                model: "deepseek-v4-pro".into(),
                context_window: 1_000_000,
                models: vec![],
            },
        );
        cfg
    }

    #[test]
    fn provider_for_custom_builds_chat_client_without_a_key() {
        let p = provider_for(&deepseek_config("chat")).expect("builds");
        assert_eq!(p.id(), ProviderId::Local);
        assert_eq!(p.capabilities().max_context, 1_000_000);
    }

    #[test]
    fn provider_for_custom_responses_builds_responses_client() {
        let p = provider_for(&deepseek_config("responses")).expect("builds");
        assert_eq!(p.id(), ProviderId::OpenAi);
    }

    #[test]
    fn provider_for_rejects_unknown_names_and_shapes() {
        let mut bad = Config::default();
        bad.tiers.code.provider = "weird".into();
        assert!(provider_for(&bad).is_err(), "unknown name bails");
        assert!(
            provider_for(&deepseek_config("smoke-signals")).is_err(),
            "unknown api bails at startup, not mid-turn"
        );
    }
}
