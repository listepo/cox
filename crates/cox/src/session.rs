//! Builds a live [`Session`] for the interactive surfaces: config, provider,
//! store and the built-in tool set. Kept out of `main.rs` so the TUI and
//! `cox run -p` (T6.1) assemble the same session the same way.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cox_core::{History, Session};
use cox_protocol::Config;
use cox_protocol::config::Transport;
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
/// login (T22.5).
pub async fn open(
    cli: &Cli,
    cwd: &Path,
    answer: Option<String>,
    questions: Option<tokio::sync::mpsc::Sender<AskUserQuestion>>,
    tweak: impl FnOnce(&mut Config),
    resume: Option<(SessionId, History)>,
    interactive: bool,
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
    let config = loaded.config.clone();
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
            )?
        }
        None => provider_for(&config)?,
    };
    let home = cli.home.clone().unwrap_or_else(config_load::cox_home);
    let store = Arc::new(Store::open(&home)?);
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
        all.extend(mcp_tools(&config, cwd, interactive).await);
    }
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
    // A14: the presence hook wraps the user's shell hooks so the other
    // sessions of this workspace see every surface, `--no-hooks` or not.
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
            shell,
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
    Ok((session, loaded))
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
async fn mcp_tools(config: &Config, cwd: &Path, interactive: bool) -> Vec<Arc<dyn Tool>> {
    let found = mcp_servers(config, cwd);
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
    let (session, _) = rt.block_on(open(cli, cwd, None, None, |_| {}, None, false))?;
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
        let (session, loaded) = rt.block_on(open(
            cli,
            cwd,
            None,
            Some(question_tx),
            |_| {},
            resume_spec.take(),
            true,
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
        let (feed, feed_rx) = tokio::sync::mpsc::channel(4);
        let (ask, mut ask_rx) = tokio::sync::mpsc::channel(1);
        let (surfaced, surfaced_rx) = tokio::sync::mpsc::channel(1);
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
    provider_for_served(config, resolve, None)
}

/// [`provider_for_with`] plus what a local server reported for the
/// session's model (T30.16), overlaid on the catalog before any lookup.
fn provider_for_served(
    config: &Config,
    resolve: impl FnOnce(&str, &str) -> Result<String, cox_protocol::errors::ProviderError>,
    served: Option<&cox_provider::lmstudio::Model>,
) -> anyhow::Result<Arc<dyn Provider>> {
    if let Some(double) = cox_provider::from_env()? {
        return Ok(Arc::from(double));
    }
    let prices = Arc::new(PriceTable::embedded()?);
    Ok(Arc::new(Priced::new(
        backend_for_with(config, resolve, served)?,
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
) -> anyhow::Result<Arc<dyn Provider>> {
    // T30.25: `Caps.max_context` for the sections below comes from the
    // model catalog rather than a per-family literal. `Catalog::load`
    // always carries the embedded built-in rows even when `config` overlays
    // none of its own (unlike reading `providers.*.models` directly, which
    // is empty on a bare `Config::default()`); a bad/unparseable catalog
    // falls back to the empty default, which is exactly "no row found" —
    // every lookup below already has its own literal fallback for that.
    let mut catalog = cox_models::Catalog::load(config, None).unwrap_or_default();
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

        let p = provider_for_served(&cfg, no_key, Some(served)).expect("builds");
        assert_eq!(p.capabilities().max_context, 251_648);

        cfg.providers.lmstudio.context_window = 65_536;
        let p = provider_for_served(&cfg, no_key, Some(served)).expect("builds");
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
}
