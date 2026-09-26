//! Builds a live [`Session`] for the interactive surfaces: config, provider,
//! store and the built-in tool set. Kept out of `main.rs` so the TUI and
//! `cox run -p` (T6.1) assemble the same session the same way.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cox_core::{History, Session};
use cox_protocol::Config;
#[cfg(feature = "plugins")]
use cox_protocol::GrantScope;
use cox_protocol::config::{CompatibleProviderConfig, McpServerConfig, Transport};
use cox_protocol::ids::{ItemId, SessionId};
use cox_protocol::traits::{Hook, Provider, SessionRow, Store as _, Tool};
use cox_protocol::types::{Event, ItemKind, Job, Level, Submission};
use cox_provider::anthropic::{AnthropicProvider, CacheTtl};
use cox_provider::openai::chat::OpenAiChatProvider;
use cox_provider::openai::responses::OpenAiResponsesProvider;
use cox_provider::usage::{PriceTable, Priced};
use cox_store::Store;
use cox_tools::ask_user::{Answers, AskUserTool, Question as AskUserQuestion};
use cox_tools::bash::BashTool;
use cox_tools::edit::EditTool;
use cox_tools::expand::ExpandTool;
use cox_tools::glob::GlobTool;
use cox_tools::grep::GrepTool;
use cox_tools::memory::{MemorySaveTool, MemorySearchTool};
use cox_tools::read::ReadTool;
use cox_tools::send_message::SendMessageTool;
use cox_tools::todo::TodoTool;
use cox_tools::tool_search::ToolSearchTool;
use cox_tools::v4a::ApplyPatchTool;
use cox_tools::web_fetch::WebFetchTool;
use cox_tools::write::WriteTool;
use cox_tui::state::{Ask, GitStatus, Msg, State};

use crate::cli::Cli;
use crate::config_cmd;
use crate::config_load::{self, LoadedConfig};
use crate::resume;

