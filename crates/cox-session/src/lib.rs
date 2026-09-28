//! Session assembly (T37.1, DT§4.2): turns an effective `Config` into a
//! live [`Session`] — provider, built-in and MCP tools, skills, subagent
//! definitions, hooks, plugins, the checkpointer and worktrees. Separate
//! from `crates/cox` so every surface (TUI, `run -p`, ACP, the desktop app)
//! builds a session the same way without `clap`, `anyhow` or a terminal:
//! the caller loads config from its own flags, and what went wrong on the
//! way comes back as [`Warning`]s for it to show, never printed here.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cox_core::{History, Session};
use cox_protocol::Config;
use cox_protocol::errors::{CoreError, ProviderError};
use cox_protocol::ids::SessionId;
use cox_protocol::traits::{Hook, Store as _};
use cox_protocol::types::Level;
use cox_store::Store;
use cox_tools::ask_user::Question as AskUserQuestion;
use cox_tools::send_message::SendMessageTool;

#[cfg(feature = "plugins")]
pub mod external_agents;
pub mod lineage;
pub mod mcp;
pub mod plugins;
pub mod provider;
pub mod sandbox;
#[cfg(any(test, feature = "test-util"))]
pub mod testing;
pub mod tools;

pub use lineage::{fork, handoff, resume};
pub use mcp::{mcp_auth, mcp_servers};
#[cfg(feature = "plugins")]
pub use plugins::start_plugins;
pub use plugins::{Plugins, load_plugins, plugin_notices, write_grant};
pub use provider::{lmstudio_model, provider_for};
pub use sandbox::{sandbox_policy, sandboxed_argv};
pub use tools::{tools, with_client_tools};

/// Why a session could not be built. Anything an extension breaks is a
/// [`Warning`] instead (D14); these stop the session.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(transparent)]
    Store(#[from] cox_protocol::StoreError),
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    Price(#[from] cox_provider::usage::PriceError),
    /// T30.16: LM Studio did not answer for the session's model before any
    /// turn. `error` is not a `source`, so the message stays one line.
    #[error("LM Studio at {base_url}: {error}")]
    LmStudio {
        base_url: String,
        error: ProviderError,
    },
    #[error("unknown provider `{0}` in tiers.code")]
    UnknownProvider(String),
    #[error("unknown api `{api}` for provider `{owner}` (want \"chat\" or \"responses\")")]
    UnknownApi { api: String, owner: String },
}

/// Something skipped while the session was built; the session runs
/// without it (D14). The caller decides how to show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    /// A `SKILL.md` that did not parse (T22.2).
    Skill(String),
    /// A subagent definition that did not parse (T34.1).
    Agent(String),
    /// An MCP server that did not start, or runs unsandboxed (T7.6, T33.42).
    Mcp(String),
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (Self::Skill(text) | Self::Agent(text) | Self::Mcp(text)) = self;
        f.write_str(text)
    }
}

/// A surface's plugin renderer (T33.23, T33.44): given the live plugins,
/// starts serving their render requests and returns the event tap's
/// redraw. Only the TUI has one.
#[cfg(feature = "plugins")]
pub type ServeUi = Box<dyn FnOnce(&cox_plugin::LivePlugins) -> cox_plugin::Redraw + Send>;

/// The slim build has no plugin to render, so there is never a renderer.
#[cfg(not(feature = "plugins"))]
pub type ServeUi = std::convert::Infallible;

/// Everything [`open`] needs. `config` is the effective config the surface
/// loaded from its own flags; [`open`] fills in what an empty config means.
pub struct SessionSpec {
    pub config: Config,
    /// The session's working directory (the worktree, for a worktree run).
    pub cwd: PathBuf,
    /// `COX_HOME`: the store, skills, plugins and checkpoints live here.
    pub home: PathBuf,
    /// `cwd` is a worktree cox made (T27.3): the main checkout joins as a
    /// read-only root and only the worktree is writable.
    pub worktree: bool,
    /// What `ask_user` returns when no one is there to ask.
    pub answer: Option<String>,
    /// T22.1: a surface that shows `ask_user` questions itself.
    pub questions: Option<tokio::sync::mpsc::Sender<AskUserQuestion>>,
    /// The session to reopen and its rebuilt history.
    pub resume: Option<(SessionId, History)>,
    /// T22.5: with a person present, how an MCP server's 401 hands them the
    /// login URL; `None` makes a 401 a notice.
    pub mcp_login: Option<cox_mcp::client::Prompt>,
    /// T33.23, T33.44: renders the live plugins; only the TUI has one.
    pub plugin_ui: Option<ServeUi>,
    /// T37.2: an ACP client that offers `fs`/`terminal` backs the file and
    /// shell tools; only `cox acp` has one.
    pub client: Option<ClientTools>,
}

