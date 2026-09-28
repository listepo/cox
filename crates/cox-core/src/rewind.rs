//! `/rewind` (T26.2): put the workspace, the conversation or both back to
//! the start of an earlier turn, or one file back to before a turn
//! (T37.28.3). Separate from `checkpoint.rs` because that
//! module records and this one replays; from `compact.rs` because a rewind
//! is the user's cut, not the budget's. Two rules hold here: the rollout is
//! append-only (a `Rewound` marker is emitted, nothing earlier is edited —
//! `rollout::History` honours the marker on resume), and every file the
//! rewind writes is itself checkpointed first, so a rewind can be undone.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use cox_protocol::errors::{CoreError, ToolError};
use cox_protocol::types::{CheckpointKind, Event, Level};
use cox_protocol::{CheckpointRow, SkipReason, SkippedFile};

use crate::checkpoint;
use crate::session::{Session, State};

impl Session {
    /// Handles `Submission::Rewind`. Never fails the session: a rewind that
    /// cannot run says why in a `Notice` and changes nothing.
    pub(crate) async fn rewind(
        &self,
        to_turn: u32,
        code: bool,
        conversation: bool,
    ) -> Result<(), CoreError> {
        let refusal = match self.refusal(to_turn).await {
            None if !code && !conversation => {
                Some("nothing to rewind: pick code, conversation or both".into())
            }
            refusal => refusal,
        };
        if let Some(text) = refusal {
            return self
                .emit(Event::Notice {
                    level: Level::Warn,
                    text: format!("rewind: {text}"),
                })
                .await;
        }
        let (restored, skipped) = if code {
            self.restore_files(to_turn, None).await?
        } else {
            (Vec::new(), Vec::new())
        };
        if conversation {
            self.cut_history(to_turn).await;
        }
        self.emit(Event::Rewound {
            to_turn,
            code,
            conversation,
            restored: restored.clone(),
            skipped: skipped.clone(),
        })
        .await?;
        let mut parts = Vec::new();
        if code {
            parts.push(format!("{} files restored", restored.len()));
            parts.extend(skip_counts(&skipped));
        }
        if conversation {
            parts.push("conversation cut".into());
        }
        self.emit(Event::Notice {
            level: Level::Info,
            text: format!("rewound to T{to_turn}: {}", parts.join(", ")),
        })
        .await
    }

    /// Handles `Submission::RevertFile` (T37.28.3): a code rewind of one
    /// file. The path goes through the checkpointer's `preimages`, which
    /// confines it exactly as a tool would (`cox_sandbox::path::confine`)
    /// and yields the confined path the checkpoint rows are keyed by; a
    /// path it drops is outside the workspace roots or unreadable. Emits
    /// `Rewound` (code only), so `/redo` and every surface treat it as the
    /// rewind it is.
    pub(crate) async fn revert_file(&self, path: &str, to_turn: u32) -> Result<(), CoreError> {
        let warn = |text: String| Event::Notice {
            level: Level::Warn,
            text: format!("revert: {text}"),
        };
        if let Some(text) = self.refusal(to_turn).await {
            return self.emit(warn(text)).await;
        }
        let Some(cp) = self.checkpointer() else {
            let text = "no checkpoints in this session; files left as they are";
            return self.emit(warn(text.into())).await;
        };
        let confined = cp
            .preimages(self.writable_roots(), &self.cwd, &[path.to_string()])
            .await
            .into_iter()
            .next()
            .map(|p| p.path);
        let Some(confined) = confined else {
            return self
                .emit(warn(format!(
                    "{path} is outside the workspace roots or cannot be read"
                )))
                .await;
        };
        let (restored, skipped) = self.restore_files(to_turn, Some(&confined)).await?;
        if restored.is_empty() && skipped.is_empty() {
            let text = format!("{path} has no checkpoint from T{to_turn} on; left as it is");
            return self.emit(warn(text)).await;
        }
        let notice = if skipped.is_empty() {
            Event::Notice {
                level: Level::Info,
                text: format!("reverted {path} to before T{to_turn}"),
            }
        } else {
            warn(format!(
                "{path} not restored: {}",
                skip_counts(&skipped).join(", ")
            ))
        };
        self.emit(Event::Rewound {
            to_turn,
            code: true,
            conversation: false,
            restored,
            skipped,
        })
        .await?;
        self.emit(notice).await
    }

