//! Session titles (A113, DT G6): after a top-level session's first turn,
//! when `[session] auto_title` is on, one `title` job names the session
//! from the first prompt and `Event::TitleSet` carries the name to every
//! surface and, through the rollout, to `sessions.title`. Separate from the
//! turn loop because it is a side request after the turn, not part of it:
//! it never touches the history or the cache-stable prefix, and a failure
//! is logged and skipped.

use cox_protocol::types::{Event, Job};

use crate::session::Session;
use crate::side::Side;

/// The `title` job's whole instruction.
const TITLE_PROMPT: &str = "Write a title for the coding session that starts with the request \
    below: at most six words, plain text, no quotes, no trailing period. Reply with the title only.";

/// How much of the first prompt the title job reads; a pasted log or file
/// would cost tokens without changing the name.
const MAX_PROMPT_CHARS: usize = 2000;

/// The longest title kept; a model that ignores "six words" is cut here.
const MAX_TITLE_CHARS: usize = 80;

/// The model's answer as a one-line title: the first non-empty line with
/// escapes stripped and the quotes, heading marks and trailing period a
/// model adds despite the prompt removed. `None` when nothing is left.
pub(crate) fn clean(raw: &str) -> Option<String> {
    let line = raw.lines().map(str::trim).find(|line| !line.is_empty())?;
    let line = cox_sanitize::sanitize(line);
    let title = line
        .trim_start_matches('#')
        .trim()
        .trim_matches(|c| matches!(c, '"' | '\'' | '`' | '*' | '“' | '”'))
        .trim()
        .trim_end_matches('.')
        .trim();
    let title: String = title.chars().take(MAX_TITLE_CHARS).collect();
    (!title.is_empty()).then_some(title)
}

impl Session {
    /// `prompt`, kept for [`Session::auto_title`], when the turn about to
    /// run may name the session: a top-level session (subagents run other
    /// jobs), the setting on, and no turn before it. A title the user set
    /// is kept by the store (`TitleSource::User`), so this never asks.
    pub(crate) async fn title_prompt(&self, prompt: &str) -> Option<String> {
        (self.job == Job::Main
            && self.config.session.auto_title
            && self.inner.lock().await.turn_seq == 0)
            .then(|| prompt.chars().take(MAX_PROMPT_CHARS).collect())
    }

    /// Asks the `title` job for a name for `prompt` and emits `TitleSet`,
    /// once the turn is the session's first (a prompt a hook blocked left
    /// none). Fail open: every failure is logged and skipped.
    pub(crate) async fn auto_title(&self, prompt: &str) {
        if self.inner.lock().await.turn_seq != 1 {
            return;
        }
        let Some(raw) = self
            .side_call(Side {
                job: Job::Title,
                turn: 1,
                system: TITLE_PROMPT,
                text: prompt,
                max_tokens: 256,
                max_chars: MAX_TITLE_CHARS * 4,
            })
            .await
        else {
            return;
        };
        let Some(title) = clean(&raw) else {
            tracing::warn!("title job answered with no title");
            return;
        };
        if let Err(error) = self.emit(Event::TitleSet { title }).await {
            tracing::warn!(error = %error, "session title not recorded");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;

    use cox_protocol::types::Submission;
    use cox_provider::scripted::Scripted;
    use tokio::sync::mpsc;

    use crate::session::MemoryStore;

    const SCENARIO: &str = "[[turn]]\ntext = \"one\"\n\
        [[turn]]\njob = \"title\"\ntext = \"\\\"Fix the ledger.\\\"\"\n\
        [[turn]]\ntext = \"two\"\n";

    fn session(auto_title: bool) -> (Session, Arc<MemoryStore>, mpsc::Receiver<Event>) {
        let mut config = cox_protocol::Config::default();
        config.session.auto_title = auto_title;
        let provider = Arc::new(Scripted::from_toml(SCENARIO, "").expect("scenario"));
        let store = Arc::new(MemoryStore::new());
        let session = Session::new(
            config,
            provider,
            vec![],
            store.clone(),
            store.clone(),
            PathBuf::from("/tmp/cox-title"),
        )
        .expect("session");
        let rx = session.events().expect("events once");
        (session, store, rx)
    }

    /// Runs one user turn to completion and returns every title it
    /// emitted; a text-only turn fits the event channel, so no drain task.
    async fn turn(session: &Session, rx: &mut mpsc::Receiver<Event>, text: &str) -> Vec<String> {
        session
            .submit(Submission::UserTurn {
                text: text.to_owned(),
                attachments: vec![],
                confirm_think: false,
            })
            .await
            .expect("turn");
        let mut titles = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            if let Event::TitleSet { title } = ev {
                titles.push(title);
            }
        }
        titles
    }

    #[test]
    fn clean_keeps_the_first_line_without_quotes_or_period() {
        assert_eq!(
            clean("\n  \"Fix the ledger.\"\nmore").as_deref(),
            Some("Fix the ledger")
        );
        assert_eq!(clean("# Title\u{1b}[31m").as_deref(), Some("Title"));
        assert_eq!(clean(" \"\" "), None);
    }

    #[tokio::test]
    async fn auto_title_is_emitted_once_after_the_first_turn() {
        let (session, store, mut rx) = session(true);
        let first = turn(&session, &mut rx, "fix the ledger sum").await;
        assert_eq!(first, vec!["Fix the ledger".to_string()]);
        assert!(turn(&session, &mut rx, "and test it").await.is_empty());
        let jobs: Vec<Job> = store.usage_rows().into_iter().map(|row| row.job).collect();
        assert_eq!(jobs.iter().filter(|job| **job == Job::Title).count(), 1);
    }

    #[tokio::test]
    async fn auto_title_off_emits_no_title() {
        let (session, _store, mut rx) = session(false);
        assert!(
            turn(&session, &mut rx, "fix the ledger sum")
                .await
                .is_empty()
        );
        assert!(turn(&session, &mut rx, "and test it").await.is_empty());
    }
}
