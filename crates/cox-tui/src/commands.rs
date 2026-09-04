//! Slash commands (T5.5): the §1.13 list, parsed from the composer into what
//! each means — a `Submission` for the core, a runtime action, or a change to
//! the screen. One table feeds the `/` palette, `/help` and the parser, so
//! they cannot disagree. Separate from `state` so a test checks a line of
//! text against an `Action` without a terminal.

use cox_protocol::types::{ModelId, PermissionMode, SlashCommand, Submission, Tier};

/// `(name, usage, what it does)`; the palette lists the names in this order.
pub const COMMANDS: &[(&str, &str, &str)] = &[
    (
        "model",
        "/model [cheap|code|think] [model]",
        "switch a tier's model",
    ),
    (
        "think",
        "/think <prompt>",
        "one turn on the think tier, price confirmed first",
    ),
    ("compact", "/compact [focus]", "compact the context now"),
    ("cost", "/cost", "what this session has spent"),
    (
        "permissions",
        "/permissions [default|plan|auto|bypass]",
        "show or set the permission mode",
    ),
    (
        "sandbox",
        "/sandbox <read-only|workspace-write|danger-full-access>",
        "set the sandbox mode",
    ),
    ("resume", "/resume", "pick an earlier session"),
    ("sessions", "/sessions", "list sessions"),
    (
        "expand",
        "/expand <id>",
        "show an archived tool output in full",
    ),
    ("agents", "/agents", "live cox sessions in this workspace"),
    ("skills", "/skills", "list skills"),
    ("hooks", "/hooks", "list hooks"),
    ("mcp", "/mcp", "MCP servers and their tools"),
    ("doctor", "/doctor", "check the install"),
    ("clear", "/clear", "new session, same directory"),
    ("todo", "/todo", "toggle the todo panel"),
    ("tasks", "/tasks", "list running background tasks"),
    ("vim", "/vim", "toggle vim keys"),
    ("help", "/help", "this list"),
    ("quit", "/quit", "exit"),
];

/// What a parsed command asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Submit(Submission),
    Quit,
    Help,
    Cost,
    /// Toggle the todo panel.
    Todo,
    /// List the running background tasks.
    Tasks,
    /// List the other live sessions of this workspace.
    Agents,
    /// Set the permission mode on the screen and in the core.
    Mode(PermissionMode),
    /// Toggle vim keys in the composer.
    Vim,
    /// Something to tell the user without leaving the TUI.
    Notice(String),
}

/// `None` when `line` is not a slash command; `tier` is the one `/model`
/// switches when the first argument does not name one.
pub fn parse(line: &str, tier: Tier) -> Option<Action> {
    let rest = line.strip_prefix('/')?;
    let mut words = rest.split_whitespace();
    let name = words.next()?;
    let args: Vec<String> = words.map(str::to_string).collect();
    let joined = || (!args.is_empty()).then(|| args.join(" "));
    Some(match name {
        "model" => {
            let (tier, model) = match args.first().and_then(|a| tier_named(a)) {
                Some(t) => (t, args.get(1)),
                None => (tier, args.first()),
            };
            Action::Submit(Submission::SwitchModel {
                tier,
                model: model.map(|m| ModelId(m.clone())),
            })
        }
        "think" => match joined() {
            Some(text) => Action::Submit(Submission::UserTurn {
                text,
                attachments: Vec::new(),
                confirm_think: true,
            }),
            None => Action::Notice("/think needs a prompt".into()),
        },
        "compact" => Action::Submit(Submission::Compact { focus: joined() }),
        "cost" => Action::Cost,
        "permissions" => match args.first().map(String::as_str) {
            None => Action::Notice("/permissions <default|plan|auto|bypass>".into()),
            Some(m) => match mode_named(m) {
                Some(mode) => Action::Mode(mode),
                None => Action::Notice(format!("unknown permission mode `{m}`")),
            },
        },
        "todo" => Action::Todo,
        "tasks" => Action::Tasks,
        "agents" => Action::Agents,
        "vim" => Action::Vim,
        "help" => Action::Help,
        "quit" => Action::Quit,
        _ if COMMANDS.iter().any(|(n, ..)| *n == name) => Action::Submit(Submission::Command {
            command: SlashCommand {
                name: name.to_string(),
                args,
            },
        }),
        _ => Action::Notice(format!("unknown command /{name}; /help lists them")),
    })
}

/// `/help`: one line per command.
pub fn help() -> String {
    let width = COMMANDS.iter().map(|(_, u, _)| u.len()).max().unwrap_or(0);
    COMMANDS
        .iter()
        .map(|(_, usage, what)| format!("{usage:width$}  {what}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn tier_named(s: &str) -> Option<Tier> {
    match s {
        "cheap" => Some(Tier::Cheap),
        "code" => Some(Tier::Code),
        "think" => Some(Tier::Think),
        _ => None,
    }
}

fn mode_named(s: &str) -> Option<PermissionMode> {
    match s {
        "default" => Some(PermissionMode::Default),
        "plan" => Some(PermissionMode::Plan),
        "auto" => Some(PermissionMode::Auto),
        "bypass" => Some(PermissionMode::Bypass),
        _ => None,
    }
}

/// `Tab`: default → plan → auto → default (§1.13).
pub fn next_mode(mode: PermissionMode) -> PermissionMode {
    match mode {
        PermissionMode::Default => PermissionMode::Plan,
        PermissionMode::Plan => PermissionMode::Auto,
        PermissionMode::Auto | PermissionMode::Bypass => PermissionMode::Default,
    }
}
