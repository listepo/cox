//! Rebuild history from a store rollout for `cox run --resume` (T2.4).
//! `--continue` (latest session for this cwd) waits on a store listing API.

use std::path::Path;

use cox_core::History;
use cox_protocol::ids::SessionId;
use cox_protocol::traits::Store as _;
use cox_store::Store;

use crate::cli::{Cli, RunArgs};
use crate::config_load;

/// Reconstructs history for `--resume <id>` or the current directory's latest
/// session for `--continue`.
pub fn run(cli: &Cli, args: &RunArgs) -> anyhow::Result<()> {
    let home = cli.home.clone().unwrap_or_else(config_load::cox_home);
    let id = if args.r#continue {
        let cwd = cli.cwd.clone().unwrap_or(std::env::current_dir()?);
        Store::open(&home)?
            .latest_session_for_cwd(&cwd)?
            .to_string()
    } else {
        let Some(id) = args.resume.as_deref() else {
            println!("not implemented");
            return Ok(());
        };
        id.to_string()
    };
    let history = from_home(&home, &id)?;
    println!("{} messages", history.messages.len());
    Ok(())
}

/// Reads a session's rollout from `home` and rebuilds [`History`].
pub fn from_home(home: &Path, id: &str) -> anyhow::Result<History> {
    let store = Store::open(home)?;
    let id: SessionId = id.parse()?;
    let (events, truncated) = store.rollout_read_with_truncation(&id)?;
    Ok(History::from_rollout(&events, truncated))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use cox_protocol::Event;
    use cox_protocol::ids::{SessionId, TurnId};
    use cox_protocol::traits::Store as _;
    use cox_protocol::types::StopReason;

    use super::*;

    #[test]
    fn resume_truncated_tail_keeps_events_and_warns() {
        let home = tempfile::tempdir().expect("home");
        let store = Store::open(home.path()).expect("store");
        let id = SessionId::new();
        store
            .rollout_append(
                &id,
                &Event::TurnDone {
                    turn: TurnId::new(),
                    stop: StopReason::EndTurn,
                },
            )
            .expect("append");
        let path = home.path().join("sessions").join(format!("{id}.jsonl"));
        fs::write(
            &path,
            [fs::read(&path).expect("read"), b"{\"event\":".to_vec()].concat(),
        )
        .expect("truncate tail");

        let history = from_home(home.path(), &id.to_string()).expect("resume");
        assert!(history.truncated);
        assert!(history.truncated_notice().is_some());
    }
}
