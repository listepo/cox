//! Slash commands (T5.5): the §1.13 list, parsed from the composer into what
//! each means — a `Submission` for the core, a runtime action, or a change to
//! the screen. One table feeds the `/` palette, `/help` and the parser, so
//! they cannot disagree. Separate from `state` so a test checks a line of
//! text against an `Action` without a terminal.

use std::time::Duration;

use cox_protocol::types::{Effort, ModelId, PermissionMode, SlashCommand, Submission, Tier};

use crate::keymap::Keymap;

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
    (
        "effort",
        "/effort [low|medium|high|xhigh]",
        "effort for the rest of the session; bare restores the tier default",
    ),
    ("compact", "/compact [focus]", "compact the context now"),
    (
        "rewind",
        "/rewind",
        "go back to an earlier turn: code, conversation or both",
    ),
    ("undo", "/undo", "undo the last turn's file changes"),
    ("redo", "/redo", "redo what /undo took back"),
    ("cost", "/cost", "what this session has spent"),
    ("context", "/context", "where the next request's tokens go"),
    (
        "autocompact",
        "/autocompact",
        "the compaction threshold and its config source",
    ),
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
    ("resume", "/resume", "pick an earlier session to resume"),
    ("sessions", "/sessions", "this project's recent sessions"),
    (
        "expand",
        "/expand <id>",
        "show an archived tool output in full",
    ),
    ("agents", "/agents", "live cox sessions in this workspace"),
    (
        "loop",
        "/loop <interval> <prompt> [--budget usd] | /loop stop",
        "repeat a prompt on a timer with its own budget cap",
    ),
    ("skills", "/skills", "list skills"),
    ("hooks", "/hooks", "list hooks"),
    ("mcp", "/mcp", "MCP servers and their tools"),
    ("doctor", "/doctor", "check the install"),
    ("clear", "/clear", "new session, same directory"),
    (
        "fork",
        "/fork [turn]",
        "new child session with the history up to a turn (default: all)",
    ),
    (
        "handoff",
        "/handoff <objective>",
        "new child session seeded with a cheap summary and the objective",
    ),
    (
        "init",
        "/init [--force]",
        "scaffold AGENTS.md for this repo",
    ),
    ("todo", "/todo", "toggle the todo panel"),
    ("tasks", "/tasks", "list running background tasks"),
    ("vim", "/vim", "toggle vim keys"),
    (
        "theme",
        "/theme [name]",
        "pick a colour theme, previewed live",
    ),
    ("help", "/help", "this list"),
    ("quit", "/quit", "exit"),
];

/// Where a key applies (T24.6): the composer with no turn, a running turn,
/// a modal that takes the keys (approval, question, picker), or an overlay
/// drawn over the transcript (help, diff).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    Idle,
    Running,
    Modal,
    Overlay,
}

impl Context {
    pub const ALL: [Context; 4] = [
        Context::Idle,
        Context::Running,
        Context::Modal,
        Context::Overlay,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Context::Idle => "idle",
            Context::Running => "running",
            Context::Modal => "modal",
            Context::Overlay => "overlay",
        }
    }
}

