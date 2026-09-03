//! The clap surface and dispatch — nothing else. `config` (T0.3) and
//! `doctor` (T0.5) are wired up; every other subcommand is a stub until its
//! task lands (T2.x run, ...) — each prints a notice and exits 0 rather than
//! erroring, so the binary is a stable target for scripts and CI while the
//! crate fills in.

mod acp_cmd;
mod cli;
mod config_cmd;
mod config_load;
mod doctor;
mod expand_cmd;
mod ext_cmd;
mod mcp_cmd;
mod record;
mod resume;
mod run;
mod self_update;
mod session;
mod sessions;
mod stats;

use clap::Parser;
use cli::{Cli, Command, ConfigAction};

fn main() -> anyhow::Result<()> {
    load_dotenv()?;
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
            let home = cli.home.as_deref().unwrap_or_else(|| &cwd);
            stats::run(home, args)?;
            Ok(())
        }
        Some(Command::Mcp(args)) => mcp_cmd::run(&cli, args, &cwd),
        Some(Command::Ext(args)) => match &args.action {
            None => {
                print!("{}", ext_cmd::report(&cli, &cwd));
                Ok(())
            }
            Some(crate::cli::ExtAction::List { json }) => {
                print!("{}", ext_cmd::list(&cli, &cwd, *json));
                Ok(())
            }
        },
        Some(Command::Run(args)) => std::process::exit(run::run(&cli, args, &cwd)?),
        Some(Command::Acp) => acp_cmd::run(&cli, &cwd),
        Some(Command::Sessions(args)) => {
            let home = cli.home.clone().unwrap_or_else(config_load::cox_home);
            sessions::run(&home, args)?;
            Ok(())
        }
        Some(Command::Expand(args)) => {
            let home = cli.home.as_deref().unwrap_or_else(|| &cwd);
            expand_cmd::run(home, &args.id, args.lines.as_deref())
        }
        Some(Command::SelfUpdate(args)) => {
            let version = match &args.action {
                Some(crate::cli::SelfUpdateAction::Update { version }) => version.clone(),
                // Bare `cox self` updates to latest, like `update` without a tag.
                None => None,
            };
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(self_update::run(version))?;
            Ok(())
        }
        None => session::run_tui(&cli, &cwd),
    }
}

/// Loads local secrets and `COX_*` overrides before clap reads the process
/// environment. `from_filename` walks upward from the process cwd and never
/// overwrites a value supplied by the shell or CI; `.env.local` therefore
/// fills only still-unset keys after `.env`.
fn load_dotenv() -> anyhow::Result<()> {
    for filename in [".env", ".env.local"] {
        if let Err(error) = dotenvy::from_filename(filename)
            && !error.not_found()
        {
            return Err(error.into());
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    fn config_cli() -> Cli {
        Cli::parse_from(["cox", "config", "show"])
    }

    #[test]
    fn config_dotenv_fills_unset_cox_key() {
        let home = tempdir().expect("home tempdir");
        let env_file = tempdir().expect("dotenv tempdir");
        let cwd = tempdir().expect("cwd tempdir");
        let path = env_file.path().join(".env");
        fs::write(&path, "COX_TIERS_CODE_MODEL=dotenv-model\n").expect("write dotenv");

        crate::config_load::temp_env(
            &[
                ("COX_HOME", Some(home.path().to_str().expect("utf-8 home"))),
                ("COX_TIERS_CODE_MODEL", None),
            ],
            || {
                dotenvy::from_path(&path).expect("load dotenv");
                let loaded =
                    crate::config_load::load(cwd.path(), &config_cli()).expect("load config");
                assert_eq!(loaded.config.tiers.code.model, "dotenv-model");
                assert_eq!(loaded.source_of("tiers.code.model"), "env");
            },
        );
    }

    #[test]
    fn config_dotenv_does_not_override_set_env() {
        let home = tempdir().expect("home tempdir");
        let env_file = tempdir().expect("dotenv tempdir");
        let cwd = tempdir().expect("cwd tempdir");
        let path = env_file.path().join(".env");
        fs::write(&path, "COX_TIERS_CODE_MODEL=dotenv-model\n").expect("write dotenv");

        crate::config_load::temp_env(
            &[
                ("COX_HOME", Some(home.path().to_str().expect("utf-8 home"))),
                ("COX_TIERS_CODE_MODEL", Some("shell-model")),
            ],
            || {
                dotenvy::from_path(&path).expect("load dotenv");
                let loaded =
                    crate::config_load::load(cwd.path(), &config_cli()).expect("load config");
                assert_eq!(loaded.config.tiers.code.model, "shell-model");
                assert_eq!(loaded.source_of("tiers.code.model"), "env");
            },
        );
    }
}
