//! `cox expand`: reads an archived tool result from the configured Store.

use std::path::Path;

use cox_protocol::{ArchiveId, Store as _};
use cox_store::Store;

pub fn run(home: &Path, raw_id: &str, lines: Option<&str>) -> anyhow::Result<()> {
    let id: ArchiveId = raw_id.parse()?;
    let bytes = Store::open(home)?.archive_get(&id)?;
    let text = String::from_utf8_lossy(&bytes);
    if let Some((start, end)) = lines.and_then(parse_range) {
        for (index, line) in text.lines().enumerate() {
            let line_no = index + 1;
            if (start..=end).contains(&line_no) {
                println!("{line_no}\t{line}");
            }
        }
    } else {
        print!("{text}");
    }
    Ok(())
}

fn parse_range(raw: &str) -> Option<(usize, usize)> {
    let (start, end) = raw.split_once('-')?;
    let (start, end) = (start.parse().ok()?, end.parse().ok()?);
    (start > 0 && start <= end).then_some((start, end))
}
