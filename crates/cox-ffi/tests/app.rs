//! The exported surface end to end (T37.14): a real session built by
//! cox-session in a scratch `COX_HOME`, driven through `App` and
//! `SessionHandle` as Swift drives them, with the in-memory host from the
//! fixture recorder. nextest runs each test in its own process, so each
//! sets its own environment.

#[path = "../examples/record.rs"]
#[allow(dead_code)]
mod record;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cox_app::{BlockKind, Intent, Need, TimelinePatch};
use cox_ffi::AppError;
use cox_protocol::types::{Decision, StopReason};
use record::{MemoryHost, ends_turn, open};

fn scenario(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("fixtures/{name}.toml"))
}

fn scratch(scenario: Option<&Path>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    // SAFETY: first thing in this test's own process (nextest).
    unsafe { record::scratch_env(dir.path(), scenario) };
    dir
}

/// Pulls until the running turn ends.
async fn finish(session: &Arc<cox_ffi::SessionHandle>) -> Vec<TimelinePatch> {
    let mut seen = Vec::new();
    while let Some(batch) = session.next_patches().await {
        let done = batch.iter().any(ends_turn);
        seen.extend(batch);
        if done {
            return seen;
        }
    }
    panic!("the stream closed before the turn ended: {seen:?}");
}

#[tokio::test]
async fn a_scripted_turn_reaches_the_app_as_blocks_and_the_meter() {
    let dir = scratch(Some(&scenario("read-and-reply")));
    let session = open(dir.path(), Arc::default()).await.expect("open");
    let prompts = ["read the notes".to_string()];
    let recording = record::record(&session, &prompts).await.expect("record");
    assert!(
        recording["batches"]
            .as_array()
            .is_some_and(|b| !b.is_empty())
    );

    let kinds: Vec<BlockKind> = session.snapshot().into_iter().map(|b| b.kind).collect();
    let user =
        |k: &BlockKind| matches!(k, BlockKind::User { text, .. } if text == "read the notes");
    assert!(kinds.iter().any(user), "{kinds:?}");
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, BlockKind::Tool { tool, .. } if tool == "read"))
    );
    assert!(kinds.iter().any(
        |k| matches!(k, BlockKind::Assistant { text, doc } if text.contains("**hello**") && !doc.blocks.is_empty())
    ));
    assert!(kinds.iter().any(|k| matches!(
        k,
        BlockKind::TurnMeta {
            stop: Some(StopReason::EndTurn),
            ..
        }
    )));
    let batches = &recording["batches"];
    let usage = batches.to_string().contains(r#""op":"usage""#);
    assert!(usage, "the meter's patch crosses too: {batches}");
}

#[tokio::test]
async fn an_approval_notifies_the_host_and_the_intent_answers_it() {
    let dir = scratch(Some(&scenario("write-approved")));
    let host = Arc::new(MemoryHost::default());
    let session = open(dir.path(), Arc::clone(&host)).await.expect("open");
    let send = Intent::Send {
        text: "write it".into(),
        attachments: vec![],
    };
    Arc::clone(&session).send(send).await.expect("send");
    let call = loop {
        let batch = session.next_patches().await.expect("open stream");
        let pending = batch.iter().find_map(|p| match p {
            TimelinePatch::Upsert { block, .. } => match &block.kind {
                BlockKind::Approval {
                    call,
                    decision: None,
                    ..
                } => Some(*call),
                _ => None,
            },
            _ => None,
        });
        if let Some(call) = pending {
            break call;
        }
    };
    let notes = host.notes.lock().expect("notes").clone();
    assert!(
        matches!(&notes[..], [(item, 1)] if matches!(&item.need, Need::Approval { call: c, .. } if c.id == call)),
        "{notes:?}"
    );
    let approve = Intent::Approve {
        call,
        decision: Decision::Allow,
    };
    Arc::clone(&session).send(approve).await.expect("approve");
    finish(&session).await;
    let written = std::fs::read_to_string(dir.path().join("project/out.txt"));
    assert_eq!(written.ok().as_deref(), Some("approved\n"));
}

#[tokio::test]
async fn fork_opens_the_child_with_the_parents_blocks() {
    let dir = scratch(Some(&scenario("read-and-reply")));
    let session = open(dir.path(), Arc::default()).await.expect("open");
    record::record(&session, &["read the notes".to_string()])
        .await
        .expect("turn");
    let fork = Intent::Fork { turn: None };
    let child = Arc::clone(&session).send(fork).await.expect("fork");
    let child = child.expect("a fork returns its child");
    assert_ne!(child.id(), session.id());
    let kinds = |s: &cox_ffi::SessionHandle| -> Vec<String> {
        let blocks = s.snapshot().into_iter().map(|b| b.kind);
        blocks
            .filter_map(|k| match k {
                BlockKind::User { text, .. } | BlockKind::Assistant { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    };
    assert_eq!(kinds(&child), kinds(&session));
}

#[tokio::test]
async fn the_host_keychain_supplies_the_provider_key_and_its_absence_fails() {
    let dir = scratch(None);
    let empty = Arc::new(MemoryHost::default());
    let err = open(dir.path(), Arc::clone(&empty)).await.err();
    assert!(matches!(err, Some(AppError::Session { .. })), "{err:?}");
    assert_eq!(*empty.asked.lock().expect("asked"), ["anthropic"]);

    let mut keyed = MemoryHost::default();
    keyed.secrets.insert("anthropic".into(), "sk-test".into());
    let session = open(dir.path(), Arc::new(keyed)).await;
    assert!(session.is_ok(), "{:?}", session.err());
}
