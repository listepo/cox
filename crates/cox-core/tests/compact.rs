//! Compaction integration tests (T8.1 §1.10).
//!
//! Names use `compact_` so `cargo test -p cox-core compact_` matches them;
//! the plan's `compaction_…` names do not contain that substring.

mod common;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use common::{drain, spawn_turn};
use cox_core::{History, MemoryStore, Session};
use cox_protocol::errors::ProviderError;
use cox_protocol::traits::{Provider, Store as _};
use cox_protocol::types::{
    Caps, Event, ItemKind, Job, ModelId, ProviderEvent, ProviderId, Request, Submission, Usage,
};
use cox_provider::scripted::Scripted;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const THREE_PLUS_SUMMARY: &str = concat!(
    "[[turn]]\ntext = \"first\"\n",
    "[[turn]]\ntext = \"second\"\n",
    "[[turn]]\ntext = \"third\"\n",
    "[[turn]]\ntext = \"## Goal\\nstuff\\n## Decisions\\nnone\\n## Files touched\\na.rs\\n## Open todo\\nnone\\n## Errors seen\\nnone\\n## Next step\\ngo\"\n",
);

fn open_three() -> (Session, Arc<MemoryStore>, mpsc::Receiver<Event>) {
    let config = cox_protocol::Config::default();
    let provider = Arc::new(Scripted::from_toml(THREE_PLUS_SUMMARY, "").expect("scenario"));
    let store = Arc::new(MemoryStore::new());
    let session = Session::new(
        config,
        provider,
        common::tools(),
        store.clone(),
        store.clone(),
        PathBuf::from("/tmp/cox-turn"),
    )
    .expect("session");
    let rx = session.events().expect("events once");
    (session, store, rx)
}

async fn user_turn(session: &Session, rx: &mut mpsc::Receiver<Event>, text: &str) -> Vec<Event> {
    let running = spawn_turn(session, text);
    let events = drain(rx).await;
    running.await.expect("join").expect("turn");
    events
}

async fn three_turns() -> (Session, Arc<MemoryStore>, mpsc::Receiver<Event>) {
    let (session, store, mut rx) = open_three();
    for t in ["t0", "t1", "t2"] {
        user_turn(&session, &mut rx, t).await;
    }
    (session, store, rx)
}

#[tokio::test]
async fn compact_keeps_last_two_turns_verbatim() {
    let (session, _, mut rx) = three_turns().await;
    let before = session.history().await;
    assert_eq!(before.len(), 6, "3 turns x user+assistant");
    session
        .submit(Submission::Compact { focus: None })
        .await
        .expect("compact");
    // Compaction emits ItemStarted/Done + Compacted, no TurnDone.
    let mut saw_compacted = false;
    while let Ok(ev) = rx.try_recv() {
        if matches!(ev, Event::Compacted { .. }) {
            saw_compacted = true;
        }
    }
    assert!(saw_compacted, "Compacted event emitted");
    let after = session.history().await;
    assert_eq!(after.len(), 5, "summary + 2 kept turns");
    assert!(matches!(
        &after[0].content[0],
        cox_protocol::types::Content::Text { text } if text.contains("stuff")
    ));
    assert_eq!(&after[1..], &before[2..], "kept turns byte-identical");
}

#[tokio::test]
async fn compact_is_append_only_in_rollout() {
    use cox_protocol::ids::SessionId;
    let (session, store, _) = three_turns().await;
    let dummy = SessionId::new();
    // MemoryStore ignores the session key, so a dummy id reads everything.
    let n_before = store.rollout_read(&dummy).expect("read").len();
    session
        .submit(Submission::Compact { focus: None })
        .await
        .expect("compact");
    let events = store.rollout_read(&dummy).expect("read");
    assert!(events.len() > n_before, "rollout only grows");
    let compacted = events
        .iter()
        .find_map(|e| match e {
            Event::Compacted { dropped, .. } => Some(dropped.clone()),
            _ => None,
        })
        .expect("Compacted in rollout");
    assert_eq!(compacted.len(), 1, "3 turns keep 2, drop 1");
    // Original user items are still in the rollout.
    let users = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                Event::ItemStarted {
                    kind: ItemKind::UserMessage { .. },
                    ..
                }
            )
        })
        .count();
    assert!(users >= 3, "dropped turn still on disk");
    // Rebuild matches live memory.
    let rebuilt = History::from_events(&events).messages;
    assert_eq!(rebuilt, session.history().await);
}

