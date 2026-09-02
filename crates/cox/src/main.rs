//! The clap surface and dispatch — nothing else. Every subcommand below is
//! a stub until its task lands (T0.3 config, T0.4/T0.5 doctor, T2.x run,
//! ...); each one prints a notice and exits 0 rather than erroring, so the
//! binary is a stable target for scripts and CI while the crate fills in.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cox", version, about = "cox — a modular terminal coding agent")]
struct Cli {
    /// First-turn prompt for the interactive TUI (stub: ignored for now).
    prompt: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Headless run: `cox run -p <prompt>`.
    Run,
    /// List / search rollouts.
    Sessions,
    /// Print archived tool output by id.
    Expand,
    /// Usage and cost stats.
    Stats,
    /// Read or write config.
    Config,
    /// Report why cox will or will not work on this machine.
    Doctor,
    /// Re-record a provider cassette.
    Record,
    /// Serve built-in tools over MCP stdio.
    Mcp,
    /// Agent Client Protocol server on stdio.
    Acp,
    /// Instruction files, skills, commands, agents, hooks, MCP servers in effect.
    Ext,
    /// Self-update the binary.
    #[command(name = "self")]
    SelfUpdate,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Some(_) => println!("not implemented"),
        None => println!("not implemented"),
    }
}