/// Loads config, picks the provider (`COX_PROVIDER` test doubles first) and
/// opens the store under `COX_HOME`. `answer` is what `ask_user` returns
/// when no one is there to ask; `questions` (T22.1) lets a surface — only
/// `run_tui` has one — take `ask_user` over instead, answering each call
/// interactively rather than with `answer`. `tweak` lets a surface adjust
/// the effective config before the session locks it in; `interactive` says
/// a person is at the terminal, so an MCP server's 401 may open a browser
/// login (T22.5). `plugin_ui` (T33.23, T33.44) — only `run_tui` has one —
/// takes the TUI's plugin feed and render requests, so the live plugins
/// can be rendered and ask for redraws.
#[allow(clippy::too_many_arguments)]
pub async fn open(
    cli: &Cli,
    cwd: &Path,
    answer: Option<String>,
    questions: Option<tokio::sync::mpsc::Sender<AskUserQuestion>>,
    tweak: impl FnOnce(&mut Config),
    resume: Option<(SessionId, History)>,
    interactive: bool,
    plugin_ui: Option<PluginUi>,
) -> anyhow::Result<(Session, LoadedConfig)> {
    let mut loaded = config_load::load(cwd, cli)?;
    tweak(&mut loaded.config);
    // §1.6: empty `workspace_roots` means the git root of cwd, else cwd.
    if loaded.config.core.workspace_roots.is_empty() {
        loaded.config.core.workspace_roots =
            vec![config_load::find_git_root(cwd).unwrap_or_else(|| cwd.to_path_buf())];
    }
    let worktree_main = if cli.worktree.is_some() {
        let main = project_root(cwd).await;
        add_read_root(&mut loaded.config, &main);
        Some(main)
    } else {
        None
    };
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
    let mut config = loaded.config.clone();
    let home = cli.home.clone().unwrap_or_else(config_load::cox_home);
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
    let served = lmstudio_served(&config).await?;
    let provider = match &served {
        Some(s) => {
            let key = s.api_key.clone();
            provider_for_served(
                &config,
                move |_, _| key.ok_or(cox_protocol::errors::ProviderError::Auth),
                Some(&s.model),
                &plugin_models,
            )?
        }
        None => provider_for_served(
            &config,
            cox_provider::http::resolve_key,
            None,
            &plugin_models,
        )?,
    };
    let mdir = memory_dir_for(&loaded.config, &home, cwd);
    // T27.3: a worktree session's project is still the main checkout, so
    // the sessions of one repository see each other whatever tree they edit.
    let project = project_root(cwd).await;
    // T22.2: `SKILL.md` files are discovered once per session build; the
    // `skill` tool hands bodies out on demand, and a broken skill is a
    // warning and skipped, never fatal (D14).
    let claude_home = config_load::home_dir().join(".claude");
    let found = cox_ext::skills::discover(&cox_ext::skills::skill_dirs(
        Some(&home),
        Some(&claude_home),
        Some(&project),
    ));
    for notice in &found.notices {
        eprintln!("cox: warning: {notice}");
    }
    // T34.1: subagent definitions are discovered once here, at session
    // build, the same roots `cox ext list` reads — never inside `cox-core`,
    // which does no filesystem I/O of its own (`agent_defs` on `Session`
    // is set below, after construction, like `set_worktrees`).
    let agents_found = cox_ext::agents::discover(&cox_ext::agents::agent_dirs(
        Some(&home),
        Some(&claude_home),
        Some(&project),
    ));
    for notice in &agents_found.notices {
        eprintln!("cox: warning: {notice}");
    }
    let mut all = tools(answer, &store, mdir);
    if let Some(tx) = questions {
        all = with_question_surface(all, tx);
    }
    // T34.6: stateless — the session that builds each call's own `ToolCx`
    // (`cox-core/src/turn.rs`) stamps `ToolCx.relay` with itself, so this
    // one shared instance still reaches each caller's own session, never a
    // handle fixed at construction time (SM§4; no preset grants it to a
    // child yet, but a shared instance must be safe if one someday does).
    all.push(Arc::new(SendMessageTool));
    // T22.2: the deferred `skill` tool hands skill bodies out on demand
    // (its spec is `deferred`, `ReadOnly`; broken skills are skipped above,
    // D14). `tool_search` answers from the spec list it was built with, so
    // its index is rebuilt over the full set — the swap-by-name shape of
    // `with_question_surface` — or the deferred `skill` could never be
    // discovered (D6d).
    all.push(Arc::new(cox_ext::skills::SkillTool::new(found.skills)));
    let specs: Vec<_> = all.iter().map(|t| t.spec()).collect();
    all = all
        .into_iter()
        .map(|t| match t.spec().name.as_str() {
            "tool_search" => Arc::new(ToolSearchTool::new(specs.clone())) as Arc<dyn Tool>,
            _ => t,
        })
        .collect();
    if config.mcp.enabled {
        all.extend(mcp_tools(&config, cwd, interactive, plugins.mcp, &writable).await);
    }
    let plugin_warnings = plugins.notices;
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
        None => Session::new(
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
    // T33.44: each granted plugin's `cox_init` runs once, now that the
    // session id exists; its instance is shared by its hooks (below) and
    // the event tap `start_plugins` sets.
    #[cfg(feature = "plugins")]
    let (plugin_hooks, plugin_started) = start_plugins(
        &session,
        plugins.live,
        &loaded.config.plugins,
        cwd,
        plugin_ui,
    );
    #[cfg(not(feature = "plugins"))]
    let (plugin_hooks, plugin_started) = {
        drop(plugin_ui);
        (Vec::new(), Vec::new())
    };
    // A14: the presence hook wraps the user's shell hooks so the other
    // sessions of this workspace see every surface, `--no-hooks` or not;
    // PL§6: plugin hooks follow the shell's in one chain, and `--no-hooks`
    // turns off only the shell's.
    let shell: Option<Arc<dyn Hook>> = loaded.config.hooks.enabled.then(|| {
        Arc::new(cox_ext::hooks::ShellHooks::new(
            &loaded.config.hooks,
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
        .with_worktree(cli.worktree.as_ref().map(|_| cwd.to_path_buf())),
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
    Ok((session, loaded))
}

/// The TUI's ends of the plugin UI channels (T33.23): where `Msg::Plugin`
/// answers go, and the `Cmd::Plugin` requests to serve. `open` hands them
/// to the live plugins (T33.44).
#[cfg_attr(
    not(feature = "plugins"),
    expect(dead_code, reason = "the slim build has no plugin to render")
)]
pub struct PluginUi {
    /// The TUI's feed.
    pub feed: tokio::sync::mpsc::Sender<Msg>,
    /// The TUI's render requests.
    pub requests: tokio::sync::mpsc::Receiver<cox_tui::state::PluginRequest>,
}

/// What the grant check let into a session (T33.6, T33.19).
#[derive(Default)]
pub(crate) struct Plugins {
    /// One warning per plugin or plugin server that did not load.
    pub notices: Vec<String>,
    /// Each loaded plugin's `[[mcp]]` servers by plugin id, stdio commands
    /// already sandboxed, for `cox_mcp::discovery::add_plugin`.
    pub mcp: Vec<(String, Vec<(String, McpServerConfig)>)>,
    /// New `providers.custom` sections from each granted plugin's
    /// declarative `[[provider]]` rows (`docs/design/plugins.md` §7a,
    /// T33.17): `api = "chat" | "responses"` only, already checked against
    /// the live config by `cox_plugin::provider::merge` (a name the config
    /// already uses is skipped with a notice, not returned here). The
    /// caller extends `config.providers.custom` with these before building
    /// the tier's provider.
    pub providers: HashMap<String, CompatibleProviderConfig>,
    /// Each granted plugin's `[[external_agents]]` entries (EA§2, T35.2),
    /// resolved like a `[[mcp]]` stdio server and already wrapped by
    /// `sandboxed_argv`: a driver spawns `ExternalAgentCommand::command`.
    /// Never an ungranted plugin's; empty without `writable`.
    #[cfg(feature = "plugins")]
    #[allow(dead_code, reason = "T35.13's drivers are the first reader")]
    pub external_agents: Vec<cox_plugin::external_agent::ExternalAgentCommand>,
    /// Each loaded plugin's `[[models]]` rows by plugin id (T33.44).
    pub models: Vec<(String, Vec<cox_protocol::plugin::ModelDecl>)>,
    /// The loaded instances, compiled under their grants; `start_plugins`
    /// runs their `cox_init` once the session exists (T33.44).
    #[cfg(feature = "plugins")]
    pub live: cox_plugin::LivePlugins,
}

impl Plugins {
    /// `models` in the shape `Catalog::load` takes.
    pub fn catalog_rows(&self) -> Vec<cox_models::PluginModels<'_>> {
        self.models
            .iter()
            .map(|(plugin, models)| cox_models::PluginModels { plugin, models })
            .collect()
    }
}

/// The warnings of `load_plugins`, for a surface with no MCP servers (ACP).
pub(crate) fn plugin_notices(
    config: &Config,
    home: &Path,
    cwd: &Path,
    store: Arc<dyn cox_protocol::PluginStore>,
) -> Vec<String> {
    load_plugins(config, home, cwd, store, None).notices
}

/// T33.6 (PL§3): discovers plugins, checks each against its grant and
/// loads only the `Granted` ones. Warns once per plugin that did not load,
/// naming the command to run — headless and ACP never approve, matching
/// headless approval with no approver (`run.rs`). Empty when
/// `plugins.enabled` is off. A store read error counts as "no grant": it
/// must never let an unapproved plugin load. With `writable` (the roots a
/// server may write), a loaded plugin's `[[mcp]]` servers come back too.
#[cfg(feature = "plugins")]
pub(crate) fn load_plugins(
    config: &Config,
    home: &Path,
    cwd: &Path,
    store: Arc<dyn cox_protocol::PluginStore>,
    writable: Option<&[PathBuf]>,
) -> Plugins {
    use cox_plugin::discover::{self, State};
    use cox_plugin::grant::{self, Verdict};

    let mut out = Plugins::default();
    if !config.plugins.enabled {
        return out;
    }
    let root = config_load::find_git_root(cwd);
    let found = discover::discover(home, root.as_deref());
    let notices = &mut out.notices;
    notices.extend(found.notices);
    // T33.17: each granted, successfully loaded plugin's `[[provider]]`
    // rows, collected alongside `[[mcp]]` below and merged into
    // `providers.custom` once every plugin has been checked (PL§7a).
    let mut plugin_providers: Vec<cox_plugin::provider::PluginProviders<'_>> = Vec::new();
    for p in &found.plugins {
        let id = &p.id;
        let (manifest, digest) = match &p.state {
            State::Loaded { manifest, digest } => (manifest, digest),
            State::Skipped { reason } => {
                notices.push(format!("plugin {id} skipped: {reason}"));
                continue;
            }
        };
        let stored = grant::scope(p.source, root.as_deref())
            .and_then(|scope| store.grant_get(id, &scope, digest).ok().flatten());
        let enable = grant::enable_command(id, p.source);
        match grant::check(manifest, digest, stored.as_ref()) {
            Verdict::Granted => {
                // T33.44: compiled once under its real grant and kept;
                // `start_plugins` runs `cox_init`. A failure is a visible
                // warning, never fatal (D14).
                let loaded = std::fs::read(p.dir.join(&manifest.wasm))
                    .map_err(|e| e.to_string())
                    .and_then(|wasm| {
                        out.live
                            .load(manifest, &wasm, store.clone())
                            .map_err(|e| e.to_string())
                    });
                if loaded.is_ok() && !manifest.provider.is_empty() {
                    plugin_providers.push(cox_plugin::provider::PluginProviders {
                        plugin: id.as_str(),
                        decls: &manifest.provider,
                    });
                }
                if loaded.is_ok() && !manifest.models.is_empty() {
                    out.models.push((id.clone(), manifest.models.clone()));
                }
                match (loaded, writable) {
                    (Err(e), _) => notices.push(format!("plugin {id} failed to load: {e}")),
                    (Ok(_), Some(writable)) => {
                        if !manifest.mcp.is_empty() {
                            let servers =
                                plugin_mcp(id, &p.dir, manifest, config, writable, notices);
                            out.mcp.push((id.clone(), servers));
                        }
                        let agents = plugin_agents(id, &p.dir, manifest, config, writable, notices);
                        out.external_agents.extend(agents);
                    }
                    (Ok(_), None) => {}
                }
            }
            Verdict::Disabled => {
                notices.push(format!(
                    "plugin {id} is disabled; run `{enable}` to load it"
                ));
            }
            Verdict::NeedsApproval { added, .. } => {
                let asks = if added.is_empty() {
                    String::from("its package changed")
                } else {
                    format!("it asks for {}", added.join(", "))
                };
                notices.push(format!(
                    "plugin {id} is not loaded: {asks} and needs approval; run `{enable}`"
                ));
            }
        }
    }
    let merged = cox_plugin::provider::merge(&config.providers, &plugin_providers);
    out.notices.extend(merged.warnings);
    out.providers = merged.custom;
    // PL§7b: a plugin row that would change a configured or built-in one
    // is ignored; the catalog says which, and the session shows it.
    if !out.models.is_empty()
        && let Ok(catalog) = cox_models::Catalog::load(config, &out.catalog_rows(), None)
    {
        out.notices.extend(catalog.warnings().iter().cloned());
    }
    out
}

/// T33.44 (PL§3–§6): runs each loaded plugin's `cox_init` once for
/// `session` and sets the event tap, which owns the instances from then on.
/// Returns the plugins' hook sources for `HookChain` and the notices to
/// emit now: init failures (that plugin is skipped) and what `cox_init`
/// queued. Later notices (`cox_notify` from a hook, `Effects.notices`) are
/// drained by the tap after each event and emitted by a task, because
/// `EventTap::offer` runs inside `Session::emit` and must not emit itself.
/// `start_plugins`'s answer: the plugins' hook sources for `HookChain`
/// and the notices to emit now.
#[cfg(feature = "plugins")]
type Started = (Vec<(String, Arc<dyn Hook>)>, Vec<(Level, String)>);

#[cfg(feature = "plugins")]
fn start_plugins(
    session: &Session,
    mut live: cox_plugin::LivePlugins,
    config: &cox_protocol::config::PluginsConfig,
    cwd: &Path,
    ui: Option<PluginUi>,
) -> Started {
    let mut notices = Vec::new();
    // T33.15: plugins reach the session's router only through a `Weak`;
    // the notice task below holds the one strong handle.
    let caller: Arc<dyn cox_protocol::traits::ModelCaller> = Arc::new(session.clone());
    live.bind_model_caller(&caller);
    if !live.plugins().is_empty() {
        let started = live.start(config, session.id(), cwd);
        notices.extend(started.into_iter().map(|w| (Level::Warn, w)));
        // Taken before the tap is set, so the caller emits them in order
        // instead of the tap's own drain racing it.
        notices.extend(live.take_notices());
    }
    // Non-TUI surfaces have no slots to redraw.
    let redraw = match ui {
        Some(ui) => serve_plugin_ui(&live, ui),
        None => Arc::new(|_: &str| {}),
    };
    if live.plugins().is_empty() {
        return (Vec::new(), notices);
    }
    let hooks = live.hooks();
    // T33.20: the session asks only the plugin `[plugins.decide]` names.
    session.set_advisors(live.advisors());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(Level, String)>();
    let emitter = session.clone();
    tokio::spawn(async move {
        let _caller = caller;
        while let Some((level, text)) = rx.recv().await {
            if emitter.notice(level, text).await.is_err() {
                break;
            }
        }
    });
    let forward: cox_plugin::Notices = Arc::new(move |batch| {
        for notice in batch {
            let _ = tx.send(notice);
        }
    });
    let (tap, warnings) = live.into_tap(redraw, forward);
    notices.extend(warnings.into_iter().map(|w| (Level::Warn, w)));
    session.set_event_tap(Arc::new(tap));
    (hooks, notices)
}

/// T33.23's render server over the live hosts, and each plugin's granted
/// status slots, commands and keys (T33.25) declared on the feed, which
/// renders/registers them the first time. A plugin with commands or keys
/// but no status slot still gets a `Declare`. Returns the tap's `Redraw`.
/// A server thread that fails to start only leaves plugin segments
/// unrendered.
#[cfg(feature = "plugins")]
fn serve_plugin_ui(live: &cox_plugin::LivePlugins, ui: PluginUi) -> cox_plugin::Redraw {
    use cox_tui::state::PluginUiMsg;

    let _ = crate::plugin_ui::serve(live.hosts(), ui.requests, ui.feed.clone());
    let declares: Vec<_> = live
        .plugins()
        .iter()
        .map(|p| {
            (
                p.id().to_string(),
                p.granted_status(),
                p.granted_commands(),
                p.granted_keys(),
            )
        })
        .filter(|(_, slots, commands, keys)| {
            !slots.is_empty() || !commands.is_empty() || !keys.is_empty()
        })
        .collect();
    let feed = ui.feed.clone();
    tokio::spawn(async move {
        for (plugin, slots, commands, keys) in declares {
            let msg = Msg::Plugin(PluginUiMsg::Declare {
                plugin,
                slots,
                commands,
                keys,
            });
            if feed.send(msg).await.is_err() {
                break;
            }
        }
    });
    crate::plugin_ui::redraw(ui.feed)
}

/// The slim build has no plugin host, so there is never a plugin to load.
#[cfg(not(feature = "plugins"))]
pub(crate) fn load_plugins(
    _config: &Config,
    _home: &Path,
    _cwd: &Path,
    _store: Arc<dyn cox_protocol::PluginStore>,
    _writable: Option<&[PathBuf]>,
) -> Plugins {
    Plugins::default()
}

/// PL§7c: one plugin's `[[mcp]]` entries as servers `cox-mcp` can start. A
/// server that cannot be started safely is a warning and absent (D14).
#[cfg(feature = "plugins")]
fn plugin_mcp(
    id: &str,
    dir: &Path,
    manifest: &cox_plugin_api::PluginManifest,
    config: &Config,
    writable: &[PathBuf],
    notices: &mut Vec<String>,
) -> Vec<(String, McpServerConfig)> {
    let mut servers = Vec::new();
    for decl in &manifest.mcp {
        let server = match (&decl.command, &decl.url) {
            (Some(command), _) => cox_plugin::external_agent::package_program(dir, command)
                .map_err(|e| e.to_string())
                .and_then(|program| sandboxed_argv(&program, &decl.args, config, writable))
                .map(|mut argv| McpServerConfig {
                    command: Some(argv.remove(0)),
                    args: argv,
                    ..McpServerConfig::default()
                }),
            (None, Some(url)) => reqwest::Url::parse(url)
                .map_err(|e| format!("url {url:?}: {e}"))
                .and_then(|u| match u.host_str() {
                    Some(host) if manifest.capabilities.net_allows(host) => Ok(()),
                    host => Err(format!("url host {host:?} is not in capabilities.net")),
                })
                .map(|()| McpServerConfig {
                    url: Some(url.clone()),
                    ..McpServerConfig::default()
                }),
            (None, None) => Err(String::from("no command or url")),
        };
        match server {
            Ok(cfg) => servers.push((decl.name.clone(), cfg)),
            Err(why) => notices.push(format!(
                "plugin {id}: mcp server {} skipped: {why}",
                decl.name
            )),
        }
    }
    servers
}

/// EA§2 (T35.2): one granted plugin's `[[external_agents]]` entries, each
/// resolved like a `[[mcp]]` stdio server and wrapped by `sandboxed_argv`.
/// It is plugin-shipped code, so where the wrap is impossible the entry is
/// refused with a warning (wrap-or-refuse, as `plugin_mcp`), never run bare.
#[cfg(feature = "plugins")]
fn plugin_agents(
    id: &str,
    dir: &Path,
    manifest: &cox_plugin_api::PluginManifest,
    config: &Config,
    writable: &[PathBuf],
    notices: &mut Vec<String>,
) -> Vec<cox_plugin::external_agent::ExternalAgentCommand> {
    use cox_plugin::external_agent::ExternalAgentCommand;

    let wrap = |program: &Path, args: &[String]| sandboxed_argv(program, args, config, writable);
    let mut agents = Vec::new();
    for decl in &manifest.external_agents {
        match ExternalAgentCommand::resolve(id, dir, decl, wrap) {
            Ok(agent) => agents.push(agent),
            Err(e) => notices.push(format!(
                "plugin {id}: external agent {} skipped: {e}",
                decl.name
            )),
        }
    }
    agents
}

/// `program args` under `sandbox::command`, the guard `bash` runs under. The
/// backend wraps a `<shell> -c <line>` triple last, so the line
/// `exec "$0" "$@"` with the argv appended runs the program with no shell
/// quoting to get wrong. Landlock confines in a pre-exec hook that no argv
/// can carry, so it — like a host with no backend — refuses the server
/// rather than run it bare; `danger-full-access` is the user's own choice.
/// Shared by a plugin's `[[mcp]]` servers (`plugin_mcp`), its external
/// agents (`plugin_agents`, T35.2) and, T33.42, every other stdio server
/// (`sandbox_stdio_servers`) — one wrap, not two. `pub(crate)` so
/// `doctor::check_external_agents` (T35.8) can probe a granted entry's
/// `--version` under the same wrap, without a second sandbox-wrap
/// implementation.
pub(crate) fn sandboxed_argv(
    program: &Path,
    args: &[String],
    config: &Config,
    writable: &[PathBuf],
) -> Result<Vec<String>, String> {
    use cox_protocol::{SandboxMode, SandboxPolicy};
    use cox_tools::sandbox::{self, Backend};

    let policy = SandboxPolicy {
        mode: config.sandbox.mode,
        network: config.sandbox.network,
        writable: config.sandbox.writable.clone(),
        readonly_in_workspace: config.sandbox.readonly_in_workspace.clone(),
        linux_backend: config.sandbox.linux_backend,
    };
    if policy.mode != SandboxMode::DangerFullAccess {
        match sandbox::backend(policy.linux_backend) {
            Some(Backend::Seatbelt | Backend::Bwrap) => {}
            Some(Backend::Landlock) => {
                return Err(String::from(
                    "the landlock sandbox cannot wrap a server's argv",
                ));
            }
            None => return Err(String::from("no sandbox backend on this host")),
        }
    }
    let line = r#"exec "$0" "$@""#;
    let cmd = sandbox::command(
        &policy,
        &config.core.workspace_roots,
        writable,
        Path::new("/bin/sh"),
        line,
    )
    .map_err(|e| format!("sandbox: {e}"))?;
    let mut argv: Vec<String> = std::iter::once(cmd.get_program())
        .chain(cmd.get_args())
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    argv.push(program.to_string_lossy().into_owned());
    argv.extend(args.iter().cloned());
    Ok(argv)
}

/// T33.42 (`docs/design/plugins.md` §7c/§14 decision 4): every stdio MCP
/// server — config, `.mcp.json`, `~/.claude.json` — runs under the same
/// `sandboxed_argv` wrap a plugin's server gets, unless its config sets
/// `sandbox = false` or the session is `danger-full-access` (no wrap
/// either way). A plugin's own servers are skipped here: `plugin_mcp`
/// already wrapped them, refusing outright where the wrap is impossible
/// (T33.19). A user-configured server must not silently stop working on a
/// Landlock-only or backend-less host, so it keeps running unwrapped
/// there; one notice names every server that could not be sandboxed,
/// not one per server.
fn sandbox_stdio_servers(
    found: &mut cox_mcp::discovery::Discovered,
    config: &Config,
    writable: &[PathBuf],
) {
    if config.sandbox.mode == cox_protocol::SandboxMode::DangerFullAccess {
        return;
    }
    let mut names: Vec<String> = found.servers.keys().cloned().collect();
    names.sort();
    let mut unwrapped = Vec::new();
    for name in names {
        let is_plugin = found
            .sources
            .get(&name)
            .is_some_and(|s| s.starts_with("plugin:"));
        let Some(cfg) = found.servers.get(&name) else {
            continue;
        };
        if is_plugin || !cfg.sandbox {
            continue;
        }
        let Some(command) = cfg.command.clone() else {
            continue; // an HTTP server has nothing to wrap
        };
        let args = cfg.args.clone();
        match sandboxed_argv(Path::new(&command), &args, config, writable) {
            Ok(mut argv) => {
                if let Some(cfg) = found.servers.get_mut(&name) {
                    cfg.command = Some(argv.remove(0));
                    cfg.args = argv;
                }
            }
            Err(_) => unwrapped.push(name),
        }
    }
    if !unwrapped.is_empty() {
        found.notices.push(format!(
            "mcp: this host's sandbox cannot wrap a stdio server's argv; running unsandboxed: {}",
            unwrapped.join(", ")
        ));
    }
}

/// T33.8 (PL§3): the `NeedsApproval` plugins from the same discovery walk
/// `plugin_notices` performs, shaped for `Modal::PluginGrant` instead of a
/// warning line — the TUI is the only surface with somewhere interactive to
/// put the choice (`run_tui`, below); headless and ACP still get the text
/// notice `plugin_notices` already emits, unchanged. A store read error,
/// like `plugin_notices`, counts as no grant: it must never let a dialog
/// offer to widen a grant it cannot actually confirm.
#[cfg(feature = "plugins")]
fn plugin_grant_requests(
    config: &Config,
    home: &Path,
    cwd: &Path,
    store: &dyn cox_protocol::PluginStore,
) -> Vec<cox_tui::modal::PluginGrantDialog> {
    use cox_plugin::discover::{self, State};
    use cox_plugin::grant::{self, Verdict};

    if !config.plugins.enabled {
        return Vec::new();
    }
    let root = config_load::find_git_root(cwd);
    let found = discover::discover(home, root.as_deref());
    let mut out = Vec::new();
    for p in &found.plugins {
        let State::Loaded { manifest, digest } = &p.state else {
            continue;
        };
        // A project plugin discovered with no git root has nowhere to
        // write a grant, so it stays a `plugin_notices` text warning only.
        let Some(scope) = grant::scope(p.source, root.as_deref()) else {
            continue;
        };
        let stored = store.grant_get(&p.id, &scope, digest).ok().flatten();
        let Verdict::NeedsApproval { added, removed } =
            grant::check(manifest, digest, stored.as_ref())
        else {
            continue;
        };
        let repo = match &scope {
            GrantScope::Project(root) => Some(root.display().to_string()),
            GrantScope::User => None,
        };
        out.push(cox_tui::modal::PluginGrantDialog::new(
            p.id.clone(),
            digest.clone(),
            scope,
            manifest.name.clone(),
            manifest.description.clone(),
            grant::capability_list(manifest),
            added,
            removed,
            repo,
        ));
    }
    out
}

/// The slim build never discovers a plugin, so there is never a dialog to
/// queue.
#[cfg(not(feature = "plugins"))]
fn plugin_grant_requests(
    _config: &Config,
    _home: &Path,
    _cwd: &Path,
    _store: &dyn cox_protocol::PluginStore,
) -> Vec<cox_tui::modal::PluginGrantDialog> {
    Vec::new()
}

/// Writes one plugin grant (T33.8): forwards to `plugin_cmd::write_grant`,
/// the single place a `PluginGrant` row is assembled — T33.7's `cox plugin
/// enable`/`install` write through the same function, so this module never
/// grows its own copy of that literal or of `cox_store::now_rfc3339`'s
/// timestamp. Feature-gated like `plugin_grant_requests`: the slim build
/// has no `plugin_cmd` module to forward to, and there is never a decision
/// to write in that build anyway.
#[cfg(feature = "plugins")]
fn write_plugin_grant(
    store: &Store,
    decision: cox_tui::state::GrantDecision,
) -> Result<(), cox_protocol::StoreError> {
    crate::plugin_cmd::write_grant(
        store,
        &decision.plugin_id,
        &decision.scope,
        &decision.digest,
        decision.capabilities,
        serde_json::json!({}),
    )
}

#[cfg(not(feature = "plugins"))]
fn write_plugin_grant(
    _store: &Store,
    _decision: cox_tui::state::GrantDecision,
) -> Result<(), cox_protocol::StoreError> {
    Ok(())
}

/// The model id an `lmstudio` session sends: the section's pin, else
/// `tiers.code.model` — `Router::pick`'s rule, so the id asked about is the
/// id chatted with.
pub(crate) fn lmstudio_model(config: &Config) -> &str {
    let pinned = &config.providers.lmstudio.model;
    if pinned.is_empty() {
        &config.tiers.code.model
    } else {
        pinned
    }
}

/// What LM Studio reported for the session's model, plus the key it was
/// asked with.
struct Served {
    model: cox_provider::lmstudio::Model,
    api_key: Option<String>,
}

/// T30.16: reads (and, with `load = true`, loads) the session's model on
/// LM Studio's native API. `None` unless `tiers.code` is `lmstudio` and no
/// test double (`COX_PROVIDER`) stands in for the server. An unreachable
/// server is an error here, before any turn: the first chat call would
/// fail the same way.
async fn lmstudio_served(config: &Config) -> anyhow::Result<Option<Served>> {
    let double = std::env::var_os("COX_PROVIDER").is_some_and(|v| !v.is_empty());
    if config.tiers.code.provider != "lmstudio" || double {
        return Ok(None);
    }
    let l = &config.providers.lmstudio;
    let transport = l.transport();
    let api_key = cox_provider::http::resolve_key(&transport.api_key_env, "lmstudio").ok();
    let client = cox_provider::lmstudio::LmStudio::new(&transport, api_key.clone())?;
    let context_length = (l.context_window > 0).then_some(l.context_window);
    let model = client
        .prepare(lmstudio_model(config), l.load, context_length)
        .await
        .map_err(|e| anyhow::anyhow!("LM Studio at {}: {e}", transport.base_url))?;
    Ok(Some(Served { model, api_key }))
}

/// `--worktree <name>` (T27.3): creates or reuses the worktree, then makes
/// the rest of the run see it as `--cwd <worktree>`; [`open`] adds the main
/// checkout as a read-only root after config loading. Returns the new cwd.
/// Runs before config is loaded because the
/// project config is read from the worktree like everything else.
pub fn enter_worktree(cli: &mut Cli, cwd: &Path) -> anyhow::Result<PathBuf> {
    let Some(name) = cli.worktree.clone() else {
        return Ok(cwd.to_path_buf());
    };
    let owner = format!("cox / pid {}", std::process::id());
    let rt = tokio::runtime::Runtime::new()?;
    let wt = rt.block_on(cox_tools::git::worktree_add(cwd, &name, &owner))?;
    cli.cwd = Some(wt.path.clone());
    Ok(wt.path)
}

/// One worktree-aware project identity for presence writes and polling.
async fn project_root(cwd: &Path) -> PathBuf {
    cox_tools::git::project_root(cwd)
        .await
        .unwrap_or_else(|_| config_load::find_git_root(cwd).unwrap_or_else(|| cwd.to_path_buf()))
}

fn add_read_root(config: &mut Config, root: &Path) {
    if !config.core.workspace_roots.iter().any(|r| r == root) {
        config.core.workspace_roots.push(root.to_path_buf());
    }
}

/// After `/quit` in a worktree session: a clean tree is offered for
/// removal on the terminal the TUI just gave back; a dirty one is kept and
/// said so. The branch always stays — merging is the user's action.
fn offer_worktree_removal(rt: &tokio::runtime::Runtime, path: &Path) {
    if rt.block_on(cox_tools::git::is_clean(path)) != Some(true) {
        eprintln!(
            "cox: worktree {} kept: it has uncommitted or untracked files",
            path.display()
        );
        return;
    }
    eprint!(
        "cox: worktree {} is clean; remove it? [y/N] ",
        path.display()
    );
    let mut answer = String::new();
    let _ = std::io::stdin().read_line(&mut answer);
    if !matches!(answer.trim(), "y" | "Y" | "yes") {
        eprintln!("cox: worktree {} kept", path.display());
        return;
    }
    match rt.block_on(cox_tools::git::worktree_remove(
        path,
        cox_tools::git::OWNER_PREFIX,
    )) {
        Ok(()) => eprintln!(
            "cox: worktree {} removed; its branch is kept for you to merge",
            path.display()
        ),
        Err(e) => eprintln!("cox: worktree {} kept: {e}", path.display()),
    }
}

/// The MCP servers in effect for `cwd`: config, `.mcp.json`, `~/.claude.json`.
pub fn mcp_servers(config: &Config, cwd: &Path) -> cox_mcp::discovery::Discovered {
    let project = config_load::find_git_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let home = config_load::home_dir();
    cox_mcp::discovery::discover(&config.mcp.servers, Some(&project), Some(&home))
}

/// T22.5: tokens live in the keyring; with a person present a 401 prints
/// the login URL and opens the browser, headless surfaces get a notice.
pub fn mcp_auth(interactive: bool) -> cox_mcp::client::Auth {
    cox_mcp::client::Auth {
        secrets: Arc::new(cox_mcp::auth::Keyring),
        prompt: interactive.then(|| {
            Arc::new(|url: &str| {
                eprintln!("cox: mcp login: open {url}");
                if !cox_mcp::auth::open_browser(url) {
                    eprintln!("cox: no browser found; open the URL by hand");
                }
            }) as cox_mcp::client::Prompt
        }),
    }
}

/// T7.6: every discovered MCP server's tools, connected on the runtime the
/// session will run on (the sessions live in the tools). A server that will
/// not start is a warning and no tools (D14).
/// `plugins` (T33.19) join discovery as its lowest-precedence source, and
/// every stdio server (theirs and the user's) is sandboxed (T33.42) before
/// `connect_all` spawns it.
async fn mcp_tools(
    config: &Config,
    cwd: &Path,
    interactive: bool,
    plugins: Vec<(String, Vec<(String, McpServerConfig)>)>,
    writable: &[PathBuf],
) -> Vec<Arc<dyn Tool>> {
    let mut found = mcp_servers(config, cwd);
    for (id, servers) in plugins {
        cox_mcp::discovery::add_plugin(&mut found, &id, servers);
    }
    sandbox_stdio_servers(&mut found, config, writable);
    let timeout = std::time::Duration::from_secs(u64::from(config.mcp.timeout_s));
    let (_clients, tools, notices) = cox_mcp::client::connect_all(
        &found.servers,
        timeout,
        config.mcp.deferred,
        &mcp_auth(interactive),
    )
    .await;
    for notice in found.notices.iter().chain(&notices) {
        eprintln!("cox: warning: {notice}");
    }
    tools
}

/// `/sessions` and `/resume` rows: this project's sessions, newest first,
/// forks and handoffs indented under their parent (T26.3). Row zero is the
/// project header from the one SQL aggregate in `Store::project_totals`
/// (T28.2). A store that will not open is an empty list, not a failed start.
fn project_sessions(home: &Path, cwd: &Path) -> Vec<(String, String)> {
    let project = config_load::find_git_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let now = crate::sessions::now_secs();
    let store = Store::open(home).ok();
    let Some(store) = store.as_ref() else {
        return Vec::new();
    };
    let mut rows: Vec<(String, String)> = Vec::new();
    let tree = store.sessions_tree(200).unwrap_or_default();
    let kept: Vec<&cox_store::queries::TreeRow> = tree
        .iter()
        .filter(|row| Path::new(&row.info.cwd).starts_with(&project))
        .collect();
    if !kept.is_empty() {
        let slug = cox_ext::memory::slug_for(cwd);
        let totals = store.project_totals(&slug).ok();
        let (sessions, cost) =
            totals.map_or((kept.len() as i64, 0.0), |t| (t.sessions, t.cost_usd));
        rows.push((
            String::new(),
            cox_tui::picker::project_header(&slug, sessions, cost),
        ));
    }
    rows.extend(kept.into_iter().map(|row| {
        let entry = cox_tui::picker::session_entry(
            row.depth,
            row.info.title.as_deref(),
            &row.info.cwd,
            &crate::sessions::age_of(&row.info.updated_at, now),
            row.info.cost_usd,
        );
        (row.info.id.clone(), entry)
    }));
    rows
}

/// `Ctrl+R`'s other-session rows (T25.8): the prompts typed in this
/// project's other sessions, newest first and each text once, as
/// `(picker row, full text)`. Best-effort like `project_sessions`.
fn project_prompts(home: &Path, cwd: &Path, me: SessionId) -> Vec<(String, String)> {
    let project = config_load::find_git_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let Ok(store) = Store::open(home) else {
        return Vec::new();
    };
    let now = crate::sessions::now_secs();
    let me = me.to_string();
    let ages: HashMap<String, String> = store
        .list_sessions(1000)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.id != me && Path::new(&s.cwd).starts_with(&project))
        .map(|s| (s.id, crate::sessions::age_of(&s.updated_at, now)))
        .collect();
    let mut seen = HashSet::new();
    store
        .user_prompts(5000)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| Some((ages.get(&p.session_id)?, p.text)))
        .filter(|(_, text)| seen.insert(text.clone()))
        .take(500)
        .map(|(age, text)| (cox_tui::picker::prompt_entry(age, &text), text))
        .collect()
}