/// The ACP client's side of a session (T11.1): the link its proxy tools
/// call back through and which of `fs`/`terminal` it offers.
pub struct ClientTools {
    pub link: cox_acp::ClientLink,
    pub fs: bool,
    pub terminal: bool,
}

/// A built session.
pub struct Opened {
    pub session: Session,
    /// `spec.config` with the defaults [`open`] filled in, before granted
    /// plugins' provider sections joined it.
    pub config: Config,
    /// In the order they happened.
    pub warnings: Vec<Warning>,
}

/// Opens the store under `spec.home`, picks the provider (`COX_PROVIDER`
/// test doubles first) and builds the session with every tool, hook and
/// plugin it gets.
pub async fn open(spec: SessionSpec) -> Result<Opened, SessionError> {
    let SessionSpec {
        config: mut base,
        cwd,
        home,
        worktree,
        answer,
        questions,
        resume,
        mcp_login,
        plugin_ui,
        client,
    } = spec;
    let cwd = cwd.as_path();
    let mut warnings = Vec::new();
    // §1.6: empty `workspace_roots` means the git root of cwd, else cwd.
    if base.core.workspace_roots.is_empty() {
        base.core.workspace_roots =
            vec![cox_config::load::find_git_root(cwd).unwrap_or_else(|| cwd.to_path_buf())];
    }
    let worktree_main = if worktree {
        let main = project_root(cwd).await;
        add_read_root(&mut base, &main);
        Some(main)
    } else {
        None
    };
    // T9.1 step 4 (generalised): a non-first-party `tiers.code.provider`
    // maps every tier to the same server; the router then pins each tier to
    // that provider's section model, so a `--provider deepseek` flip works
    // without editing every tier model.
    if !["anthropic", "openai"].contains(&base.tiers.code.provider.as_str()) {
        for tier in [&mut base.tiers.cheap, &mut base.tiers.think] {
            tier.provider = base.tiers.code.provider.clone();
        }
    }
    let mut config = base.clone();
    let store = Arc::new(Store::open(&home)?);
    // T33.19: granted plugins load before the provider is built and before
    // MCP discovery: a granted plugin's declarative `[[provider]]` rows
    // (T33.17) must already be in `config.providers.custom` when
    // `provider_for`/`provider_for_served` below pick the tier's client,
    // and its `[[mcp]]` servers join MCP discovery further down. A
    // worktree session's plugin server may write only the worktree, like
    // its `bash` (`set_writable_roots` below).
    let writable = match worktree_main {
        Some(_) => vec![cwd.to_path_buf()],
        None => config.core.workspace_roots.clone(),
    };
    let plugins = load_plugins(&config, &home, cwd, store.clone(), Some(&writable));
    config.providers.custom.extend(plugins.providers.clone());
    // T33.44: granted plugins' `[[models]]` join the catalog the provider
    // reads its context window from (PL§7b).
    let plugin_models = plugins.catalog_rows();
    // T30.16: ask LM Studio what it runs before the provider is built, so
    // the loaded context becomes the session's window. The key resolved
    // here is reused for the chat client: one keyring read, not two.
    let served = provider::lmstudio_served(&config).await?;
    let provider = match &served {
        Some(s) => {
            let key = s.api_key.clone();
            provider::provider_for_served(
                &config,
                move |_, _| key.ok_or(ProviderError::Auth),
                Some(&s.model),
                &plugin_models,
            )?
        }
        None => provider::provider_for_served(
            &config,
            cox_provider::http::resolve_key,
            None,
            &plugin_models,
        )?,
    };
    let mdir = memory_dir_for(&base, &home, cwd);
    // T27.3: a worktree session's project is still the main checkout, so
    // the sessions of one repository see each other whatever tree they edit.
    let project = project_root(cwd).await;
    // T22.2: `SKILL.md` files are discovered once per session build; the
    // `skill` tool hands bodies out on demand, and a broken skill is a
    // warning and skipped, never fatal (D14).
    let claude_home = cox_config::load::home_dir().join(".claude");
    let found = cox_ext::skills::discover(&cox_ext::skills::skill_dirs(
        Some(&home),
        Some(&claude_home),
        Some(&project),
    ));
    warnings.extend(found.notices.into_iter().map(Warning::Skill));
    // T34.1: subagent definitions are discovered once here, at session
    // build, the same roots `cox ext list` reads — never inside `cox-core`,
    // which does no filesystem I/O of its own (`agent_defs` on `Session`
    // is set below, after construction, like `set_worktrees`).
    let agents_found = cox_ext::agents::discover(&cox_ext::agents::agent_dirs(
        Some(&home),
        Some(&claude_home),
        Some(&project),
    ));
    warnings.extend(agents_found.notices.into_iter().map(Warning::Agent));
    let mut all = tools(answer, &store, mdir);
    if let Some(tx) = questions {
        all = tools::with_question_surface(all, tx);
    }
    if let Some(c) = client {
        all = with_client_tools(all, c.link, c.fs, c.terminal);
    }
    // T34.6: stateless — the session that builds each call's own `ToolCx`
    // (`cox-core/src/turn.rs`) stamps `ToolCx.relay` with itself, so this
    // one shared instance still reaches each caller's own session, never a
    // handle fixed at construction time (SM§4; no preset grants it to a
    // child yet, but a shared instance must be safe if one someday does).
    all.push(Arc::new(SendMessageTool));
    // T22.2: the deferred `skill` tool hands skill bodies out on demand
    // (its spec is `deferred`, `ReadOnly`; broken skills are skipped above,
    // D14); `with_tool_search_index` below makes it discoverable.
    all.push(Arc::new(cox_ext::skills::SkillTool::new(found.skills)));
    if config.mcp.enabled {
        let (mcp, notices) = mcp::mcp_tools(&config, cwd, mcp_login, plugins.mcp, &writable).await;
        all.extend(mcp);
        warnings.extend(notices.into_iter().map(Warning::Mcp));
    }
    #[cfg_attr(not(feature = "plugins"), allow(unused_mut))]
    let mut plugin_warnings = plugins.notices;
    let id = resume.as_ref().map_or_else(SessionId::new, |(id, _)| *id);
    #[cfg(feature = "plugins")]
    let mut live = plugins.live;
    #[cfg(feature = "plugins")]
    all.extend(plugins::plugin_tools(
        &mut live,
        &base.plugins,
        id,
        cwd,
        &mut plugin_warnings,
    ));
    let all = tools::with_tool_search_index(all);
    let session = match resume {
        Some((id, history)) => Session::resume(
            config,
            provider,
            all,
            store.clone(),
            store,
            cwd.to_path_buf(),
            id,
            history,
        )?,
        None => Session::new_with_id(
            id,
            config,
            provider,
            all,
            store.clone(),
            store,
            cwd.to_path_buf(),
        )?,
    };
    if worktree_main.is_some() {
        session.set_writable_roots(vec![cwd.to_path_buf()]);
    }
    session.set_agent_defs(agents_found.agents);
    // T35.13: one driver per granted `[[external_agents]]` entry; an entry
    // whose CLI or key is missing is left out with one warning (EA§7).
    #[cfg(feature = "plugins")]
    let plugin_warnings = {
        let (drivers, left_out) = external_agents::drivers(
            plugins.external_agents,
            &base,
            cwd,
            &writable,
            std::env::var_os("PATH").as_deref(),
            cox_provider::http::resolve_key,
        );
        session.set_external_agents(drivers);
        plugin_warnings
            .into_iter()
            .chain(left_out)
            .collect::<Vec<_>>()
    };
    // T33.44: each granted plugin's `cox_init` ran once, in `plugin_tools`
    // above; its instance is shared by its tools, its hooks (below) and the
    // event tap `start_plugins` sets.
    #[cfg(feature = "plugins")]
    let (plugin_hooks, plugin_started) =
        start_plugins(&session, live, &base.plugins, cwd, plugin_ui);
    #[cfg(not(feature = "plugins"))]
    let (plugin_hooks, plugin_started): plugins::Started = {
        let _ = plugin_ui;
        (Vec::new(), Vec::new())
    };
    // A14: the presence hook wraps the user's shell hooks so the other
    // sessions of this workspace see every surface, `--no-hooks` or not;
    // PL§6: plugin hooks follow the shell's in one chain, and `--no-hooks`
    // turns off only the shell's.
    let shell: Option<Arc<dyn Hook>> = base.hooks.enabled.then(|| {
        Arc::new(cox_ext::hooks::ShellHooks::new(
            &base.hooks,
            cwd.to_path_buf(),
        )) as Arc<dyn Hook>
    });
    session.set_hook(Arc::new(
        cox_ext::presence::PresenceHook::new(
            home.clone(),
            session.id(),
            cwd.to_path_buf(),
            project,
            Some(Arc::new(cox_ext::hooks::HookChain::new(
                shell,
                plugin_hooks,
            ))),
        )
        .with_worktree(worktree.then(|| cwd.to_path_buf())),
    ));
    // T26.1: pre-images for `/rewind` live in private git dirs under home.
    session.set_checkpointer(Arc::new(cox_tools::checkpoint::GitCheckpointer::new(
        home.clone(),
    )));
    // T27.3: `agent(isolation: "worktree")` gets real worktrees on every surface.
    session.set_worktrees(Arc::new(cox_tools::git::GitWorktrees));
    for warning in served.iter().flat_map(|s| s.model.warnings()) {
        session.notice(Level::Warn, warning).await?;
    }
    for warning in plugin_warnings {
        session.notice(Level::Warn, warning).await?;
    }
    for (level, text) in plugin_started {
        session.notice(level, text).await?;
    }
    Ok(Opened {
        session,
        config: base,
        warnings,
    })
}

