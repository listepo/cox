//! The inspector's Changes tab (DT§5.1, T37.29.1): the files this session
//! changed with their kind, `+n −m` and the call that changed them last; the
//! turns code can be rewound to; the linked worktree it runs in. Built on
//! request from what the session already has — its blocks, the
//! `checkpoints` rows (kinds and times), the rollout's `write` inputs and
//! git — not folded per event,
//! because only an open tab asks. Separate from the timeline fold, which it
//! only reads.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use cox_protocol::CheckpointRow;
use cox_protocol::ids::CallId;
use cox_protocol::types::{CheckpointKind, Event};
use cox_render::diffmodel::{DiffLineKind, DiffModel};
use cox_tools::git::Linked;
use serde::{Deserialize, Serialize};

use crate::patch::{Block, BlockKind};
use crate::timeline::key;

/// What the Changes tab lists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Changes {
    /// In the order they were first changed.
    pub files: Vec<ChangedFile>,
    /// Oldest first.
    pub checkpoints: Vec<Checkpoint>,
    /// `None` when the session runs outside a linked worktree.
    pub worktree: Option<Linked>,
}

/// How the session left a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChange {
    Edited,
    Created,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedFile {
    /// Relative to the session's cwd when inside it.
    pub path: PathBuf,
    pub change: FileChange,
    /// Summed over the calls' diffs; a `write` that created the file adds
    /// all its lines, any other call without a diff (a shell, a `write`
    /// over an existing file) adds nothing.
    pub added: u32,
    pub removed: u32,
    /// The last call that changed it, and that call's turn.
    pub call: CallId,
    pub turn: u32,
}

/// A turn that changed files: rewinding code to it (`Intent::Rewind`'s
/// `to_turn`) restores them to how they were before it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub turn: u32,
    /// `Turn 2 · before retry.rs and 1 more`.
    pub label: String,
    /// RFC 3339: when the turn started, else its first change.
    pub time: String,
}

/// `rows` are the session's checkpoint rows with their times, `written`
/// the line counts from [`written`], `cwd` the directory paths are shown
/// relative to. A row counts only while its call's block is in `blocks`, so
/// a turn rewound out of the conversation drops out here too.
pub fn build(
    blocks: &[Block],
    rows: &[(CheckpointRow, String)],
    written: &HashMap<CallId, usize>,
    cwd: &Path,
    worktree: Option<Linked>,
) -> Changes {
    let diffs: HashMap<_, Option<&DiffModel>> = blocks
        .iter()
        .filter_map(|b| match &b.kind {
            BlockKind::Tool { diff, .. } => Some((&b.id, diff.as_ref())),
            _ => None,
        })
        .collect();
    let starts: HashMap<u32, &str> = rows
        .iter()
        .filter(|(r, _)| r.kind == CheckpointKind::Turn)
        .map(|(r, at)| (r.turn, at.as_str()))
        .collect();
    // Each file with the kind it was first recorded with.
    let mut files: Vec<(ChangedFile, CheckpointKind)> = Vec::new();
    let mut turns: Vec<(u32, String, Vec<String>)> = Vec::new();
    for (row, at) in rows {
        let Some(call) = row.call else { continue };
        let Some(diff) = diffs.get(&key("call", call)) else {
            continue;
        };
        let created = || match row.kind {
            CheckpointKind::Created => (written.get(&call).copied().unwrap_or(0), 0),
            _ => (0, 0),
        };
        let (added, removed) = diff
            .filter(|d| row.path.ends_with(&d.path))
            .map_or_else(created, counts);
        let path = row.path.strip_prefix(cwd).unwrap_or(&row.path);
        let name = path.file_name().unwrap_or(path.as_os_str());
        let name = name.to_string_lossy().into_owned();
        match turns.iter_mut().find(|(turn, ..)| *turn == row.turn) {
            Some((.., names)) if !names.contains(&name) => names.push(name),
            Some(_) => {}
            None => {
                let at = starts.get(&row.turn).copied().unwrap_or(at);
                turns.push((row.turn, at.to_owned(), vec![name]));
            }
        }
        let i = match files.iter().position(|(f, _)| f.path == path) {
            Some(i) => {
                files[i].0.change = change_of(files[i].1, row.kind);
                i
            }
            None => {
                let file = ChangedFile {
                    path: path.to_path_buf(),
                    change: change_of(row.kind, row.kind),
                    added: 0,
                    removed: 0,
                    call,
                    turn: row.turn,
                };
                files.push((file, row.kind));
                files.len() - 1
            }
        };
        let file = &mut files[i].0;
        file.added += count(added);
        file.removed += count(removed);
        (file.call, file.turn) = (call, row.turn);
    }
    Changes {
        // Created and deleted again: nothing is left to show.
        files: files
            .into_iter()
            .filter(|(f, first)| {
                !(*first == CheckpointKind::Created && f.change == FileChange::Deleted)
            })
            .map(|(f, _)| f)
            .collect(),
        checkpoints: turns
            .into_iter()
            .map(|(turn, at, names)| Checkpoint {
                label: label(turn, &names),
                turn,
                time: at,
            })
            .collect(),
        worktree,
    }
}