/// `/fork` and `/handoff` (T26.3): a new session with `parent_id` whose own
/// rollout opens with `events`, so a later `--resume` of the child rebuilds
/// the same history from its file. The parent's rollout is only read.
fn seed_child(
    store: &Store,
    cwd: &Path,
    parent: SessionId,
    events: &[Event],
) -> anyhow::Result<(SessionId, History)> {
    let id = SessionId::new();
    store.session_create(&SessionRow {
        id,
        created_at: String::new(),
        cwd: cwd.to_path_buf(),
        project_slug: String::new(),
        title: None,
        parent_id: Some(parent),
        rollout_path: PathBuf::new(),
    })?;
    let started = Event::SessionStarted {
        session: id,
        config_digest: String::new(),
        cwd: cwd.to_path_buf(),
    };
    for ev in std::iter::once(&started).chain(events) {
        store.rollout_append(&id, ev)?;
    }
    Ok((id, History::from_events(events)))
}

/// `/fork [turn]`: the parent's events up to the end of main turn `turn`
/// (all of them for `None`), minus its `SessionStarted`, which names the
/// parent. A `Rewound` or `Compacted` inside the kept span replays as-is.
fn fork(
    home: &Path,
    cwd: &Path,
    parent: SessionId,
    turn: Option<u32>,
) -> anyhow::Result<(SessionId, History)> {
    let store = Store::open(home)?;
    let (events, _) = store.rollout_read_with_truncation(&parent)?;
    let kept: Vec<Event> = events
        .into_iter()
        .take_while(|ev| match (ev, turn) {
            (
                Event::TurnStarted {
                    seq,
                    job: Job::Main,
                    ..
                },
                Some(turn),
            ) => *seq <= turn,
            _ => true,
        })
        .filter(|ev| !matches!(ev, Event::SessionStarted { .. }))
        .collect();
    seed_child(&store, cwd, parent, &kept)
}

/// `/handoff <objective>`: the child's first history item is one `Summary`
/// (compaction's item kind) carrying the parent's summary and the
/// objective. A summariser that returned nothing still hands the objective
/// over, and says so in the item.
fn handoff(
    home: &Path,
    cwd: &Path,
    parent: SessionId,
    objective: &str,
    summary: Option<&str>,
) -> anyhow::Result<(SessionId, History)> {
    let summary = summary.unwrap_or("(no summary: the summariser returned nothing)");
    let item = ItemId::new();
    let events = [
        Event::ItemStarted {
            item,
            kind: ItemKind::Summary {
                text: format!(
                    "[Handoff from session {parent}]\n\n{summary}\n\nObjective: {objective}"
                ),
            },
        },
        Event::ItemDone { item },
    ];
    seed_child(&Store::open(home)?, cwd, parent, &events)
}

/// `--continue` / `--resume <id>` for the interactive surfaces (the TUI and
/// `--plain`, T29.1): the session to reopen and its rebuilt history.
pub(crate) fn resume_from_flags(
    cli: &Cli,
    home: &Path,
    cwd: &Path,
) -> anyhow::Result<Option<(SessionId, History)>> {
    if cli.r#continue {
        let id = Store::open(home)?.latest_session_for_cwd(cwd)?;
        let history = resume::from_home(home, &id.to_string())?;
        Ok(Some((id, history)))
    } else if let Some(id_str) = &cli.resume {
        let id: SessionId = id_str.parse()?;
        let history = resume::from_home(home, id_str)?;
        Ok(Some((id, history)))
    } else {
        Ok(None)
    }
}

/// `cox init [--force]` (T25.6): scaffold `AGENTS.md` headlessly over the
/// same core path the interactive `/init` drives, then print where it
/// landed. Exit 0 when the file was written, 1 when it was refused (an
/// existing `AGENTS.md` without `--force`) or denied.
pub fn run_init(cli: &Cli, cwd: &Path, force: bool) -> anyhow::Result<i32> {
    let rt = tokio::runtime::Runtime::new()?;
    let (session, _) = rt.block_on(open(cli, cwd, None, None, |_| {}, None, false, None))?;
    let mut events = session
        .events()
        .ok_or_else(|| anyhow::anyhow!("session events already taken"))?;
    let running = {
        let session = session.clone();
        rt.spawn(async move { session.run_init(force).await })
    };
    let mut written = false;
    let mut refused = false;
    while let Some(ev) = rt.block_on(events.recv()) {
        match &ev {
            Event::Notice { text, .. } if text.starts_with("wrote AGENTS.md") => {
                println!("{text}");
                written = true;
            }
            Event::Notice {
                level: Level::Warn,
                text,
            } if text.contains("already exists") => {
                println!("cox init: {text}");
                refused = true;
            }
            Event::ApprovalRequired { call, .. } => {
                let session = session.clone();
                let call_id = call.id;
                rt.block_on(session.submit(Submission::Approve {
                    call_id,
                    decision: cox_protocol::types::Decision::Allow,
                }))?;
            }
            _ => {}
        }
        if written || refused {
            break;
        }
    }
    drop(running);
    Ok(i32::from(!written))
}

