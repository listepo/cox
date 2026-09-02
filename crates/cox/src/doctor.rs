//! T0.4 stub for `cox doctor`: does the store open under `COX_HOME`? T0.5
//! (plan.md §0.5) replaces `run` with the full check list (toolchain,
//! `COX_HOME` writable, API keys, sandbox backend, `git`, terminal, prices
//! table age, `.claude/settings.json`); this only proves the wiring cox-store
//! needs is in place.

use std::path::PathBuf;

use cox_protocol::Store as _;

/// Resolves `COX_HOME` (env override, else `cox_store::Store::default_home`),
/// opens the store, and prints one `db: ok` / `db: fail <reason>` line.
pub fn run() {
    let home = std::env::var("COX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| cox_store::Store::default_home());

    match cox_store::Store::open(&home) {
        Ok(_) => println!("db: ok"),
        Err(e) => println!("db: fail {e}"),
    }
}