/// The line count of each `write` call's `content`, from the rollout: a
/// `write` returns no diff, and its input already holds what the created
/// file contains, so the disk (which may have changed since) is not read.
pub fn written(events: &[Event]) -> HashMap<CallId, usize> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::ToolCallRequested { call } if call.name == "write" => {
                let content = call.input.get("content")?.as_str()?;
                Some((call.id, content.lines().count()))
            }
            _ => None,
        })
        .collect()
}

/// A file first recorded as `first` and last as `last`.
fn change_of(first: CheckpointKind, last: CheckpointKind) -> FileChange {
    match (first, last) {
        (_, CheckpointKind::Deleted) => FileChange::Deleted,
        (CheckpointKind::Created, _) => FileChange::Created,
        _ => FileChange::Edited,
    }
}

fn label(turn: u32, names: &[String]) -> String {
    match names {
        [one] => format!("Turn {turn} · before {one}"),
        [first, rest @ ..] => format!("Turn {turn} · before {first} and {} more", rest.len()),
        [] => format!("Turn {turn}"),
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Added and removed lines, from the model the timeline already built.
fn counts(diff: &DiffModel) -> (usize, usize) {
    let lines = diff.hunks.iter().flat_map(|h| &h.lines);
    lines.fold((0, 0), |(add, del), l| match l.kind {
        DiffLineKind::Add => (add + 1, del),
        DiffLineKind::Del => (add, del + 1),
        DiffLineKind::Context => (add, del),
    })
}

#[cfg(test)]
mod tests {
    use cox_protocol::ids::SessionId;
    use cox_protocol::types::Risk;

    use super::*;
    use crate::patch::ToolState;
    use crate::summary::Icon;

    fn tool(call: CallId, turn: u32) -> Block {
        Block {
            id: key("call", call),
            turn,
            kind: BlockKind::Tool {
                tool: "bash".into(),
                summary: String::new(),
                icon: Icon::Shell,
                risk: Risk::Exec,
                state: ToolState::Done,
                tail: String::new(),
                archive: None,
                diff: None,
                duration_ms: 0,
            },
            plugin_view: None,
        }
    }

    fn row(
        turn: u32,
        call: Option<CallId>,
        path: &str,
        kind: CheckpointKind,
    ) -> (CheckpointRow, String) {
        let row = CheckpointRow {
            session: SessionId::new(),
            turn,
            call,
            path: PathBuf::from(path),
            kind,
            archive: None,
        };
        (row, format!("2026-09-28T14:0{turn}:00.000Z"))
    }

    #[test]
    fn a_file_created_then_deleted_and_a_call_rewound_away_leave_no_row() {
        let (a, b, gone) = (CallId::new(), CallId::new(), CallId::new());
        let rows = [
            row(1, None, "", CheckpointKind::Turn),
            row(1, Some(a), "/w/tmp.txt", CheckpointKind::Created),
            row(1, Some(a), "/w/src/lib.rs", CheckpointKind::Pre),
            row(2, Some(b), "/w/tmp.txt", CheckpointKind::Deleted),
            row(3, Some(gone), "/w/src/main.rs", CheckpointKind::Pre),
        ];
        let blocks = [tool(a, 1), tool(b, 2)];
        let changes = build(&blocks, &rows, &HashMap::new(), Path::new("/w"), None);
        let files: Vec<_> = changes
            .files
            .iter()
            .map(|f| (f.path.to_str(), f.change))
            .collect();
        assert_eq!(files, [(Some("src/lib.rs"), FileChange::Edited)]);
        let points: Vec<_> = changes
            .checkpoints
            .iter()
            .map(|c| (c.turn, c.label.as_str(), &c.time[11..16]))
            .collect();
        assert_eq!(
            points,
            [
                (1, "Turn 1 · before tmp.txt and 1 more", "14:01"),
                (2, "Turn 2 · before tmp.txt", "14:02"),
            ]
        );
    }

    #[test]
    fn a_file_a_write_created_counts_its_content_as_added_lines() {
        let (new, over) = (CallId::new(), CallId::new());
        let write = |id, path: &str, content: &str| Event::ToolCallRequested {
            call: cox_protocol::types::ToolCall {
                id,
                name: "write".into(),
                input: serde_json::json!({ "path": path, "content": content }),
                risk: Risk::Write,
                subject: path.into(),
                segments: None,
            },
        };
        let events = [
            write(new, "new.rs", "fn main() {\n    run();\n}\n"),
            write(over, "old.rs", "a\nb\n"),
        ];
        let rows = [
            row(1, Some(new), "/w/new.rs", CheckpointKind::Created),
            row(1, Some(over), "/w/old.rs", CheckpointKind::Pre),
        ];
        let blocks = [tool(new, 1), tool(over, 1)];
        let changes = build(&blocks, &rows, &written(&events), Path::new("/w"), None);
        let files: Vec<_> = changes
            .files
            .iter()
            .map(|f| (f.path.to_str(), f.change, f.added, f.removed))
            .collect();
        assert_eq!(
            files,
            [
                (Some("new.rs"), FileChange::Created, 3, 0),
                (Some("old.rs"), FileChange::Edited, 0, 0),
            ]
        );
    }
}