/// Runs the interactive TUI until the user quits.
pub fn run_tui(cli: &Cli, cwd: &Path) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let home = cli.home.clone().unwrap_or_else(config_load::cox_home);
    let project = rt.block_on(project_root(cwd));
    let themes_dir = home.join("themes");
    // T24.2: a `/theme` picker choice reaches here as `(key, value)`
    // because only `crates/cox` owns `config_cmd::set`; a write failing
    // (a full disk, a bad permission) is not fatal, just not persisted —
    // the picker already applied the theme to `state` either way.
    let (persist_tx, mut persist_rx) = tokio::sync::mpsc::channel::<(String, String)>(4);
    rt.spawn(async move {
        while let Some((key, value)) = persist_rx.recv().await {
            let _ = config_cmd::set(&key, &value);
        }
    });
    let mut resume_spec = resume_from_flags(cli, &home, cwd)?;
    let mut first = true;
    // What `/fork`/`/handoff` did, shown atop the next session's transcript.
    let mut announce: Option<(Level, String)> = None;
    loop {
        let seed = resume_spec.as_ref().map(|(_, history)| history.clone());
        // T22.1: the TUI is the only surface with somewhere to show a
        // question, so it is the only `open` caller that passes one.
        let (question_tx, mut question_rx) = tokio::sync::mpsc::channel::<AskUserQuestion>(1);
        // T33.23/T33.44: made before `open`, which hands the feed and the
        // `Cmd::Plugin` requests to the live plugins it starts.
        let (feed, feed_rx) = tokio::sync::mpsc::channel(4);
        let (plugin_tx, plugin_rx) = tokio::sync::mpsc::channel(16);
        let plugin_ui = PluginUi {
            feed: feed.clone(),
            requests: plugin_rx,
        };
        let (session, loaded) = rt.block_on(open(
            cli,
            cwd,
            None,
            Some(question_tx),
            |_| {},
            resume_spec.take(),
            true,
            Some(plugin_ui),
        ))?;
        let config = &loaded.config;
        let mut state = State::new(config.permissions.mode, config.sandbox.mode);
        // T28.1: the status line names the spend over the session cap.
        state.status.budget_cap_usd = config.budget.session_usd;
        state.status.budget_warn_at = config.budget.warn_at;
        // T22.2: markdown commands from `.claude/commands`/`.cox/commands`
        // join the `/` palette after the built-ins; a broken file is a
        // warning and skipped (D14).
        let cmds = cox_ext::commands::discover(&cox_ext::commands::command_dirs(
            Some(&home),
            Some(&config_load::home_dir().join(".claude")),
            Some(&project),
        ));
        for notice in &cmds.notices {
            eprintln!("cox: warning: {notice}");
        }
        state.commands.extend(cmds.commands.iter().map(|c| {
            let usage = match &c.argument_hint {
                Some(hint) => format!("/{} <{hint}>", c.name),
                None => format!("/{}", c.name),
            };
            (
                c.name.clone(),
                usage,
                c.description.clone().unwrap_or_default(),
            )
        }));
        // T25.5: rebound keys drive dispatch, hints, `?` and `/help` alike.
        let keys = config_load::keymap(&home, &config_load::home_dir().join(".claude"));
        for text in keys.warnings {
            state.transcript.push(cox_tui::state::Cell::Notice {
                level: cox_protocol::types::Level::Warn,
                text,
            });
        }
        for skipped in &keys.skipped {
            tracing::debug!("{skipped}");
        }
        state.keymap = keys.keymap;
        if let Some(history) = seed {
            state.transcript_from_history(&history);
        }
        if let Some((level, text)) = announce.take() {
            state
                .transcript
                .push(cox_tui::state::Cell::Notice { level, text });
        }
        state.files = cox_tools::glob::workspace_files(cwd);
        state.cwd = cwd.to_path_buf();
        state.git_branches = rt.block_on(cox_tools::git::branches(cwd));
        state.worktree = cli.worktree.clone();
        state.sessions = project_sessions(&home, cwd);
        state.past_prompts = project_prompts(&home, cwd, session.id());
        state.composer.set_vim(config.tui.vim);
        // T22.6/T24.2: `"auto"` and a named theme both query the terminal's
        // OSC 11 background once, before raw mode; `"light"`/`"dark"` are
        // an explicit choice and skip it, same as before T24.2 existed.
        let needs_background = !matches!(config.tui.theme.as_str(), "light" | "dark");
        let background_dark = needs_background
            .then(|| cox_tui::color::detect_dark(cox_tui::color::OSC11_TIMEOUT))
            .flatten();
        let catalog = cox_tui::theme::catalog(&themes_dir);
        let resolved = cox_tui::theme::resolve(&config.tui.theme, background_dark, &catalog);
        state.dark = resolved.dark;
        state.glyphs = cox_tui::glyph::resolve(&config.tui);
        state.depth = cox_tui::color::resolve(&config.tui);
        // T23.1: the env heuristic only, not `query()` — a live `CSI ?u`
        // round trip needs exclusive use of stdin for its reply, and
        // `app.rs`'s own input thread starts reading it moments later; the
        // two racing is exactly how `doctor` (which owns the terminal
        // outright and prints the query's own verdict) gets away with
        // `query()` and an interactive session should not risk it. A
        // heuristic miss is what `[tui.caps]` (surfaced by `doctor`) is for.
        let env_fn = |key: &str| std::env::var(key).ok();
        let mut caps = cox_tui::term::Caps::detect(&env_fn);
        caps.apply(&config.tui.caps);
        state.caps = caps;
        // `NO_COLOR` (T24.1) wins over whatever `tui.theme` picked: every
        // token resets so only `Modifier::BOLD`/`DIM` carry hierarchy.
        state.theme = match state.depth {
            cox_tui::color::Depth::None => cox_tui::theme::Theme::mono(),
            _ => resolved.theme,
        };
        if let Some(warning) = resolved.warning {
            state.transcript.push(cox_tui::state::Cell::Notice {
                level: cox_protocol::types::Level::Warn,
                text: warning,
            });
        }
        // T24.2 step 4: `.tmTheme` files merge into the bundled syntect set
        // once per process, like the theme name leak below.
        cox_tui::markdown::load_user_themes(&themes_dir);
        let syntax_names: Vec<&'static str> = cox_tui::theme::tm_theme_names(&themes_dir)
            .into_iter()
            .map(|name| -> &'static str { String::leak(name) })
            .collect();
        // The theme name outlives every render; one leak per process buys a
        // `Copy` `Look` instead of a clone on each line. A theme file's own
        // `syntax` wins over `tui.syntax_theme` when it names one.
        let syntax_theme_cfg = resolved
            .syntax
            .unwrap_or_else(|| config.tui.syntax_theme.clone());
        state.syntax_theme = String::leak(syntax_theme_cfg);
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
        // `/theme` (T24.2 step 3): built-ins and user files, then every
        // `.tmTheme` under a `syntax: ` row.
        state.theme_rows = catalog
            .iter()
            .map(|(name, _)| name.clone())
            .chain(syntax_names.iter().map(|name| format!("syntax: {name}")))
            .collect();
        state.theme_catalog = catalog;
        state.syntax_names = syntax_names;
        state.show_thinking = config.tui.show_thinking == "full";
        state.diff_mode = cox_tui::diff::Mode::parse(&config.tui.diff);
        state.still = config.tui.motion == "reduced";
        state.notify = cox_tui::state::Notify::parse(&config.tui.notify);
        // T22.4: the only switch for mouse capture is this config key.
        state.mouse = config.tui.mouse;
        state.marks = cli.verbose > 0;
        // T33.8, PL§3: one `Modal::PluginGrant` per `NeedsApproval` plugin,
        // queued in `pending_grants` since the TUI has one modal slot. A
        // fresh `Store::open` here, like the poll task's own reads below —
        // `open()` already moved its store into `session`. A read error
        // (a locked or missing file) leaves the queue empty rather than
        // failing the whole session open (D14: fail open on extensions).
        let mut pending_grants: VecDeque<_> = Store::open(&home)
            .map(|gs| plugin_grant_requests(config, &home, cwd, &gs))
            .unwrap_or_default()
            .into();
        state.modal = pending_grants
            .pop_front()
            .map(cox_tui::state::Modal::PluginGrant);
        state.pending_grants = pending_grants;
        let (ask, mut ask_rx) = tokio::sync::mpsc::channel(1);
        let (surfaced, surfaced_rx) = tokio::sync::mpsc::channel(1);
        let (grant_tx, mut grant_rx) =
            tokio::sync::mpsc::channel::<cox_tui::state::GrantDecision>(4);
        // The poller lives here, not in cox-tui: the TUI never touches the disk.
        let poll = {
            let home = home.clone();
            let project = project.clone();
            let me = session.id();
            let git = config.tui.git;
            let dir = cwd.to_path_buf();
            rt.spawn(async move {
                let mut every = tokio::time::interval(std::time::Duration::from_secs(2));
                loop {
                    tokio::select! {
                        _ = every.tick() => {
                            let now = cox_ext::presence::now_secs();
                            let agents = cox_ext::presence::others(&home, &project, &me, now);
                            if feed.send(Msg::Agents(agents)).await.is_err() {
                                break;
                            }
                            if !git {
                                continue;
                            }
                            // T15.2: the same poll carries the branch and counts.
                            let status = cox_tools::git::status(&dir).await.map(|s| GitStatus {
                                branch: s.branch,
                                added: s.added,
                                removed: s.removed,
                            });
                            if feed.send(Msg::Git(status)).await.is_err() {
                                break;
                            }
                        }
                        // T15.3: `Ctrl+G` asks for the diff; the answer rides the feed.
                        ask = ask_rx.recv() => match ask {
                            Some(Ask::GitDiff) => {
                                let diff = cox_tools::git::diff(&dir).await;
                                if feed.send(Msg::Diff(diff)).await.is_err() {
                                    break;
                                }
                            }
                            // T27.5: `Enter` on an `/agents` sibling-session
                            // row asks for that session's rollout, the same
                            // read `crates/cox/src/resume.rs` does for
                            // `--resume`; a read error (store missing, id
                            // stale) answers empty rather than killing the
                            // poll loop the rest of `/agents` still needs.
                            Some(Ask::Rollout(id)) => {
                                let events = Store::open(&home)
                                    .and_then(|store| store.rollout_read(&id))
                                    .unwrap_or_default();
                                if feed.send(Msg::Rollout(events)).await.is_err() {
                                    break;
                                }
                            }
                            None => break,
                        },
                        // T22.1: `ask_user`'s surface; the reply sender rides
                        // along so `cox_tui::app::run` can answer it once the
                        // modal resolves the question.
                        Some(q) = question_rx.recv() => {
                            let forwarded = cox_tui::app::Question {
                                call: q.call,
                                question: q.question,
                                options: q.options,
                                reply: q.reply,
                                agent: q.source.and_then(|s| s.agent),
                            };
                            if surfaced.send(forwarded).await.is_err() {
                                break;
                            }
                        }
                        // T33.8: `Modal::PluginGrant`'s `y`, written here —
                        // the one place in this surface that opens the
                        // store, same reasoning as `Ask::Rollout` above.
                        // Best-effort: a write that fails (a locked or
                        // full store) leaves the plugin ungranted, same as
                        // any other `plugin_notices` warning.
                        Some(decision) = grant_rx.recv() => {
                            if let Ok(gs) = Store::open(&home) {
                                let _ = write_plugin_grant(&gs, decision);
                            }
                        }
                    }
                }
            })
        };
        if first {
            if let Some(prompt) = cli.prompt.clone().filter(|p| !p.is_empty()) {
                let starter = session.clone();
                rt.spawn(async move {
                    let _ = starter
                        .submit(Submission::UserTurn {
                            text: prompt,
                            attachments: vec![],
                            confirm_think: false,
                        })
                        .await;
                });
            }
            first = false;
        }
        let quit = session.clone();
        let outcome = rt.block_on(cox_tui::app::run(
            session,
            state,
            feed_rx,
            ask,
            surfaced_rx,
            persist_tx.clone(),
            grant_tx,
            plugin_tx,
        ))?;
        poll.abort();
        // `/handoff`'s summary is the parent's `compact` call, so it runs
        // while the parent still has its provider and ledger.
        let summary = match &outcome {
            cox_tui::app::TuiOutcome::Handoff { objective } => {
                rt.block_on(quit.handoff_summary(objective))
            }
            _ => None,
        };
        // The TUI never shut the core down, so `SessionEnd` hooks and the
        // presence record outlived the window (T16.2).
        rt.block_on(quit.submit(Submission::Shutdown))?;
        // T34.11: the same kill headless `run` does before it exits (see
        // its comment there): a detached `bash` still running would
        // otherwise outlive this session — and, at quit, cox itself as an
        // orphan. Every outcome leaves this session, so every one reaps it.
        quit.interrupt();
        rt.block_on(quit.wait_tasks_cleared(crate::run::SHELL_CANCEL_GRACE));
        let parent = quit.id();
        let (child, what) = match outcome {
            cox_tui::app::TuiOutcome::Clear => continue,
            cox_tui::app::TuiOutcome::Quit => break,
            cox_tui::app::TuiOutcome::Fork { turn } => {
                let at = turn.map_or_else(|| "the latest turn".into(), |t| format!("T{t}"));
                (fork(&home, cwd, parent, turn), format!("fork at {at}"))
            }
            cox_tui::app::TuiOutcome::Handoff { objective } => {
                let child = handoff(&home, cwd, parent, &objective, summary.as_deref());
                let what = match summary {
                    Some(_) => "handoff".to_string(),
                    None => "handoff (no summary: the summariser returned nothing)".into(),
                };
                (child, what)
            }
        };
        // Fail open: a child that cannot be built puts the user back in the
        // parent rather than ending the program.
        (resume_spec, announce) = match child {
            Ok((id, history)) => (
                Some((id, history)),
                Some((
                    Level::Info,
                    format!("{what}: session {id}, child of {parent}"),
                )),
            ),
            Err(e) => (
                resume::from_home(&home, &parent.to_string())
                    .ok()
                    .map(|history| (parent, history)),
                Some((
                    Level::Warn,
                    format!("{what} failed: {e}; still in {parent}"),
                )),
            ),
        };
    }
    if cli.worktree.is_some() {
        offer_worktree_removal(&rt, cwd);
    }
    // T34.11: `rt`'s own `Drop` waits for every blocking task, including a
    // shell `wait_tasks_cleared` gave up on, which hung quit for as long as
    // that shell ran; same reasoning as `run`'s `shutdown_background`.
    rt.shutdown_background();
    Ok(())
}

