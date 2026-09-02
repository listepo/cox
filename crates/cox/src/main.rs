//! The clap surface and dispatch — nothing else. `config` (T0.3) and
//! `doctor` (T0.5) are wired up; every other subcommand is a stub until its
//! task lands (T2.x run, ...) — each prints a notice and exits 0 rather than
//! erroring, so the binary is a stable target for scripts and CI while the
//! crate fills in.

mod cli;
mod config_cmd;
mod config_load;
mod doctor;
mod record;
mod resume;
mod stats;

use clap::Parser;
use cli::{Cli, Command, ConfigAction};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cwd = match &cli.cwd {
        Some(dir) => dir.clone(),
        None => std::env::current_dir().unwrap_or_default(),
    };

    match &cli.command {
        Some(Command::Config(args)) => run_config(&cwd, &cli, &args.action),
        Some(Command::Doctor) => {
            std::process::exit(doctor::run(cli.json));
        }
        Some(Command::Record(args)) => record::run(&cli, args),
        Some(Command::Stats(args)) => {
            if let Some(session_id) = &args.session {
                let home = cli.home.as_deref().unwrap_or_else(|| &cwd);
                stats::run(home, session_id)?;
                Ok(())
            } else {
                println!("error: --session <ID> is required");
                std::process::exit(1);
            }
        }
        Some(Command::Run(args)) => resume::run(&cli, args),
        Some(_) => {
            println!("not implemented");
            Ok(())
        }
        None => {
            println!("not implemented");
            Ok(())
        }
    }
}

fn run_config(cwd: &std::path::Path, cli: &Cli, action: &ConfigAction) -> anyhow::Result<()> {
    match action {
        ConfigAction::Show { sources } => {
            let loaded = config_load::load(cwd, cli)?;
            config_cmd::show(&loaded, *sources);
            Ok(())
        }
        ConfigAction::Get { key } => {
            let loaded = config_load::load(cwd, cli)?;
            match config_cmd::get(&loaded, key) {
                Some(value) => {
                    println!("{value}");
                    Ok(())
                }
                None => Err(anyhow::anyhow!("no such config key: {key}")),
            }
        }
        ConfigAction::Set { key, value } => {
            let path = config_cmd::set(key, value)?;
            println!("{}", path.display());
            Ok(())
        }
        ConfigAction::Path => {
            println!("{}", config_cmd::path().display());
            Ok(())
        }
    }
}