/// `(key, action, context)`: the one keymap the footer hints, the `?`
/// overlay, `/help` and `docs/getting-started.md` all read, and the default
/// `keymap::Keymap` (T25.5). Within a context the first rows are the footer
/// hints, so order matters; an action's rows stay together.
pub const KEYMAP: &[(&str, &str, Context)] = &[
    ("Enter", "send", Context::Idle),
    ("Shift+Tab", "mode.cycle", Context::Idle),
    ("@", "file", Context::Idle),
    ("/", "command", Context::Idle),
    ("?", "help", Context::Idle),
    ("Shift+Enter", "newline", Context::Idle),
    ("Alt+Enter", "newline", Context::Idle),
    ("Ctrl+Enter", "newline", Context::Idle),
    ("Ctrl+R", "history", Context::Idle),
    ("Ctrl+T", "thinking", Context::Idle),
    ("Ctrl+O", "transcript", Context::Idle),
    ("Ctrl+E", "expand", Context::Idle),
    ("Ctrl+G", "diff", Context::Idle),
    ("Ctrl+C", "quit", Context::Idle),
    ("Ctrl+D", "quit", Context::Idle),
    // T23.4: plain letters, so only an empty composer claims them (same
    // rule `?`/`help` follows) — typing "yes" still types "yes".
    ("y", "copy", Context::Idle),
    ("Shift+Y", "copy.all", Context::Idle),
    ("Esc", "interrupt", Context::Running),
    ("Ctrl+C", "interrupt", Context::Running),
    ("Ctrl+B", "background", Context::Running),
    ("Ctrl+O", "transcript", Context::Running),
    ("Alt+Enter", "send.now", Context::Running),
    ("Ctrl+Enter", "send.now", Context::Running),
    ("Ctrl+U", "unqueue", Context::Running),
    ("Enter", "choose", Context::Modal),
    ("Esc", "close", Context::Modal),
    ("Up", "previous", Context::Modal),
    ("Down", "next", Context::Modal),
    ("Esc", "close", Context::Overlay),
    ("?", "close", Context::Overlay),
    ("PageUp", "scroll.up", Context::Overlay),
    ("PageDown", "scroll.down", Context::Overlay),
];

/// An action id as the footer and overlay print it: `mode.cycle` → `mode cycle`.
pub fn label(action: &str) -> String {
    action.replace('.', " ")
}

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
    /// List this project's recent sessions.
    Sessions,
    /// Open the session picker.
    Resume,
    /// Open the rewind timeline (T26.2); `Esc Esc` on an empty composer too.
    Rewind,
    /// `/undo` (T26.4): a code-only rewind of the last turn.
    Undo,
    /// `/redo` (T26.4): one step back out of the last rewind.
    Redo,
    /// `/fork [turn]` (T26.3): `None` keeps every turn.
    Fork(Option<u32>),
    /// `/handoff <objective>` (T26.3).
    Handoff(String),
    /// Set the permission mode on the screen and in the core.
    Mode(PermissionMode),
    /// Toggle vim keys in the composer.
    Vim,
    /// `/theme [name]`: a name applies it directly, `None` opens the picker
    /// (T24.2).
    Theme(Option<String>),
    /// Something to tell the user without leaving the TUI.
    Notice(String),
    /// `/loop <interval> <prompt> [--budget <usd>]` (T27.4): repeat `prompt`
    /// on `interval` while idle, until `/loop stop`, `Esc` on an empty
    /// composer, or `budget_usd` (`None`: the session cap) is spent.
    LoopStart {
        interval: Duration,
        prompt: String,
        budget_usd: Option<f64>,
    },
    /// `/loop stop` (T27.4).
    LoopStop,
    /// `!cmd` / `!!cmd` (T25.3): run `cmd` through the `bash` tool; `share`
    /// lets the output into history.
    Shell {
        cmd: String,
        share: bool,
    },
}

