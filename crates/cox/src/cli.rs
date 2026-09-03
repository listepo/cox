//! The clap command tree (plan.md §1.12). Kept separate from `main.rs` so
//! `config_load`'s `every_flag_has_a_config_key` test and the flag→config-key
//! table (`config_load::flag_key_map`) can inspect `Cli::command()` without
//! pulling in dispatch logic.

use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand};

/// Root command: `cox [PROMPT] [subcommand] [global flags]`.
#[derive(Parser, Debug)]
#[command(name = "cox", version, about = "cox — a modular terminal coding agent")]
pub struct Cli {
    /// First-turn prompt for the interactive TUI (stub: ignored for now).
    pub prompt: Option<String>,

    #[command(subcommand)]
    pub command: Option<Command>,

    // ---- Global flags (plan.md §1.12 "Global:" row) ----
    /// Override the `code` tier's provider.
    #[arg(long, global = true, value_name = "NAME")]
    pub provider: Option<String>,
    /// Override the `code` tier's model.
    #[arg(long, global = true, value_name = "ID")]
    pub model: Option<String>,
    /// Override one tier's model, `TIER=MODEL` (repeatable).
    #[arg(long, global = true, value_name = "TIER=MODEL")]
    pub tier: Vec<String>,
    /// Override `sandbox.mode`. `danger-full-access` is flag-only.
    #[arg(long, global = true, value_name = "MODE")]
    pub sandbox: Option<String>,
    /// Override `permissions.mode`. `bypass` is flag-only and shows a persistent banner.
    #[arg(long = "permission-mode", global = true, value_name = "MODE")]
    pub permission_mode: Option<String>,
    /// Override `permissions.approval`.
    #[arg(long, global = true, value_name = "POLICY")]
    pub approve: Option<String>,
    /// Override `budget.session_usd`.
    #[arg(long, global = true, value_name = "USD")]
    pub budget: Option<f64>,
    /// Run as if started from this directory.
    #[arg(long, global = true, value_name = "DIR")]
    pub cwd: Option<PathBuf>,
    /// Add an extra workspace root (repeatable).
    #[arg(long = "add-dir", global = true, value_name = "DIR")]
    pub add_dir: Vec<PathBuf>,
    /// Override `core.home` (same effect as the `COX_HOME` env var).
    #[arg(long, global = true, value_name = "DIR")]
    pub home: Option<PathBuf>,
    /// Increase log verbosity: `-v` debug, `-vv` trace.
    #[arg(short = 'v', long = "verbose", global = true, action = ArgAction::Count)]
    pub verbose: u8,
    /// Machine-readable output where supported.
    #[arg(long, global = true)]
    pub json: bool,
    /// Disable hooks for this invocation.
    #[arg(long = "no-hooks", global = true)]
    pub no_hooks: bool,
    /// Disable MCP servers for this invocation.
    #[arg(long = "no-mcp", global = true)]
    pub no_mcp: bool,
}

/// Top-level subcommands (plan.md §1.12). Only `Run` and `Config` are
/// implemented past their clap shape in T0.3; the rest print `not
/// implemented` until their own task lands.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Headless run: `cox run -p <prompt>`.
    Run(RunArgs),
    /// List / search rollouts.
    Sessions,
    /// Print archived tool output by id.
    Expand(ExpandArgs),
    /// Usage and cost stats.
    Stats(StatsArgs),
    /// Read or write config.
    Config(ConfigArgs),
    /// Report why cox will or will not work on this machine.
    Doctor,
    /// Re-record a provider cassette.
    Record(RecordArgs),
    /// Serve built-in tools over MCP stdio.
    Mcp(McpArgs),
    /// Agent Client Protocol server on stdio.
    Acp,
    /// Instruction files, skills, commands, agents, hooks, MCP servers in effect.
    Ext,
    /// Self-update the binary.
    #[command(name = "self")]
    SelfUpdate,
}

/// `cox expand <id>` (plan.md T2.5).
#[derive(Args, Debug)]
pub struct ExpandArgs {
    /// Archive id printed alongside a truncated tool result.
    pub id: String,
    /// Optional 1-based inclusive line range (`START-END`).
    #[arg(long)]
    pub lines: Option<String>,
}