/// One worktree-aware project identity for presence writes and polling.
pub async fn project_root(cwd: &Path) -> PathBuf {
    cox_tools::git::project_root(cwd).await.unwrap_or_else(|_| {
        cox_config::load::find_git_root(cwd).unwrap_or_else(|| cwd.to_path_buf())
    })
}

/// Adds `root` to the workspace roots once (T27.3: the main checkout of a
/// worktree session, readable, not writable).
pub fn add_read_root(config: &mut Config, root: &Path) {
    if !config.core.workspace_roots.iter().any(|r| r == root) {
        config.core.workspace_roots.push(root.to_path_buf());
    }
}

/// Where a session's memory facts live: `config.memory.dir` wins, else
/// `<home>/projects/<slug>/memory` (T10.1).
pub fn memory_dir_for(config: &Config, home: &Path, cwd: &Path) -> PathBuf {
    if config.memory.dir.is_empty() {
        cox_ext::memory::memory_dir(home, cwd)
    } else {
        PathBuf::from(&config.memory.dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T37.1: a broken `SKILL.md` comes back from `open` as a
    /// `Warning::Skill`, for the caller to show, and the session still opens.
    #[test]
    fn open_returns_skill_warnings_as_data() {
        let home = tempfile::tempdir().expect("home");
        let work = tempfile::tempdir().expect("work");
        let bad = home.path().join("skills/bad");
        std::fs::create_dir_all(&bad).expect("skill dir");
        std::fs::write(bad.join("SKILL.md"), "no front matter\n").expect("skill");
        let scenario = work.path().join("scenario.toml");
        std::fs::write(&scenario, "[[turn]]\ntext = \"hi\"\n").expect("scenario");
        let mut config = Config::default();
        // Nothing from the developer's own MCP servers, plugins or hooks.
        config.mcp.enabled = false;
        config.plugins.enabled = false;
        config.hooks.enabled = false;
        let spec = SessionSpec {
            config,
            cwd: work.path().to_path_buf(),
            home: home.path().to_path_buf(),
            worktree: false,
            answer: None,
            questions: None,
            resume: None,
            mcp_login: None,
            plugin_ui: None,
            client: None,
        };
        let scenario = scenario.display().to_string();
        let vars = [
            ("COX_PROVIDER", Some("scripted")),
            ("COX_SCENARIO", Some(scenario.as_str())),
        ];
        cox_config::load::temp_env(&vars, || {
            let rt = tokio::runtime::Runtime::new().expect("runtime");
            let opened = rt.block_on(open(spec)).expect("the session opens");
            // Only this home's skill: `~/.claude/skills` is read too.
            let ours: Vec<_> = opened
                .warnings
                .iter()
                .filter(|w| w.to_string().contains("skills/bad/SKILL.md skipped"))
                .collect();
            assert!(
                matches!(ours.as_slice(), [Warning::Skill(_)]),
                "{:?}",
                opened.warnings
            );
            assert_eq!(
                opened.config.core.workspace_roots,
                vec![work.path().to_path_buf()],
                "an empty root list became cwd"
            );
        });
    }
}