/// `None` when `line` is neither a slash command nor a `!` shell line;
/// `tier` is the one `/model` switches when the first argument does not
/// name one.
pub fn parse(line: &str, tier: Tier) -> Option<Action> {
    if let Some(rest) = line.strip_prefix('!') {
        let (share, cmd) = match rest.strip_prefix('!') {
            Some(cmd) => (true, cmd.trim()),
            None => (false, rest.trim()),
        };
        return Some(match cmd.is_empty() {
            true => Action::Notice("`!` needs a command".into()),
            false => Action::Shell {
                cmd: cmd.into(),
                share,
            },
        });
    }
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
        "effort" => match args.first().map(String::as_str) {
            None => Action::Submit(Submission::SetEffort { effort: None }),
            Some(level) => match Effort::parse(level) {
                Some(effort) => Action::Submit(Submission::SetEffort {
                    effort: Some(effort),
                }),
                None => Action::Notice(format!(
                    "unknown effort `{level}`; low, medium, high or xhigh"
                )),
            },
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
        "loop" => match args.first().map(String::as_str) {
            Some("stop") => Action::LoopStop,
            _ => loop_start(&args).unwrap_or_else(|| {
                Action::Notice("/loop <interval> <prompt> [--budget usd] | /loop stop".into())
            }),
        },
        "sessions" => Action::Sessions,
        "resume" => Action::Resume,
        "rewind" => Action::Rewind,
        "undo" => Action::Undo,
        "redo" => Action::Redo,
        // `T7` as the rewind timeline prints it, or a bare `7`.
        "fork" => match args.first() {
            None => Action::Fork(None),
            Some(arg) => match arg.trim_start_matches(['T', 't']).parse::<u32>() {
                Ok(turn) if turn > 0 => Action::Fork(Some(turn)),
                _ => Action::Notice(format!("/fork [turn]: `{arg}` is not a turn number")),
            },
        },
        "handoff" => match joined() {
            Some(objective) => Action::Handoff(objective),
            None => Action::Notice("/handoff needs an objective".into()),
        },
        "vim" => Action::Vim,
        "theme" => Action::Theme(joined()),
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

/// `/help`: the keymap as bound now, one line per context, then one line
/// per command.
pub fn help(keymap: &Keymap) -> String {
    let keys = Context::ALL.iter().map(|ctx| {
        let rows: Vec<String> = keymap
            .rows(*ctx)
            .into_iter()
            .map(|(k, a)| format!("{k} {}", label(a)))
            .collect();
        format!("{:8} {}", ctx.name(), rows.join(" · "))
    });
    let width = COMMANDS.iter().map(|(_, u, _)| u.len()).max().unwrap_or(0);
    let commands = COMMANDS
        .iter()
        .map(|(_, usage, what)| format!("{usage:width$}  {what}"));
    keys.chain(commands).collect::<Vec<_>>().join("\n")
}

/// `/loop <interval> <prompt...> [--budget <usd>]`: `--budget` may sit
/// anywhere after the interval; `None` when the interval or the prompt is
/// missing or malformed, which `parse` turns into the usage notice.
fn loop_start(args: &[String]) -> Option<Action> {
    let (interval_s, rest) = args.split_first()?;
    let interval = parse_interval(interval_s)?;
    let mut words = rest.to_vec();
    let budget_at = words.iter().position(|a| a == "--budget").and_then(|i| {
        let value = words.get(i + 1)?.parse::<f64>().ok()?;
        Some((i, value))
    });
    let budget_usd = budget_at.map(|(i, value)| {
        words.drain(i..=i + 1);
        value
    });
    (!words.is_empty()).then(|| Action::LoopStart {
        interval,
        prompt: words.join(" "),
        budget_usd,
    })
}

/// `<n>s` / `<n>m` / `<n>h`, or a bare `<n>` as seconds; `0` is rejected so
/// a due tick cannot fire on every `Msg::Tick`. `pub`: `cox run --loop`
/// (T27.6, headless counterpart of `/loop`) reuses this grammar rather
/// than parsing intervals a second way.
pub fn parse_interval(s: &str) -> Option<Duration> {
    let (digits, mult) = match s.strip_suffix('h') {
        Some(n) => (n, 3600),
        None => match s.strip_suffix('m') {
            Some(n) => (n, 60),
            None => (s.strip_suffix('s').unwrap_or(s), 1),
        },
    };
    let secs: u64 = digits.parse().ok()?;
    (secs > 0).then(|| Duration::from_secs(secs * mult))
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

/// `Shift+Tab`: default → plan → auto → default (§1.13).
pub fn next_mode(mode: PermissionMode) -> PermissionMode {
    match mode {
        PermissionMode::Default => PermissionMode::Plan,
        PermissionMode::Plan => PermissionMode::Auto,
        PermissionMode::Auto | PermissionMode::Bypass => PermissionMode::Default,
    }
}

/// `/autocompact`'s line: the threshold and the config layer that set it —
/// `source` is `cox config show --sources`' `source_of` layer name.
pub fn autocompact(compact_at: f64, source: &str) -> String {
    let layer = match source {
        "project" => "project config",
        "user" => "user config",
        other => other,
    };
    format!("compact_at = {compact_at} ({layer})")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T25.3: `!` runs a shell line, `!!` also hands its output to the model.
    #[test]
    fn bang_lines_parse_to_shell_with_share_on_double_bang() {
        let p = |line| parse(line, Tier::Code);
        let shell = |cmd: &str, share| {
            Some(Action::Shell {
                cmd: cmd.into(),
                share,
            })
        };
        assert_eq!(p("!ls -la"), shell("ls -la", false));
        assert_eq!(p("!! git status "), shell("git status", true));
        assert!(matches!(p("!"), Some(Action::Notice(_))));
        assert!(matches!(p("!! "), Some(Action::Notice(_))));
    }

    /// T26.3: `/fork` takes an optional turn in either spelling the
    /// timeline uses; `/handoff` needs its objective.
    #[test]
    fn fork_and_handoff_parse_their_arguments() {
        let p = |line| parse(line, Tier::Code);
        assert_eq!(p("/fork"), Some(Action::Fork(None)));
        assert_eq!(p("/fork T3"), Some(Action::Fork(Some(3))));
        assert_eq!(p("/fork 3"), Some(Action::Fork(Some(3))));
        assert!(matches!(p("/fork later"), Some(Action::Notice(_))));
        assert!(matches!(p("/fork 0"), Some(Action::Notice(_))));
        assert_eq!(
            p("/handoff ship the parser"),
            Some(Action::Handoff("ship the parser".into()))
        );
        assert!(matches!(p("/handoff"), Some(Action::Notice(_))));
    }

    /// T24.6: `docs/getting-started.md`'s keymap table is `KEYMAP`, row for
    /// row, and ends where `KEYMAP` ends (a blank line follows it).
    #[test]
    fn keymap_table_matches_docs() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/getting-started.md");
        let doc = std::fs::read_to_string(path).expect("getting-started.md");
        let mut table = String::from("| Key | Action | Context |\n| --- | --- | --- |\n");
        for (key, action, ctx) in KEYMAP {
            table.push_str(&format!("| `{key}` | {action} | {} |\n", ctx.name()));
        }
        assert!(
            doc.contains(&format!("{table}\n")),
            "docs/getting-started.md's keymap table differs from KEYMAP; expected:\n{table}"
        );
    }

    /// T27.4: `/loop` takes an interval, a prompt and an optional trailing
    /// `--budget`; `/loop stop` and a malformed call parse separately.
    #[test]
    fn loop_parses_interval_prompt_budget_and_stop() {
        let p = |line| parse(line, Tier::Code);
        assert_eq!(
            p("/loop 5m fix the flaky test"),
            Some(Action::LoopStart {
                interval: Duration::from_secs(300),
                prompt: "fix the flaky test".into(),
                budget_usd: None,
            })
        );
        assert_eq!(
            p("/loop 30s ping --budget 2.5"),
            Some(Action::LoopStart {
                interval: Duration::from_secs(30),
                prompt: "ping".into(),
                budget_usd: Some(2.5),
            })
        );
        assert_eq!(p("/loop stop"), Some(Action::LoopStop));
        assert!(matches!(p("/loop 5m"), Some(Action::Notice(_))));
        assert!(matches!(p("/loop soon go"), Some(Action::Notice(_))));
    }

    /// T25.7: `/autocompact` names the project config layer, the same data
    /// `cox config show --sources` prints.
    #[test]
    fn autocompact_line_names_the_project_config_layer() {
        assert_eq!(
            autocompact(0.75, "project"),
            "compact_at = 0.75 (project config)"
        );
    }
}
