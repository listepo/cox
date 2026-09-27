//! `cox config show|get|set|path` at their old path (T32.16): the logic
//! lives in `cox_config::cmd`; this module keeps only the printing, because
//! terminal output stays in `crates/cox` and `cox-tui` (AGENTS.md).

pub use cox_config::cmd::{get, path, set};

use crate::config_load::LoadedConfig;

/// `cox config show [--sources]`.
pub fn show(loaded: &LoadedConfig, with_sources: bool) -> anyhow::Result<()> {
    for line in cox_config::cmd::show_lines(loaded, with_sources)? {
        println!("{line}");
    }
    Ok(())
}