    /// Why a rewind or revert to `to_turn` cannot run now, if it cannot.
    async fn refusal(&self, to_turn: u32) -> Option<String> {
        let inner = self.inner.lock().await;
        let last = inner.turn_seq;
        if inner.state != State::Idle {
            Some("a turn is running; interrupt it first".into())
        } else if to_turn == 0 || to_turn > last {
            Some(format!("no turn T{to_turn}; this session has T1..T{last}"))
        } else {
            None
        }
    }

    /// Handles `Submission::Redo` (T26.4): a rewind writes the files it is
    /// about to overwrite under a turn of its own, so rewinding code to that
    /// turn puts them back. Only when that rewind is the last thing that
    /// happened — a user turn since has its own marker — and was not itself
    /// a redo, so `/redo` twice does not toggle.
    pub(crate) async fn redo(&self) -> Result<(), CoreError> {
        let rows = self
            .store
            .checkpoint_list(&self.id)
            .map_err(|error| CoreError::Store { error })?;
        let last = rows
            .iter()
            .rev()
            .find(|r| r.kind == CheckpointKind::Turn)
            .map(|r| r.turn);
        let redone = self.inner.lock().await.redone;
        let target = last.filter(|&seq| {
            let mut own = rows
                .iter()
                .filter(|r| r.turn == seq && r.kind != CheckpointKind::Turn)
                .peekable();
            redone != Some(seq) && own.peek().is_some() && own.all(|r| r.call.is_none())
        });
        let Some(seq) = target else {
            return self
                .emit(Event::Notice {
                    level: Level::Warn,
                    text:
                        "redo: nothing to redo; only the step right after /undo or /rewind can be"
                            .into(),
                })
                .await;
        };
        self.rewind(seq, true, false).await?;
        let mut inner = self.inner.lock().await;
        inner.redone = Some(inner.turn_seq);
        Ok(())
    }

    /// Writes the earliest pre-image of every file touched since `to_turn`
    /// back, under a fresh turn number so the rewind's own pre-images make
    /// it undoable; with `only`, just that confined path. Returns
    /// `(restored, skipped)`.
    async fn restore_files(
        &self,
        to_turn: u32,
        only: Option<&Path>,
    ) -> Result<(Vec<PathBuf>, Vec<SkippedFile>), CoreError> {
        let Some(cp) = self.checkpointer() else {
            self.emit(Event::Notice {
                level: Level::Warn,
                text: "rewind: no checkpoints in this session; files left as they are".into(),
            })
            .await?;
            return Ok((Vec::new(), Vec::new()));
        };
        let rows = self
            .store
            .checkpoint_list(&self.id)
            .map_err(|error| CoreError::Store { error })?;
        // Insertion order is chronological; the first row per path is the
        // state before `to_turn` touched it.
        let mut seen = HashSet::new();
        let targets: Vec<CheckpointRow> = rows
            .into_iter()
            .filter(|r| r.turn >= to_turn && r.kind != CheckpointKind::Turn)
            .filter(|r| only.is_none_or(|p| r.path == p))
            .filter(|r| seen.insert(r.path.clone()))
            .collect();
        if targets.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        let seq = {
            let mut inner = self.inner.lock().await;
            inner.turn_seq += 1;
            inner.turn_seq
        };
        checkpoint::mark_turn(self, seq);
        let roots = self.writable_roots();
        let mut restored = Vec::new();
        let mut skipped = Vec::new();
        for row in targets {
            let bytes = match (row.kind, row.archive) {
                (CheckpointKind::Created, _) => None,
                (_, Some(id)) => match self.archive.get(&id).await {
                    Ok(bytes) => Some(bytes),
                    Err(e) => {
                        let error = e.to_string();
                        skipped.push(skip(row.path, SkipReason::Failed { error }));
                        continue;
                    }
                },
                // A pre-image over the size cap was recorded without bytes.
                (_, None) => {
                    skipped.push(skip(row.path, SkipReason::TooLarge));
                    continue;
                }
            };
            let current = cp
                .preimages(roots, &self.cwd, &[row.path.to_string_lossy().into_owned()])
                .await
                .into_iter()
                .map(|p| (p.path, false, p.before))
                .collect();
            checkpoint::store_rows(self, seq, None, current).await;
            match cp
                .restore(roots, &self.cwd, &row.path, bytes.as_deref())
                .await
            {
                Ok(()) => restored.push(row.path),
                Err(e) => skipped.push(skip(row.path, skip_reason(&e))),
            }
        }
        Ok((restored, skipped))
    }

