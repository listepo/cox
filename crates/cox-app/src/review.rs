//! Review's diff of one changed file (DT§5.4, T37.28.2, A101): the net
//! difference between the copy the session's first checkpoint of it holds
//! and the file on disk now, not the model's calls one by one, so after a
//! code-only rewind it shows what is left rather than what was undone.
//! Separate from `changes.rs`, which lists the files and their turns from
//! what the session recorded; this reads one file's bytes on request.

use std::path::Path;

use cox_protocol::CheckpointRow;
use cox_protocol::types::CheckpointKind;
use cox_protocol::{ArchiveId, Before};
use cox_render::diffmodel::{self, DiffModel};

/// The first row that recorded `path` (confined, so canonical): the file as
/// it was before this session changed it. Rows are in insertion order.
pub fn base<'a>(rows: &'a [(CheckpointRow, String)], path: &Path) -> Option<&'a CheckpointRow> {
    rows.iter()
        .map(|(row, _)| row)
        .find(|row| row.kind != CheckpointKind::Turn && row.path == path)
}

/// What `row` kept of the file: nothing for one the session created, the
/// archived bytes, or [`Before::TooLarge`] for a copy over the size cap.
pub fn kept<E>(
    row: &CheckpointRow,
    archive: impl FnOnce(&ArchiveId) -> Result<Vec<u8>, E>,
) -> Result<Before, E> {
    Ok(match (row.kind, &row.archive) {
        (CheckpointKind::Created, _) => Before::Absent,
        (_, Some(id)) => Before::Bytes(archive(id)?),
        (_, None) => Before::TooLarge,
    })
}

/// `shown`'s change from `before` to `now`: no hunks when they are equal,
/// `None` when a side is over the size cap.
pub fn diff(shown: &Path, before: &Before, now: &Before, theme: &str) -> Option<DiffModel> {
    let text = |side: &Before| match side {
        Before::Absent => Some(String::new()),
        Before::Bytes(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        Before::TooLarge => None,
    };
    Some(diffmodel::between(
        shown,
        &text(before)?,
        &text(now)?,
        theme,
    ))
}
