//! `/rewind` (T26.2): put the workspace, the conversation or both back to
//! the start of an earlier turn. Separate from `checkpoint.rs` because that
//! module records and this one replays; from `compact.rs` because a rewind
//! is the user's cut, not the budget's. Two rules hold here: the rollout is
//! append-only (a `Rewound` marker is emitted, nothing earlier is edited —
//! `rollout::History` honours the marker on resume), and every file the
//! rewind writes is itself checkpointed first, so a rewind can be undone.

use std::collections::HashSet;
use std::path::PathBuf;

use cox_protocol::CheckpointRow;
use cox_protocol::errors::CoreError;
use cox_protocol::types::{CheckpointKind, Event, Level};

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
        let (state, last) = {
            let inner = self.inner.lock().await;
            (inner.state, inner.turn_seq)
        };
        let refusal = if state != State::Idle {
            Some("a turn is running; interrupt it first".to_string())
        } else if to_turn == 0 || to_turn > last {
            Some(format!("no turn T{to_turn}; this session has T1..T{last}"))
        } else if !code && !conversation {
            Some("nothing to rewind: pick code, conversation or both".into())
        } else {
            None
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
            self.restore_files(to_turn).await?
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
            if !skipped.is_empty() {
                parts.push(format!("{} too large to restore", skipped.len()));
            }
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

    /// Writes the earliest pre-image of every file touched since `to_turn`
    /// back, under a fresh turn number so the rewind's own pre-images make
    /// it undoable. Returns `(restored, skipped)`.
    async fn restore_files(&self, to_turn: u32) -> Result<(Vec<PathBuf>, Vec<PathBuf>), CoreError> {
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
                    Err(_) => {
                        skipped.push(row.path);
                        continue;
                    }
                },
                // A pre-image over the size cap was recorded without bytes.
                (_, None) => {
                    skipped.push(row.path);
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
                Err(_) => skipped.push(row.path),
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