#[tokio::test]
async fn compact_request_after_compaction_keeps_cached_prefix() {
    let (session, _, _) = three_turns().await;
    let tools = common::tools();
    let prefix_of = |h: &[cox_protocol::types::Message]| {
        cox_core::assemble(
            h,
            &cox_protocol::Config::default(),
            &tools,
            std::path::Path::new("/tmp/cox-turn"),
            "",
        )
        .system[..=2]
            .iter()
            .map(|b| b.text.clone())
            .collect::<Vec<_>>()
    };
    let before = prefix_of(&session.history().await);
    session
        .submit(Submission::Compact { focus: None })
        .await
        .expect("compact");
    assert_eq!(before, prefix_of(&session.history().await));
}

/// Records the summariser's system prompt so the focus test sees it.
struct Probe {
    system: Mutex<String>,
}

#[async_trait]
impl Provider for Probe {
    fn id(&self) -> ProviderId {
        ProviderId::Local
    }
    fn capabilities(&self) -> Caps {
        Caps {
            cache: false,
            thinking: false,
            server_tools: false,
            count_tokens: true,
            max_context: u32::MAX,
        }
    }
    async fn stream(
        &self,
        req: Request,
        sink: mpsc::Sender<ProviderEvent>,
        _cancel: CancellationToken,
    ) -> Result<Usage, ProviderError> {
        if req.job == Job::Compact {
            *self.system.lock().expect("lock") = req.system[0].text.clone();
        }
        let text = if req.job == Job::Compact {
            "compacted-summary"
        } else {
            "ok"
        };
        for ev in [
            ProviderEvent::MessageStart {
                model: ModelId("probe".into()),
            },
            ProviderEvent::TextDelta { text: text.into() },
            ProviderEvent::Stop {
                stop: cox_protocol::types::StopReason::EndTurn,
            },
            ProviderEvent::Usage {
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                    cache_read_tokens: 0,
                    cache_write_tokens: 0,
                    estimated: true,
                    cost_usd: 0.0,
                    latency_ms: 0,
                },
            },
        ] {
            sink.send(ev).await.map_err(|_| ProviderError::Cancelled)?;
        }
        Ok(Usage {
            input_tokens: 10,
            output_tokens: 5,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            estimated: true,
            cost_usd: 0.0,
            latency_ms: 0,
        })
    }
    async fn count_tokens(&self, _req: &Request) -> Result<u32, ProviderError> {
        Ok(10)
    }
}

#[tokio::test]
async fn compact_focus_is_passed_to_summarizer() {
    let probe = Arc::new(Probe {
        system: Mutex::new(String::new()),
    });
    let store = Arc::new(MemoryStore::new());
    let session = Session::new(
        cox_protocol::Config::default(),
        probe.clone(),
        common::tools(),
        store.clone(),
        store.clone(),
        PathBuf::from("/tmp/cox-turn"),
    )
    .expect("session");
    let mut rx = session.events().expect("events");
    for t in ["a", "b", "c"] {
        user_turn(&session, &mut rx, t).await;
    }
    session
        .submit(Submission::Compact {
            focus: Some("auth flow".into()),
        })
        .await
        .expect("compact");
    assert!(
        probe.system.lock().expect("lock").contains("auth flow"),
        "focus reaches summariser system prompt"
    );
}

// (MemoryStore ignores the session key, so tests read with a dummy id.)