/// `cox run` (plan.md §1.12). Only the clap shape and the flag→config-key
/// mapping land in T0.3; execution is T2.x.
#[derive(Args, Debug)]
pub struct RunArgs {
    /// The prompt to run headlessly.
    #[arg(short = 'p', long = "prompt", value_name = "TEXT")]
    pub prompt: Option<String>,
    /// Output shape: `text` | `json` | `stream-json`.
    #[arg(long = "output-format", value_name = "FORMAT")]
    pub output_format: Option<String>,
    /// Cap provider calls for this run (overrides `core.max_turns`).
    #[arg(long = "max-turns", value_name = "N")]
    pub max_turns: Option<u32>,
    /// Comma-separated tool allow-list for this run.
    #[arg(long = "allowed-tools", value_name = "A,B")]
    pub allowed_tools: Option<String>,
    /// Pre-supplied answer to the next approval prompt, if one is asked.
    #[arg(long, value_name = "TEXT")]
    pub answer: Option<String>,
    /// Continue the most recent session.
    #[arg(long, conflicts_with = "resume")]
    pub r#continue: bool,
    /// Resume a specific session by id.
    #[arg(long, value_name = "ID")]
    pub resume: Option<String>,
    /// Route this run's `plan` job through the `think` tier.
    #[arg(long)]
    pub deep: bool,
}

/// `cox stats` (plan.md §1.12/T1.7). Print usage and cost statistics.
#[derive(Args, Debug)]
pub struct StatsArgs {
    /// Print stats for a specific session by id.
    #[arg(long, value_name = "ID")]
    pub session: Option<String>,
    /// Only cache diagnostics: per-turn read ratio plus cache-miss turns.
    #[arg(long)]
    pub cache: bool,
}

/// `cox mcp [--allow-write] [--tools a,b]` (plan.md T6.2): read-only tools
/// by default; writes are opt-in and `bash` only by name.
#[derive(Args, Debug, Default)]
pub struct McpArgs {
    /// Also serve `edit`, `write` and `apply_patch`.
    #[arg(long)]
    pub allow_write: bool,
    /// Serve exactly these tools (comma-separated); the only way to get `bash`.
    #[arg(long, value_name = "A,B")]
    pub tools: Option<String>,
}

/// `cox record <name> -p <prompt> [--sse FILE] [--redact]` (plan.md T1.5).
#[derive(Args, Debug)]
pub struct RecordArgs {
    /// Cassette name (`cassettes/<name>/` under `COX_HOME`).
    pub name: String,
    /// The prompt recorded as the request's user message.
    #[arg(short = 'p', long = "prompt", value_name = "TEXT")]
    pub prompt: Option<String>,
    /// Redact `sk-` keys and `Bearer ` prefixes.
    #[arg(long)]
    pub redact: bool,
    /// Raw SSE body to store (live capture waits on a session loop).
    #[arg(long, value_name = "FILE")]
    pub sse: Option<PathBuf>,
}

/// `cox config <action>` (plan.md §1.12/§1.6).
#[derive(Args, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

/// `cox config` subcommands.
#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Print every effective config key.
    Show {
        /// Also print which layer set each key (`default|user|project|env|flag`).
        #[arg(long)]
        sources: bool,
    },
    /// Print one config key's effective value.
    Get {
        /// Dotted config key, e.g. `tiers.code.model`.
        key: String,
    },
    /// Set one config key in the user config file (preserves comments).
    Set {
        /// Dotted config key, e.g. `tiers.code.model`.
        key: String,
        /// The new value, parsed as TOML (so `5`, `true`, `"text"`, `[1,2]` all work).
        value: String,
    },
    /// Print the user config file path.
    Path,
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn config_cli_parses_run_and_config_subcommands() {
        let cli = Cli::parse_from(["cox", "--model", "x", "run", "-p", "hi", "--deep"]);
        assert_eq!(cli.model.as_deref(), Some("x"));
        match cli.command {
            Some(Command::Run(run)) => {
                assert_eq!(run.prompt.as_deref(), Some("hi"));
                assert!(run.deep);
            }
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn config_cli_command_builds_without_panicking() {
        // `debug_assert()` catches clap arg-definition mistakes (duplicate
        // ids, conflicting short/long names, ...) that only surface when the
        // `Command` is actually built.
        Cli::command().debug_assert();
    }
}
