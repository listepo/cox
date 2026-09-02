//! Rebuild history from a store rollout for `cox run --resume` (T2.4).
//! `--continue` (latest session for this cwd) waits on a store listing API.

use std::path::Path;

use cox_core::History;
use cox_protocol::ids::SessionId;
use cox_protocol::traits::Store as _;
use cox_store::Store;

use crate::cli::{Cli, RunArgs};
use crate::config_load;

/// Reconstructs history for `--resume <id>`; `--continue` is not listed yet.
pub fn run(cli: &Cli, args: &RunArgs) -> anyhow::Result<()> {
    if args.r#continue {
        println!("not implemented");
        return Ok(());
    }
    let Some(id) = args.resume.as_deref() else {
        println!("not implemented");
        return Ok(());
    };
    let home = cli.home.clone().unwrap_or_else(config_load::cox_home);
    let history = from_home(&home, id)?;
    println!("{} messages", history.messages.len());
    Ok(())
}

/// Reads a session's rollout from `home` and rebuilds [`History`].
pub fn from_home(home: &Path, id: &str) -> anyhow::Result<History> {
    let store = Store::open(home)?;
    let id: SessionId = id.parse()?;
    let events = store.rollout_read(&id)?;
    Ok(History::from_events(&events))
}