/// The `tiers.code` provider decides which real client to build; every tier
/// of a session goes through the same provider object (routing picks models).
/// Real clients are wrapped in `Priced` so every call reaches the ledger with
/// its cost; test doubles are not, because their scenarios script the cost.
pub(crate) fn provider_for(config: &Config) -> anyhow::Result<Arc<dyn Provider>> {
    provider_for_with(config, cox_provider::http::resolve_key)
}

/// [`provider_for`]'s body with the credential lookup injected, so a test
/// can build every provider kind without ever reaching the real keyring
/// (A49, T30.28).
fn provider_for_with(
    config: &Config,
    resolve: impl FnOnce(&str, &str) -> Result<String, cox_protocol::errors::ProviderError>,
) -> anyhow::Result<Arc<dyn Provider>> {
    provider_for_served(config, resolve, None, &[])
}

/// [`provider_for_with`] plus what a local server reported for the
/// session's model (T30.16), overlaid on the catalog before any lookup.
fn provider_for_served(
    config: &Config,
    resolve: impl FnOnce(&str, &str) -> Result<String, cox_protocol::errors::ProviderError>,
    served: Option<&cox_provider::lmstudio::Model>,
    plugin_models: &[cox_models::PluginModels<'_>],
) -> anyhow::Result<Arc<dyn Provider>> {
    if let Some(double) = cox_provider::from_env()? {
        return Ok(Arc::from(double));
    }
    let prices = Arc::new(PriceTable::embedded()?);
    Ok(Arc::new(Priced::new(
        backend_for_with(config, resolve, served, plugin_models)?,
        prices,
    )))
}

/// The real client `tiers.code.provider` names, before pricing: one lookup
/// from the section to (api shape, `&Transport`) to a constructor (T30.23),
/// instead of a bespoke arm per family. Anthropic and Jev stay their own
/// arms because they carry section-specific knobs no OpenAI-shaped section
/// has (cache TTL/fallbacks; a decision model); every `api = "chat"` or
/// `"responses"` section — native `openai`/`local` and any Type-2 compatible
/// section alike — goes through the one `openai_shaped` constructor.
///
/// Takes the credential lookup as `resolve` (A49, T30.28): every arm
/// resolves its key through it — `AnthropicProvider::with_key`/
/// `JevProvider::with_key` take the already-resolved key instead of
/// resolving it themselves, and `openai_shaped` takes `resolve` straight
/// through — so [`provider_for_with`]'s caller decides whether that is the
/// real `cox_provider::http::resolve_key` or a test's fake lookup.
fn backend_for_with(
    config: &Config,
    resolve: impl FnOnce(&str, &str) -> Result<String, cox_protocol::errors::ProviderError>,
    served: Option<&cox_provider::lmstudio::Model>,
    plugin_models: &[cox_models::PluginModels<'_>],
) -> anyhow::Result<Arc<dyn Provider>> {
    // T30.25: `Caps.max_context` for the sections below comes from the
    // model catalog rather than a per-family literal. `Catalog::load`
    // always carries the embedded built-in rows even when `config` overlays
    // none of its own (unlike reading `providers.*.models` directly, which
    // is empty on a bare `Config::default()`); a bad/unparseable catalog
    // falls back to the empty default, which is exactly "no row found" —
    // every lookup below already has its own literal fallback for that.
    // `plugin_models` are the granted plugins' `[[models]]` (T33.44).
    let mut catalog = cox_models::Catalog::load(config, plugin_models, None).unwrap_or_default();
    // T30.16 (A46): a server's own report is the last override layer.
    if let Some(m) = served {
        catalog.overlay_served(&m.key, m.loaded_context(), m.tool_use());
    }
    match config.tiers.code.provider.as_str() {
        "anthropic" => {
            let a = &config.providers.anthropic;
            let ttl = match a.cache_ttl.as_str() {
                "1h" => CacheTtl::OneHour,
                _ => CacheTtl::FiveMinutes,
            };
            let transport = a.transport();
            let api_key = resolve(&transport.api_key_env, "anthropic")?;
            // 200k, same as before T30.25, when the catalog has no row for
            // the tier's configured model (e.g. a custom id absent from
            // both the built-in and configured `models` lists).
            let max_context = catalog
                .get(&config.tiers.code.model)
                .and_then(|row| row.context_window)
                .unwrap_or(200_000);
            Ok(Arc::new(AnthropicProvider::with_key(
                &transport,
                Some(api_key),
                ttl,
                a.fallbacks,
                max_context,
            )?))
        }
        // T30.15: LM Studio's Anthropic-compatible `/v1/messages` goes
        // through the same Anthropic wire client as the "anthropic" arm
        // above — its native `/api/v1/chat` takes no custom tool schemas
        // and cox's OpenAI Chat path drops tool calls (R§4.3.2), so
        // Messages is the one working chat transport. The key resolves
        // under this section's own name ("lmstudio"), so it never falls
        // back to the Anthropic keyring entry; missing it builds keyless
        // (T30.21 shape), which is what LM Studio needs unless "Require
        // Authentication" is on.
        "lmstudio" => {
            let l = &config.providers.lmstudio;
            let transport = l.transport();
            let api_key = resolve(&transport.api_key_env, "lmstudio").ok();
            // `context_window = 0` means "ask the server": its loaded
            // context, overlaid on the catalog above (T30.16), then the
            // catalog's own row, then the same literal floor `local` uses.
            let max_context = if l.context_window > 0 {
                l.context_window
            } else {
                catalog
                    .get(lmstudio_model(config))
                    .and_then(|row| row.context_window)
                    .unwrap_or(32_768)
            };
            Ok(Arc::new(AnthropicProvider::with_key(
                &transport,
                api_key,
                CacheTtl::FiveMinutes,
                // The `fallbacks: "default"` beta is Anthropic's own
                // server-side model fallback; meaningless against LM
                // Studio, so this arm never sends it.
                false,
                max_context,
            )?))
        }
        // Jev is type-1 native (System One wire, T21.1): its own client,
        // not an OpenAI shape. A missing key fails `Auth` rather than
        // building keyless — that is the fail-open path, read as auth, not
        // transport.
        "typesafe" => {
            let t = &config.providers.typesafe;
            let transport = t.transport();
            let api_key = resolve(&transport.api_key_env, "typesafe")?;
            // 128k, same as before T30.25, when the catalog has no row —
            // expected, since Jev/TypeSafe models have no models.dev
            // counterpart (`cox-vendor models` never touches this section).
            let max_context = catalog
                .get(&t.model)
                .and_then(|row| row.context_window)
                .unwrap_or(128_000);
            Ok(Arc::new(cox_provider::jev::JevProvider::with_key(
                &transport,
                api_key,
                t.model.clone(),
                max_context,
            )?))
        }
        "openai" => {
            let o = &config.providers.openai;
            // No `context_window` field on the native section (it relies
            // on `models`); 400k is the same fallback `openai_shaped` used
            // before this lookup existed, now reached only when the
            // catalog has no row for the tier's configured model either.
            let max_context = catalog
                .get(&config.tiers.code.model)
                .and_then(|row| row.context_window)
                .unwrap_or(400_000);
            openai_shaped(
                "openai",
                &o.transport(),
                o.models.clone(),
                max_context,
                &o.api,
                resolve,
            )
        }
        "local" => {
            let l = &config.providers.local;
            openai_shaped(
                "local",
                &l.transport(),
                l.models.clone(),
                l.context_window,
                &l.api,
                resolve,
            )
        }
        // Type-2 providers: no code per vendor — the section's `api` picks
        // the wire client, the section's transport/key/models configure it.
        other => {
            let c = config
                .providers
                .custom
                .get(other)
                .ok_or_else(|| anyhow::anyhow!("unknown provider `{other}` in tiers.code"))?;
            openai_shaped(
                other,
                &c.transport(),
                c.models.clone(),
                c.context_window,
                &c.api,
                resolve,
            )
        }
    }
}

/// Builds the OpenAI-shaped client the `api` string names for `owner`:
/// `"responses"` speaks the Responses API, `"chat"` the Chat Completions
/// subset every compatible vendor speaks. Anything else is a config error
/// at startup, not a mid-turn 404. The key resolves once, here, through
/// `resolve` — `transport.api_key_env` first, else the keyring entry
/// `cox/<owner>` for the real caller (`backend_for`); missing both builds
/// keyless (no `Authorization` header) rather than failing at startup
/// (T30.21) — most compatible sections, and every local/self-hosted
/// gateway, need no key at all.
fn openai_shaped(
    owner: &str,
    transport: &Transport,
    models: Vec<cox_protocol::config::ProviderModel>,
    context_window: u32,
    api: &str,
    resolve: impl FnOnce(&str, &str) -> Result<String, cox_protocol::errors::ProviderError>,
) -> anyhow::Result<Arc<dyn Provider>> {
    let api_key = resolve(&transport.api_key_env, owner).ok();
    match api {
        "responses" => Ok(Arc::new(OpenAiResponsesProvider::new(
            transport,
            api_key,
            models,
            context_window,
        )?)),
        "chat" => Ok(Arc::new(OpenAiChatProvider::new(
            transport,
            api_key,
            models,
            context_window,
        )?)),
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
        // Headless/ACP/MCP: `--answer` or nothing. `open` swaps this for
        // `Answers::Surface` when a caller passes `questions` (T22.1).
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
/// Swaps the fixed-answer `ask_user` `tools()` built for one that surfaces
/// each question instead (T22.1): only `run_tui` has somewhere to show it.
/// Same swap-by-name shape as `with_client_tools`; the spec is unchanged
/// (`AskUserTool::spec` never reads `answers`), so `tool_search`'s cached
/// schema, built from the pre-swap tools, still matches.
fn with_question_surface(
    tools: Vec<Arc<dyn Tool>>,
    tx: tokio::sync::mpsc::Sender<AskUserQuestion>,
) -> Vec<Arc<dyn Tool>> {
    tools
        .into_iter()
        .map(|t| match t.spec().name.as_str() {
            "ask_user" => Arc::new(AskUserTool::new(Answers::Surface(tx.clone()))) as Arc<dyn Tool>,
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
    use clap::Parser as _;
    use cox_protocol::config::CompatibleProviderConfig;
    use cox_protocol::types::ProviderId;

    use super::*;

    /// T27.3: `--worktree t9` from a repository puts the session in
    /// `_worktrees/<repo>-t9` with the main checkout as its second root.
    #[test]
    fn worktree_flag_sets_roots() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path().join("repo");
        std::fs::create_dir(&repo).expect("mkdir");
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .current_dir(&repo)
                .args(args)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        };
        if !git(&["init", "-q", "--initial-branch=trunk"]) {
            return; // no usable git here
        }
        std::fs::write(repo.join("a.txt"), "a\n").expect("write");
        assert!(git(&["add", "a.txt"]));
        assert!(git(&[
            "-c",
            "user.email=t@example.invalid",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "first"
        ]));
        let repo = std::fs::canonicalize(&repo).expect("canon");
        let mut cli = Cli::parse_from(["cox", "--worktree", "T9"]);
        let cwd = enter_worktree(&mut cli, &repo).expect("enter");
        let root = std::fs::canonicalize(tmp.path()).expect("canon");
        assert_eq!(cwd, root.join("_worktrees").join("repo-t9"));
        assert!(cwd.join("a.txt").is_file(), "the worktree is checked out");
        let mut loaded = config_load::load(&cwd, &cli).expect("load");
        assert_eq!(loaded.config.core.workspace_roots, vec![cwd.clone()]);
        assert!(cli.add_dir.is_empty(), "the main checkout is not writable");
        let project = rt_project_root(&cwd);
        assert_eq!(project, repo);
        add_read_root(&mut loaded.config, &project);
        assert_eq!(loaded.config.core.workspace_roots, vec![cwd.clone(), repo]);
        assert_eq!(cli.cwd.as_deref(), Some(cwd.as_path()));
    }

    fn rt_project_root(cwd: &Path) -> PathBuf {
        tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(project_root(cwd))
    }

    fn deepseek_config(api: &str) -> Config {
        let mut cfg = Config::default();
        cfg.tiers.code.provider = "deepseek".into();
        cfg.providers.custom.insert(
            "deepseek".into(),
            CompatibleProviderConfig {
                base_url: "https://api.deepseek.com".into(),
                // Never resolved in a test (A49, T30.28): every caller below
                // goes through `provider_for_with` with a fake resolver, so
                // this name is never looked up anywhere, real or fake.
                api_key_env: "COX_TEST_MISSING_KEY_DEEPSEEK".into(),
                api: api.into(),
                model: "deepseek-v4-pro".into(),
                context_window: 1_000_000,
                models: vec![],
                ..Default::default()
            },
        );
        cfg
    }

    /// A49 (T30.28): every test below that needs a "no key" outcome uses
    /// this instead of an unset env var — `resolve_key`'s keyring fallback
    /// is the real platform store, and a test must never reach it.
    fn no_key(_: &str, _: &str) -> Result<String, cox_protocol::errors::ProviderError> {
        Err(cox_protocol::errors::ProviderError::Auth)
    }

    #[test]
    fn provider_for_custom_builds_chat_client_without_a_key() {
        let p = provider_for_with(&deepseek_config("chat"), no_key).expect("builds");
        assert_eq!(p.id(), ProviderId::Local);
        assert_eq!(p.capabilities().max_context, 1_000_000);
    }

    #[test]
    fn provider_for_custom_responses_builds_responses_client() {
        let p = provider_for_with(&deepseek_config("responses"), no_key).expect("builds");
        assert_eq!(p.id(), ProviderId::OpenAi);
    }

    #[test]
    fn provider_for_rejects_unknown_names_and_shapes() {
        let mut bad = Config::default();
        bad.tiers.code.provider = "weird".into();
        assert!(provider_for(&bad).is_err(), "unknown name bails");
        assert!(
            provider_for_with(&deepseek_config("smoke-signals"), no_key).is_err(),
            "unknown api bails at startup, not mid-turn"
        );
    }

    /// T30.23: `backend_for`'s one lookup still builds the right provider
    /// kind for every section, native and compatible alike (the deepseek
    /// cases above cover the "custom section" leg of the same claim). A49
    /// (T30.28): a fake resolver, not an env var, keeps every branch off
    /// the real keyring.
    #[test]
    fn backend_for_builds_the_right_provider_kind_per_section() {
        fn fake_key(_: &str, _: &str) -> Result<String, cox_protocol::errors::ProviderError> {
            Ok("sk-test".to_string())
        }

        let anthropic = Config::default();
        let p =
            provider_for_with(&anthropic, fake_key).expect("anthropic builds with a resolved key");
        assert_eq!(p.id(), ProviderId::Anthropic);

        let mut openai = Config::default();
        openai.tiers.code.provider = "openai".into();
        let p = provider_for_with(&openai, fake_key).expect("openai builds through openai_shaped");
        assert_eq!(p.id(), ProviderId::OpenAi);

        let mut local = Config::default();
        local.tiers.code.provider = "local".into();
        let p = provider_for_with(&local, fake_key)
            .expect("local goes through the same openai_shaped path as any compatible section");
        assert_eq!(p.id(), ProviderId::Local);
    }

    /// T30.15: `--provider lmstudio` builds through the Anthropic wire
    /// client (R§4.3.2), keyed or keyless alike — the "no key" leg is what
    /// `openai_shaped` already proves for the other families; this proves
    /// the same `.ok()`-turns-missing-into-None shape holds for the
    /// Anthropic-wire arm too. `AnthropicProvider::id()` always reports
    /// `ProviderId::Anthropic` regardless of section (same "wire family,
    /// not vendor" bucketing `local`/compatible sections already use for
    /// `ProviderId::Local` — the model string disambiguates the ledger row).
    #[test]
    fn backend_for_lmstudio_builds_keyed_and_keyless() {
        let mut cfg = Config::default();
        cfg.tiers.code.provider = "lmstudio".into();
        // A model id no built-in catalog row lists, unlike the Anthropic
        // default `Config::default()` otherwise carries — realistic LM
        // Studio usage (`--tier code=<local model>`), and it exercises the
        // literal floor below rather than an accidental catalog hit.
        cfg.tiers.code.model = "prism-ml/bonsai-27b".into();

        let p = provider_for_with(&cfg, no_key).expect("builds keyless (no auth header)");
        assert_eq!(p.id(), ProviderId::Anthropic);

        fn fake_key(_: &str, _: &str) -> Result<String, cox_protocol::errors::ProviderError> {
            Ok("lm-test-token".to_string())
        }
        let p = provider_for_with(&cfg, fake_key).expect("builds with a resolved key");
        assert_eq!(p.id(), ProviderId::Anthropic);
        assert_eq!(
            p.capabilities().max_context,
            32_768,
            "the literal floor when context_window is 0, no server report was given and the catalog has no row for the configured model"
        );
    }

    /// T30.16: the context LM Studio reports as loaded is the session's
    /// window (what compaction fits), overriding the catalog and the floor;
    /// a configured `context_window` still wins over the server.
    #[test]
    fn lmstudio_window_follows_the_served_loaded_context() {
        let mut cfg = Config::default();
        cfg.tiers.code.provider = "lmstudio".into();
        cfg.tiers.code.model = "prism-ml/bonsai-27b".into();
        let raw = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/lmstudio/models.json"),
        )
        .expect("fixture");
        let list: cox_provider::lmstudio::ModelList =
            serde_json::from_str(&raw).expect("fixture parses");
        let served = list.find("prism-ml/bonsai-27b").expect("listed");

        let p = provider_for_served(&cfg, no_key, Some(served), &[]).expect("builds");
        assert_eq!(p.capabilities().max_context, 251_648);

        cfg.providers.lmstudio.context_window = 65_536;
        let p = provider_for_served(&cfg, no_key, Some(served), &[]).expect("builds");
        assert_eq!(p.capabilities().max_context, 65_536);
    }

    /// T30.25 check: a model configured with a 1M context window is
    /// reported as such, not the pre-T30.25 200k literal — the catalog
    /// (built from `config`, T30.24) is consulted for `tiers.code.model`.
    #[test]
    fn anthropic_capabilities_report_the_configured_models_context_window() {
        fn fake_key(_: &str, _: &str) -> Result<String, cox_protocol::errors::ProviderError> {
            Ok("sk-test".to_string())
        }
        let mut cfg = Config::default();
        cfg.tiers.code.model = "claude-big-1m".into();
        cfg.providers.anthropic.models = vec![cox_protocol::config::ProviderModel {
            id: "claude-big-1m".into(),
            context_window: 1_000_000,
            efforts: vec![],
            ..Default::default()
        }];
        let p = provider_for_with(&cfg, fake_key).expect("anthropic builds");
        assert_eq!(p.capabilities().max_context, 1_000_000);
    }

    /// An unconfigured/unknown model still gets the pre-T30.25 200k floor —
    /// the catalog lookup is additive, not a behaviour change when nothing
    /// overrides the model.
    #[test]
    fn anthropic_capabilities_fall_back_to_200k_for_an_unknown_model() {
        fn fake_key(_: &str, _: &str) -> Result<String, cox_protocol::errors::ProviderError> {
            Ok("sk-test".to_string())
        }
        let mut cfg = Config::default();
        cfg.tiers.code.model = "claude-totally-unlisted".into();
        let p = provider_for_with(&cfg, fake_key).expect("anthropic builds");
        assert_eq!(p.capabilities().max_context, 200_000);
    }

    fn scripted_session(home: &Path, work: &Path, scenario: &str) -> (Session, Arc<Store>) {
        let store = Arc::new(Store::open(home).expect("store"));
        let provider: Arc<dyn Provider> =
            Arc::new(cox_provider::scripted::Scripted::from_toml(scenario, "").expect("scenario"));
        let session = Session::new(
            Config::default(),
            provider,
            vec![],
            store.clone(),
            store.clone(),
            work.to_path_buf(),
        )
        .expect("session");
        (session, store)
    }

    async fn user_turn(session: &Session, text: &str) {
        session
            .submit(Submission::UserTurn {
                text: text.into(),
                attachments: vec![],
                confirm_think: false,
            })
            .await
            .expect("turn");
    }

    fn texts(history: &History) -> Vec<String> {
        history
            .messages
            .iter()
            .flat_map(|m| &m.content)
            .filter_map(|c| match c {
                cox_protocol::types::Content::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn depth_of(store: &Store, id: SessionId) -> Option<usize> {
        let tree = store.sessions_tree(50).expect("tree");
        tree.iter()
            .find(|r| r.info.id == id.to_string())
            .map(|r| r.depth)
    }

    /// T26.3: `/fork T1` after two turns starts a child of the session with
    /// only the first turn; the child's own rollout rebuilds the same
    /// history (a later `--resume`), and a bare `/fork` keeps every turn.
    #[tokio::test]
    async fn fork_creates_child_with_truncated_history() {
        let home = tempfile::tempdir().expect("home");
        let work = tempfile::tempdir().expect("work");
        let (session, store) = scripted_session(
            home.path(),
            work.path(),
            "[[turn]]\ntext = \"a1\"\n[[turn]]\ntext = \"a2\"\n",
        );
        user_turn(&session, "one").await;
        user_turn(&session, "two").await;
        let parent = session.id();

        let (child, history) = fork(home.path(), work.path(), parent, Some(1)).expect("fork");
        assert_eq!(texts(&history), ["one", "a1"]);
        assert_eq!(history.turns, 1, "the child keeps counting from T1");
        let resumed = resume::from_home(home.path(), &child.to_string()).expect("resume");
        assert_eq!(resumed.messages, history.messages);
        assert_eq!(depth_of(&store, parent), Some(0));
        assert_eq!(depth_of(&store, child), Some(1));

        let (_, all) = fork(home.path(), work.path(), parent, None).expect("fork all");
        assert_eq!(texts(&all), ["one", "a1", "two", "a2"]);
    }

    /// T26.3: `/handoff` asks the parent's `compact` job (cheap tier, in the
    /// ledger) for a summary; the child's first and only history item is
    /// that summary plus the objective, as a `Summary` item, not a turn.
    #[tokio::test]
    async fn handoff_seeds_summary() {
        let home = tempfile::tempdir().expect("home");
        let work = tempfile::tempdir().expect("work");
        let (session, store) = scripted_session(
            home.path(),
            work.path(),
            "[[turn]]\ntext = \"a1\"\n[[turn]]\ntext = \"we said hello\"\n",
        );
        user_turn(&session, "hello").await;
        let parent = session.id();

        let summary = session.handoff_summary("ship it").await;
        assert_eq!(summary.as_deref(), Some("we said hello"));
        let usage = store.usage_for_session(&parent).expect("usage");
        assert!(
            usage
                .iter()
                .any(|u| u.job == Job::Compact && u.tier == cox_protocol::types::Tier::Cheap),
            "the summary is a cheap-tier compact call: {usage:?}"
        );
        assert_eq!(session.history().await.len(), 2, "the parent is untouched");

        let (child, history) = handoff(
            home.path(),
            work.path(),
            parent,
            "ship it",
            summary.as_deref(),
        )
        .expect("handoff");
        let [text] = texts(&history).try_into().expect("one seed item");
        assert!(text.contains("we said hello") && text.ends_with("Objective: ship it"));
        assert!(history.turn_marks.is_empty(), "the seed is not a user turn");
        let (events, _) = store.rollout_read_with_truncation(&child).expect("rollout");
        assert!(matches!(
            events.get(1),
            Some(Event::ItemStarted {
                kind: ItemKind::Summary { .. },
                ..
            })
        ));
        assert_eq!(depth_of(&store, child), Some(1));
    }

    struct NoopArchive;

    #[async_trait::async_trait]
    impl cox_protocol::Archive for NoopArchive {
        async fn put(
            &self,
            _put: cox_protocol::ArchivePut,
        ) -> Result<cox_protocol::ArchiveId, cox_protocol::StoreError> {
            Ok(cox_protocol::ArchiveId::new())
        }
        async fn get(
            &self,
            _id: &cox_protocol::ArchiveId,
        ) -> Result<Vec<u8>, cox_protocol::StoreError> {
            Ok(Vec::new())
        }
    }

    /// T22.1: `run_tui`'s `open` swaps `tools()`'s fixed-answer `ask_user`
    /// for one whose answers surface on the channel it is given, so a
    /// question the tool asks reaches whoever is listening on `tx` and the
    /// reply they send back is what the call returns.
    #[tokio::test]
    async fn tui_question_surface_is_wired() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(Store::open(tmp.path()).expect("open store"));
        let mdir = tmp.path().join("memory");
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let built = with_question_surface(tools(None, &store, mdir), tx);
        let ask_user = built
            .iter()
            .find(|t| t.spec().name == "ask_user")
            .expect("ask_user tool present")
            .clone();

        let (out_tx, _out_rx) = tokio::sync::mpsc::channel(1);
        let cx = cox_tools::tool_cx(
            vec![tmp.path().to_path_buf()],
            tmp.path().to_path_buf(),
            cox_protocol::SandboxPolicy {
                mode: cox_protocol::types::SandboxMode::ReadOnly,
                network: false,
                writable: vec![],
                readonly_in_workspace: vec![],
                linux_backend: Default::default(),
            },
            Arc::new(NoopArchive) as Arc<dyn cox_protocol::Archive>,
            tokio_util::sync::CancellationToken::new(),
            out_tx,
            SessionId::new(),
            cox_protocol::ids::CallId::new(),
        );

        let surface = tokio::spawn(async move {
            let q = rx
                .recv()
                .await
                .expect("question surfaced on the swapped channel");
            assert_eq!(q.question, "pick one");
            let _ = q.reply.send("b".into());
        });
        let out = ask_user
            .call(
                serde_json::json!({"question": "pick one", "options": ["a", "b"]}),
                &cx,
            )
            .await
            .expect("answered through the surface");
        assert_eq!(out.text, "b");
        surface.await.expect("surface task");
    }

    #[cfg(feature = "plugins")]
    fn manifest_with_mcp(mcp: serde_json::Value, net: &[&str]) -> cox_plugin_api::PluginManifest {
        serde_json::from_value(serde_json::json!({
            "api": 1, "id": "gh", "version": "0.1.0", "name": "gh", "wasm": "plugin.wasm",
            "capabilities": { "net": net },
            "mcp": [mcp],
        }))
        .expect("manifest")
    }

    #[cfg(feature = "plugins")]
    #[test]
    fn changing_bundled_server_binary_changes_digest() {
        let pkg = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(pkg.path().join("bin")).expect("mkdir");
        std::fs::write(pkg.path().join("plugin.wasm"), b"\0asm").expect("wasm");
        std::fs::write(pkg.path().join("bin/server"), b"v1").expect("server");
        let before = cox_plugin::package_digest(pkg.path()).expect("digest");
        std::fs::write(pkg.path().join("bin/server"), b"v2").expect("server");
        let after = cox_plugin::package_digest(pkg.path()).expect("digest");
        assert_ne!(
            before, after,
            "a changed server binary must need a new grant"
        );
    }

    /// A package with one `[[external_agents]]` entry whose in-package
    /// program is `script`.
    #[cfg(all(feature = "plugins", unix))]
    fn agent_package(pkg: &Path, args: &[String], script: &str) -> cox_plugin_api::PluginManifest {
        use std::os::unix::fs::PermissionsExt as _;

        std::fs::create_dir_all(pkg.join("bin")).expect("mkdir");
        std::fs::write(pkg.join("bin/agent"), script).expect("agent");
        std::fs::set_permissions(
            pkg.join("bin/agent"),
            std::fs::Permissions::from_mode(0o755),
        )
        .expect("chmod");
        std::fs::write(
            pkg.join("plugin.wasm"),
            r#"(module (func (export "cox_init") (result i32) (i32.const 0)))"#,
        )
        .expect("wasm");
        let quoted: Vec<String> = args.iter().map(|a| format!("{a:?}")).collect();
        let toml = format!(
            "api = 1\nid = \"cur\"\nversion = \"0.1.0\"\nname = \"Cur\"\nwasm = \"plugin.wasm\"\n\n\
             [[external_agents]]\nname = \"cursor\"\ncommand = \"bin/agent\"\nargs = [{}]\n\
             mode = \"acp\"\nkey_env = \"CURSOR_API_KEY\"\n",
            quoted.join(", ")
        );
        std::fs::write(pkg.join("plugin.toml"), &toml).expect("manifest");
        cox_plugin::discover::load_manifest(pkg, &pkg.join("plugin.toml"), Some("cur"))
            .expect("valid manifest")
            .0
    }

    /// T35.2 Check (EA§2, PL§7c): the command a driver gets runs the
    /// sandbox launcher, not the agent, so the agent is under the same
    /// Seatbelt or bwrap profile as `bash`: a write inside the workspace
    /// lands, one under `$HOME` is denied. Where no argv backend exists the
    /// entry is refused with a warning, never run bare.
    #[cfg(all(feature = "plugins", unix))]
    #[test]
    fn external_agent_command_is_wrapped_by_sandbox_before_spawn() {
        use cox_tools::sandbox::{Backend, backend};

        let pkg = tempfile::tempdir().expect("tempdir");
        let ws = tempfile::tempdir().expect("tempdir");
        let home = std::env::var("HOME").expect("HOME");
        let outside = format!("{home}/.cox-agent-escape-{}", std::process::id());
        let args = [ws.path().display().to_string(), outside.clone()];
        let script = "#!/bin/sh\necho in > \"$1/inside\"\necho x > \"$2\"\n";
        let manifest = agent_package(pkg.path(), &args, script);
        let mut config = Config::default();
        config.core.workspace_roots = vec![ws.path().to_path_buf()];
        let roots = config.core.workspace_roots.clone();
        let mut notices = Vec::new();
        let agents = plugin_agents("cur", pkg.path(), &manifest, &config, &roots, &mut notices);

        if !matches!(
            backend(cox_protocol::LinuxBackend::Auto),
            Some(Backend::Seatbelt | Backend::Bwrap)
        ) {
            assert!(agents.is_empty(), "ran without a sandbox");
            assert!(
                notices[0].contains("external agent cursor skipped"),
                "{notices:?}"
            );
            return;
        }
        assert!(notices.is_empty(), "{notices:?}");
        let program = pkg.path().join("bin/agent");
        let cmd = agents[0].command();
        assert_ne!(cmd.get_program(), program.as_os_str(), "spawned bare");
        assert!(cmd.get_args().any(|a| a == program.as_os_str()));
        let _ = agents[0].command().status().expect("spawns");
        let leaked = Path::new(&outside).exists();
        let _ = std::fs::remove_file(&outside);
        assert!(ws.path().join("inside").exists(), "the agent never ran");
        assert!(!leaked, "the sandbox let an external agent write {outside}");
    }

    /// T35.2 Check (matches T33.6's `headless_never_loads_ungranted_plugin`):
    /// an ungranted plugin's external agent is neither resolved nor run;
    /// the notice names its approval line. Granting the same bytes is what
    /// lets it through, so the refusal is the grant's.
    #[cfg(all(feature = "plugins", unix))]
    #[test]
    fn ungranted_external_agent_is_not_spawned() {
        use cox_protocol::{PluginGrant, PluginStore as _};

        let home = tempfile::tempdir().expect("tempdir");
        let repo = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(repo.path().join(".git")).expect("git");
        let pkg = repo.path().join(".cox/plugins/cur");
        let manifest = agent_package(&pkg, &[], "#!/bin/sh\ntouch \"$0.ran\"\n");
        let store = Arc::new(Store::open(home.path()).expect("store"));
        let mut config = Config::default();
        config.core.workspace_roots = vec![repo.path().to_path_buf()];
        let roots = config.core.workspace_roots.clone();

        let out = load_plugins(
            &config,
            home.path(),
            repo.path(),
            store.clone(),
            Some(&roots),
        );
        assert!(out.external_agents.is_empty());
        let line = "agent:cursor bin/agent key=CURSOR_API_KEY";
        assert!(
            out.notices
                .iter()
                .any(|n| n.contains("plugin cur is not loaded") && n.contains(line)),
            "{:?}",
            out.notices
        );
        assert!(
            !pkg.join("bin/agent.ran").exists(),
            "an ungranted agent ran"
        );

        let root = config_load::find_git_root(repo.path());
        let scope = cox_plugin::grant::scope(cox_plugin::Source::Project, root.as_deref())
            .expect("project scope");
        store
            .grant_put(&PluginGrant {
                plugin_id: "cur".into(),
                scope,
                digest: cox_plugin::package_digest(&pkg).expect("digest"),
                capabilities: serde_json::json!(cox_plugin::grant::capability_list(&manifest)),
                enabled: true,
                source: serde_json::json!({}),
                decided_at: "2026-09-26T00:00:00Z".into(),
            })
            .expect("grant");
        let out = load_plugins(
            &config,
            home.path(),
            repo.path(),
            store.clone(),
            Some(&roots),
        );
        let resolved = out.external_agents.len()
            + out
                .notices
                .iter()
                .filter(|n| {
                    n.contains("external agent cursor skipped: cannot run under the sandbox")
                })
                .count();
        assert_eq!(resolved, 1, "{:?}", out.notices);
        assert!(
            !pkg.join("bin/agent.ran").exists(),
            "loading spawned the agent"
        );
    }

    #[cfg(feature = "plugins")]
    #[test]
    fn plugin_http_server_needs_its_host_in_net() {
        let mcp = serde_json::json!({"name": "api", "url": "https://mcp.example.com/mcp"});
        let config = Config::default();
        let mut notices = Vec::new();
        let denied = manifest_with_mcp(mcp.clone(), &["api.github.com"]);
        assert!(plugin_mcp("gh", Path::new("."), &denied, &config, &[], &mut notices).is_empty());
        assert!(
            notices[0].contains("not in capabilities.net"),
            "{notices:?}"
        );
        let allowed = manifest_with_mcp(mcp, &["*.example.com"]);
        let servers = plugin_mcp("gh", Path::new("."), &allowed, &config, &[], &mut notices);
        assert_eq!(
            servers[0].1.url.as_deref(),
            Some("https://mcp.example.com/mcp")
        );
    }

    /// PL§7c, D7: a plugin's stdio server is spawned by `cox-mcp` from the
    /// argv `plugin_mcp` built, so it runs under the same Seatbelt or bwrap
    /// profile as `bash` (T4.1/T4.2): a write inside the workspace lands,
    /// one under `$HOME` is denied.
    #[cfg(all(feature = "plugins", unix))]
    #[tokio::test]
    async fn plugin_stdio_server_runs_under_sandbox() {
        use cox_tools::sandbox::{Backend, backend};
        use std::os::unix::fs::PermissionsExt as _;

        match backend(cox_protocol::LinuxBackend::Auto) {
            Some(Backend::Seatbelt | Backend::Bwrap) => {}
            other => {
                eprintln!("skipped: no argv sandbox backend here ({other:?})");
                return;
            }
        }
        let pkg = tempfile::tempdir().expect("tempdir");
        let ws = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(pkg.path().join("bin")).expect("mkdir");
        let server = pkg.path().join("bin/server");
        std::fs::write(
            &server,
            "#!/bin/sh\necho in > \"$1/inside\"\necho x > \"$2\"\n",
        )
        .expect("server");
        std::fs::set_permissions(&server, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let home = std::env::var("HOME").expect("HOME");
        let outside = format!("{home}/.cox-plugin-escape-{}", std::process::id());
        let ws_arg = ws.path().display().to_string();
        let mcp =
            serde_json::json!({"name": "s", "command": "bin/server", "args": [ws_arg, outside]});
        let mut config = Config::default();
        config.core.workspace_roots = vec![ws.path().to_path_buf()];
        let roots = config.core.workspace_roots.clone();
        let mut notices = Vec::new();
        let manifest = manifest_with_mcp(mcp, &[]);
        let servers = plugin_mcp("gh", pkg.path(), &manifest, &config, &roots, &mut notices);
        assert!(notices.is_empty(), "{notices:?}");
        let mut found = cox_mcp::discovery::Discovered::default();
        cox_mcp::discovery::add_plugin(&mut found, "gh", servers);

        // Not an MCP server, so the handshake fails once the script exits,
        // after both writes were tried.
        let timeout = std::time::Duration::from_secs(10);
        let auth = cox_mcp::client::Auth::none();
        let _ = cox_mcp::client::McpClient::connect("gh-s", &found.servers["gh-s"], timeout, &auth)
            .await;

        let leaked = Path::new(&outside).exists();
        let _ = std::fs::remove_file(&outside);
        assert!(ws.path().join("inside").exists(), "the server never ran");
        assert!(!leaked, "the sandbox let a plugin server write {outside}");
    }

    /// T33.42 Check: `every_stdio_server_runs_under_sandbox_by_default`.
    /// Same proof as `plugin_stdio_server_runs_under_sandbox`, through the
    /// path a config/`.mcp.json`-sourced server takes instead of a
    /// plugin's: a write inside the workspace lands, one under `$HOME` is
    /// denied.
    #[cfg(unix)]
    #[tokio::test]
    async fn every_stdio_server_runs_under_sandbox_by_default() {
        use cox_tools::sandbox::{Backend, backend};
        use std::os::unix::fs::PermissionsExt as _;

        match backend(cox_protocol::LinuxBackend::Auto) {
            Some(Backend::Seatbelt | Backend::Bwrap) => {}
            other => {
                eprintln!("skipped: no argv sandbox backend here ({other:?})");
                return;
            }
        }
        let ws = tempfile::tempdir().expect("tempdir");
        let server = ws.path().join("server.sh");
        std::fs::write(
            &server,
            "#!/bin/sh\necho in > \"$1/inside\"\necho x > \"$2\"\n",
        )
        .expect("server");
        std::fs::set_permissions(&server, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let home = std::env::var("HOME").expect("HOME");
        let outside = format!("{home}/.cox-mcp-escape-{}", std::process::id());
        let ws_arg = ws.path().display().to_string();

        let mut config = Config::default();
        config.core.workspace_roots = vec![ws.path().to_path_buf()];
        let writable = config.core.workspace_roots.clone();

        let mut found = cox_mcp::discovery::Discovered::default();
        found.servers.insert(
            "s".to_string(),
            McpServerConfig {
                command: Some(server.display().to_string()),
                args: vec![ws_arg, outside.clone()],
                ..McpServerConfig::default()
            },
        );
        found.sources.insert("s".to_string(), "config".to_string());

        sandbox_stdio_servers(&mut found, &config, &writable);
        assert!(found.notices.is_empty(), "{:?}", found.notices);

        // Not an MCP server, so the handshake fails once the script exits,
        // after both writes were tried.
        let timeout = std::time::Duration::from_secs(10);
        let auth = cox_mcp::client::Auth::none();
        let _ = cox_mcp::client::McpClient::connect("s", &found.servers["s"], timeout, &auth).await;

        let leaked = Path::new(&outside).exists();
        let _ = std::fs::remove_file(&outside);
        assert!(ws.path().join("inside").exists(), "the server never ran");
        assert!(!leaked, "the sandbox let a config server write {outside}");
    }

    /// T33.42 Check: `sandbox_false_opts_a_named_server_out`. A server
    /// with `sandbox = false` reaches `connect_all` with its argv
    /// untouched — not even the `/bin/sh -c 'exec "$0" "$@"'` shell hop.
    #[test]
    fn sandbox_false_opts_a_named_server_out() {
        let mut config = Config::default();
        config.core.workspace_roots = vec![PathBuf::from("/tmp")];
        let mut found = cox_mcp::discovery::Discovered::default();
        found.servers.insert(
            "s".to_string(),
            McpServerConfig {
                command: Some("echo".to_string()),
                args: vec!["hi".to_string()],
                sandbox: false,
                ..McpServerConfig::default()
            },
        );
        found.sources.insert("s".to_string(), "config".to_string());

        sandbox_stdio_servers(&mut found, &config, &[]);

        assert_eq!(found.servers["s"].command.as_deref(), Some("echo"));
        assert_eq!(found.servers["s"].args, ["hi"]);
        assert!(found.notices.is_empty(), "{:?}", found.notices);
    }

    /// The same fixture `cox-provider-openai`'s own Chat tests read
    /// (`crates/cox-provider-openai/src/chat.rs`), one directory further up
    /// from this crate.
    #[cfg(feature = "plugins")]
    fn openai_chat_fixture(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/openai-chat")
            .join(format!("{name}.sse"));
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading fixture {path:?}: {e}"))
    }

    #[cfg(feature = "plugins")]
    fn provider_test_request(model: &str) -> cox_protocol::types::Request {
        use cox_protocol::types::{
            Content, Effort, Message, ModelId, Role, SystemBlock, Thinking, Tier,
        };
        cox_protocol::types::Request {
            tier: Tier::Code,
            job: Job::Main,
            model: ModelId(model.into()),
            system: vec![SystemBlock {
                text: "You are cox.".into(),
                cache: true,
            }],
            tools: vec![],
            messages: vec![Message {
                role: Role::User,
                content: vec![Content::Text {
                    text: "hello".into(),
                }],
            }],
            effort: Effort::High,
            max_tokens: 1024,
            thinking: Thinking::Off,
            cache_breakpoints: vec![],
            stop_sequences: vec![],
        }
    }

    /// T33.17 Check: a granted plugin's `api = "chat"` `[[provider]]` row,
    /// merged by `cox_plugin::provider::merge`, becomes a real
    /// `OpenAiChatProvider` once `tiers.code.provider` names it — same
    /// `providers.custom` "Type-2" arm `deepseek_config`'s tests exercise
    /// above, now fed a plugin section instead of a hand-written one, and
    /// proven against a mock server rather than just its shape (§7a: "cox
    /// drives it with its own wire clients").
    #[cfg(feature = "plugins")]
    #[tokio::test]
    async fn plugin_chat_section_builds_openai_shaped_client() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/chat/completions"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_raw(openai_chat_fixture("text_only"), "text/event-stream"),
            )
            .mount(&server)
            .await;

        let decl = cox_plugin_api::ProviderDecl {
            name: "acme".into(),
            api: cox_plugin_api::ProviderApi::Chat,
            base_url: server.uri(),
            api_key_env: None,
            auth: cox_plugin_api::ProviderAuth::Bearer,
        };
        let decls = [decl];
        let plugins = [cox_plugin::provider::PluginProviders {
            plugin: "acme-pkg",
            decls: &decls,
        }];
        let merged = cox_plugin::provider::merge(
            &cox_protocol::config::ProvidersConfig::default(),
            &plugins,
        );
        assert!(merged.warnings.is_empty(), "{:?}", merged.warnings);

        let mut config = Config::default();
        config.providers.custom.extend(merged.custom);
        config.tiers.code.provider = "acme".into();
        config.tiers.code.model = "acme-coder".into();

        let provider = provider_for_with(&config, no_key).expect("builds the merged section");
        assert_eq!(provider.id(), ProviderId::Local);

        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        provider
            .stream(
                provider_test_request("acme-coder"),
                tx,
                tokio_util::sync::CancellationToken::new(),
            )
            .await
            .expect("the merged section speaks the wire it was built for");
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        assert!(
            events.iter().any(|e| matches!(
                e,
                cox_protocol::types::ProviderEvent::Stop {
                    stop: cox_protocol::types::StopReason::EndTurn
                }
            )),
            "{events:?}"
        );
    }

    /// T33.17 Check (invariant 8): a full turn against a plugin's merged
    /// `[[provider]]` section writes exactly one ledger row, the same as
    /// any other provider — `Priced` (wrapped in by `provider_for_with`)
    /// prices the call and `cox-core` records it (`session.rs:1340`).
    #[cfg(feature = "plugins")]
    #[tokio::test]
    async fn plugin_provider_request_has_usage_row() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/chat/completions"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_raw(openai_chat_fixture("text_only"), "text/event-stream"),
            )
            .mount(&server)
            .await;

        let decl = cox_plugin_api::ProviderDecl {
            name: "acme".into(),
            api: cox_plugin_api::ProviderApi::Chat,
            base_url: server.uri(),
            api_key_env: None,
            auth: cox_plugin_api::ProviderAuth::Bearer,
        };
        let decls = [decl];
        let plugins = [cox_plugin::provider::PluginProviders {
            plugin: "acme-pkg",
            decls: &decls,
        }];
        let merged = cox_plugin::provider::merge(
            &cox_protocol::config::ProvidersConfig::default(),
            &plugins,
        );
        let mut config = Config::default();
        config.providers.custom.extend(merged.custom);
        config.tiers.code.provider = "acme".into();
        config.tiers.code.model = "acme-coder".into();

        let provider: Arc<dyn Provider> =
            provider_for_with(&config, no_key).expect("builds the merged section");
        let home = tempfile::tempdir().expect("home");
        let work = tempfile::tempdir().expect("work");
        let store = Arc::new(Store::open(home.path()).expect("store"));
        let session = Session::new(
            config,
            provider,
            vec![],
            store.clone(),
            store.clone(),
            work.path().to_path_buf(),
        )
        .expect("session");
        user_turn(&session, "hello").await;

        let rows = store.usage_for_session(&session.id()).expect("usage query");
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].provider, ProviderId::Local);
    }

    /// T33.44 fixture: a plugin module. `cox_init` `cox_notify`s
    /// `init_note` (when not empty) and answers `init_out`; `cox_hook`
    /// counts its calls, `cox_notify`s `hook_note` (when not empty) and
    /// answers `continue`; `cox_on_event` answers one notice, `hooks <n>`.
    #[cfg(feature = "plugins")]
    fn plugin_wat(init_note: &str, init_out: &str, hook_note: &str) -> String {
        let note = |text: &str| format!(r#"{{"level":"info","text":"{text}"}}"#);
        let effects = r#"{"redraw":false,"notices":[{"level":"info","text":"hooks 0"}]}"#;
        let digit = 1024 + effects.find('0').expect("digit");
        let data = |at: usize, text: &str| {
            let wat = text.replace('\\', "\\\\").replace('"', "\\\"");
            format!(r#"(data (i32.const {at}) "{wat}")"#)
        };
        let notify = |at: usize, text: &str| match text {
            "" => String::new(),
            _ => format!(
                "(drop (call $notify (call $copy (i32.const {at}) (i32.const {}))))",
                note(text).len()
            ),
        };
        format!(
            r#"(module
              (import "extism:host/env" "alloc" (func $alloc (param i64) (result i64)))
              (import "extism:host/env" "store_u8" (func $store (param i64 i32)))
              (import "extism:host/env" "output_set" (func $output_set (param i64 i64)))
              (import "cox:host/v1" "cox_notify" (func $notify (param i64) (result i64)))
              (memory 1)
              (global $n (mut i32) (i32.const 0))
              {d0} {d1} {d2} {d3} {d4}
              (func $copy (param $p i32) (param $len i32) (result i64) (local $off i64) (local $i i32)
                (local.set $off (call $alloc (i64.extend_i32_u (local.get $len))))
                (block $done (loop $next
                  (br_if $done (i32.ge_u (local.get $i) (local.get $len)))
                  (call $store (i64.add (local.get $off) (i64.extend_i32_u (local.get $i)))
                    (i32.load8_u (i32.add (local.get $p) (local.get $i))))
                  (local.set $i (i32.add (local.get $i) (i32.const 1)))
                  (br $next)))
                (local.get $off))
              (func $out (param $p i32) (param $len i32)
                (call $output_set (call $copy (local.get $p) (local.get $len))
                  (i64.extend_i32_u (local.get $len))))
              (func (export "cox_init") (result i32)
                {init_notify}
                (call $out (i32.const 0) (i32.const {init_len})) (i32.const 0))
              (func (export "cox_hook") (result i32)
                (global.set $n (i32.add (global.get $n) (i32.const 1)))
                {hook_notify}
                (call $out (i32.const 768) (i32.const 19)) (i32.const 0))
              (func (export "cox_on_event") (result i32)
                (i32.store8 (i32.const {digit}) (i32.add (i32.const 48) (global.get $n)))
                (call $out (i32.const 1024) (i32.const {effects_len})) (i32.const 0)))"#,
            d0 = data(0, init_out),
            d1 = data(256, &note(init_note)),
            d2 = data(512, &note(hook_note)),
            d3 = data(768, r#"{"type":"continue"}"#),
            d4 = data(1024, effects),
            init_notify = notify(256, init_note),
            hook_notify = notify(512, hook_note),
            init_len = init_out.len(),
            effects_len = effects.len(),
        )
    }

    /// Installs a user plugin under `home` the way `cox plugin install`
    /// leaves it, granted for its exact digest (T33.44 fixtures).
    #[cfg(feature = "plugins")]
    fn install_granted(home: &Path, id: &str, extra_toml: &str, wasm: &str) {
        let staged = home.join("plugins").join(id).join("versions/staged");
        std::fs::create_dir_all(&staged).expect("plugin dir");
        let toml = format!(
            "api = 1\nid = \"{id}\"\nversion = \"0.1.0\"\nname = \"{id}\"\nwasm = \"plugin.wasm\"\n{extra_toml}"
        );
        std::fs::write(staged.join("plugin.toml"), toml).expect("plugin.toml");
        std::fs::write(staged.join("plugin.wasm"), wasm).expect("plugin.wasm");
        let digest = cox_plugin::package_digest(&staged).expect("digest");
        std::fs::rename(&staged, staged.with_file_name(&digest[..12])).expect("stage");
        std::fs::write(home.join("plugins").join(id).join("current"), &digest[..12])
            .expect("current");
        let found = cox_plugin::discover::discover(home, None);
        let plugin = found.plugins.iter().find(|p| p.id == id).expect("found");
        let cox_plugin::State::Loaded { manifest, digest } = &plugin.state else {
            panic!("{id} did not load: {:?}", found.notices);
        };
        let store = Store::open(home).expect("store");
        crate::plugin_cmd::write_grant(
            &store,
            id,
            &GrantScope::User,
            digest,
            cox_plugin::grant::capability_list(manifest),
            serde_json::json!({}),
        )
        .expect("grant");
    }

    /// `open`'s plugin steps over a scripted session: discover and load,
    /// build, `start_plugins`, the hook chain, then the notices in order.
    #[cfg(feature = "plugins")]
    async fn session_with_plugins(home: &Path, work: &Path, turns: usize) -> (Session, Arc<Store>) {
        let scenario: String = (0..turns)
            .map(|i| format!("[[turn]]\ntext = \"reply {i}\"\n"))
            .collect();
        let (session, store) = scripted_session(home, work, &scenario);
        let config = Config::default();
        let plugins = load_plugins(&config, home, work, store.clone(), None);
        let (hooks, started) = start_plugins(&session, plugins.live, &config.plugins, work, None);
        session.set_hook(Arc::new(cox_ext::hooks::HookChain::new(None, hooks)));
        let loaded = plugins.notices.into_iter().map(|w| (Level::Warn, w));
        for (level, text) in loaded.chain(started) {
            session.notice(level, text).await.expect("notice");
        }
        (session, store)
    }

    #[cfg(feature = "plugins")]
    fn notices(store: &Store, session: &Session) -> Vec<String> {
        store
            .rollout_read(&session.id())
            .expect("rollout")
            .into_iter()
            .filter_map(|ev| match ev {
                Event::Notice { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    }

    /// Polls the rollout until a notice satisfies `want`; the tap's notices
    /// reach it through a task, after the event that drained them.
    #[cfg(feature = "plugins")]
    async fn wait_for_notice(
        store: &Store,
        session: &Session,
        want: impl Fn(&str) -> bool,
    ) -> String {
        for _ in 0..250 {
            if let Some(found) = notices(store, session).into_iter().find(|n| want(n)) {
                return found;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("no such notice in {:?}", notices(store, session));
    }

    #[cfg(feature = "plugins")]
    #[tokio::test]
    async fn granted_plugin_runs_cox_init_once_per_session() {
        let (home, work) = (
            tempfile::tempdir().expect("home"),
            tempfile::tempdir().expect("work"),
        );
        install_granted(home.path(), "once", "", &plugin_wat("init ran", "{}", ""));
        let (session, store) = session_with_plugins(home.path(), work.path(), 2).await;
        user_turn(&session, "one").await;
        user_turn(&session, "two").await;
        let inits = notices(&store, &session)
            .into_iter()
            .filter(|n| n == "plugin once: init ran")
            .count();
        assert_eq!(inits, 1, "{:?}", notices(&store, &session));
    }

    #[cfg(feature = "plugins")]
    #[tokio::test]
    async fn plugin_notify_reaches_the_transcript() {
        let (home, work) = (
            tempfile::tempdir().expect("home"),
            tempfile::tempdir().expect("work"),
        );
        let caps = "[capabilities]\nhooks = [\"UserPromptSubmit\"]\n";
        install_granted(
            home.path(),
            "tell",
            caps,
            &plugin_wat("", "{}", r"\u001b[2Jfrom a hook"),
        );
        let (session, store) = session_with_plugins(home.path(), work.path(), 1).await;
        user_turn(&session, "hi").await;
        // Sanitized on the way (T33.9), attributed to the plugin here.
        let got = wait_for_notice(&store, &session, |n| n.contains("from a hook")).await;
        assert_eq!(got, "plugin tell: from a hook");
    }

    #[cfg(feature = "plugins")]
    #[tokio::test]
    async fn hooks_and_event_tap_share_one_plugin_instance() {
        let (home, work) = (
            tempfile::tempdir().expect("home"),
            tempfile::tempdir().expect("work"),
        );
        let caps = "[capabilities]\nhooks = [\"UserPromptSubmit\"]\nevents = [\"turn_done\"]\n";
        let init_out = r#"{"subscribe":["turn_done"]}"#;
        install_granted(home.path(), "ctr", caps, &plugin_wat("", init_out, ""));
        let (session, store) = session_with_plugins(home.path(), work.path(), 8).await;
        // The tap drains after an event, so a later turn carries the notice
        // `cox_on_event` answered for an earlier `turn_done`.
        let mut seen = None;
        for i in 0..8 {
            user_turn(&session, &format!("turn {i}")).await;
            seen = notices(&store, &session)
                .into_iter()
                .find(|n| n.starts_with("plugin ctr: hooks"));
            if seen.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        // Two instances would leave the event side's count at 0.
        let seen = seen.expect("a notice from cox_on_event");
        assert_ne!(seen, "plugin ctr: hooks 0");
    }

    #[cfg(feature = "plugins")]
    #[tokio::test]
    async fn plugin_init_failure_is_skipped_with_a_warning() {
        let (home, work) = (
            tempfile::tempdir().expect("home"),
            tempfile::tempdir().expect("work"),
        );
        let caps = "[capabilities]\nhooks = [\"UserPromptSubmit\"]\n";
        let trap = r#"(module (func (export "cox_init") (result i32) unreachable)
                      (func (export "cox_hook") (result i32) unreachable))"#;
        install_granted(home.path(), "boom", caps, trap);
        let (session, store) = session_with_plugins(home.path(), work.path(), 1).await;
        // The session goes on and the dropped plugin's hook never runs:
        // it would trap, which the core reports as a hook warning.
        user_turn(&session, "hi").await;
        let all = notices(&store, &session);
        assert!(
            all.iter()
                .any(|n| n.starts_with("plugin boom failed to start")),
            "{all:?}"
        );
        assert_eq!(all.len(), 1, "{all:?}");
    }

    #[cfg(feature = "plugins")]
    #[test]
    fn granted_plugin_models_join_the_catalog() {
        fn fake_key(_: &str, _: &str) -> Result<String, cox_protocol::errors::ProviderError> {
            Ok("sk-test".to_string())
        }
        let (home, work) = (
            tempfile::tempdir().expect("home"),
            tempfile::tempdir().expect("work"),
        );
        let init = r#"(module (func (export "cox_init") (result i32) (i32.const 0)))"#;
        let row =
            |window: u32| format!("[[models]]\nid = \"plug-big\"\ncontext_window = {window}\n");
        install_granted(home.path(), "aa", &row(1_000_000), init);
        install_granted(home.path(), "zz", &row(5), init);
        let store = Arc::new(Store::open(home.path()).expect("store"));
        let mut cfg = Config::default();
        cfg.tiers.code.model = "plug-big".into();
        let plugins = load_plugins(&cfg, home.path(), work.path(), store, None);
        // The lower id keeps the row; the other is shown, not applied.
        assert!(
            plugins
                .notices
                .iter()
                .any(|n| n == "plugin zz also defines model plug-big; plugin aa's row is kept"),
            "{:?}",
            plugins.notices
        );
        let p = provider_for_served(&cfg, fake_key, None, &plugins.catalog_rows())
            .expect("anthropic builds");
        assert_eq!(p.capabilities().max_context, 1_000_000);
    }
}