    /// Drops the in-memory history from `to_turn` on. The rollout keeps
    /// every line; `Rewound` tells resume where to stop.
    async fn cut_history(&self, to_turn: u32) {
        let mut inner = self.inner.lock().await;
        let Some(at) = inner.turn_marks.iter().position(|m| m.seq >= to_turn) else {
            // Everything from `to_turn` on was already compacted away.
            return;
        };
        let start = inner.turn_marks[at].start;
        inner.history.truncate(start);
        inner.turn_marks.truncate(at);
    }
}

fn skip(path: PathBuf, reason: SkipReason) -> SkippedFile {
    SkippedFile { path, reason }
}

/// What a failed `Checkpointer::restore` means to the user.
fn skip_reason(error: &ToolError) -> SkipReason {
    match error {
        ToolError::Confined { .. } => SkipReason::OutsideRoots,
        ToolError::TooLarge { .. } => SkipReason::TooLarge,
        other => SkipReason::Failed {
            error: other.to_string(),
        },
    }
}

/// The notice's skipped parts, one per reason in a fixed order:
/// `2 too large to restore, 1 failed: <first error>`.
fn skip_counts(skipped: &[SkippedFile]) -> Vec<String> {
    let count = |want: fn(&SkipReason) -> bool| skipped.iter().filter(|s| want(&s.reason)).count();
    let large = count(|r| matches!(r, SkipReason::TooLarge));
    let outside = count(|r| matches!(r, SkipReason::OutsideRoots));
    let failed = count(|r| matches!(r, SkipReason::Failed { .. }));
    let first_error = skipped.iter().find_map(|s| match &s.reason {
        SkipReason::Failed { error } => Some(error.as_str()),
        _ => None,
    });
    let mut parts = Vec::new();
    if large > 0 {
        parts.push(format!("{large} too large to restore"));
    }
    if outside > 0 {
        parts.push(format!("{outside} outside the workspace roots"));
    }
    if let (true, Some(error)) = (failed > 0, first_error) {
        parts.push(format!("{failed} failed: {error}"));
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_confined_restore_is_outside_the_roots_and_an_io_error_failed() {
        let confined = ToolError::Confined {
            path: PathBuf::from("/etc/passwd"),
            root: PathBuf::from("/w"),
        };
        assert_eq!(skip_reason(&confined), SkipReason::OutsideRoots);
        assert_eq!(
            skip_reason(&ToolError::Io),
            SkipReason::Failed {
                error: "io error".into()
            }
        );
    }

    #[test]
    fn skipped_files_are_counted_by_reason() {
        let skipped = [
            skip("a".into(), SkipReason::TooLarge),
            skip("b".into(), SkipReason::OutsideRoots),
            skip("c".into(), SkipReason::TooLarge),
            skip(
                "d".into(),
                SkipReason::Failed {
                    error: "io error".into(),
                },
            ),
            skip(
                "e".into(),
                SkipReason::Failed {
                    error: "other".into(),
                },
            ),
        ];
        assert_eq!(
            skip_counts(&skipped),
            [
                "2 too large to restore",
                "1 outside the workspace roots",
                "2 failed: io error"
            ]
        );
        assert!(skip_counts(&[]).is_empty());
    }
}
