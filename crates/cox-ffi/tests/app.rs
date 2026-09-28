//! The exported surface end to end (T37.14): a real session in a scratch
//! `COX_HOME`, driven through `App` and `SessionHandle` as Swift drives
//! them, with the in-memory host from the fixture recorder. The session
//! logic itself is `cox-app`'s and tested there (T37.39); these prove the
//! forwarding, the runtime, the host bridge and the error mapping. nextest
//! runs each test in its own process, so each sets its own environment.

#[path = "../examples/record.rs"]
#[allow(dead_code)]
mod record;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cox_app::BlockKind;
use cox_ffi::AppError;
use cox_protocol::types::StopReason;
use record::{MemoryHost, open};

fn scenario(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("fixtures/{name}.toml"))
}

fn scratch(scenario: Option<&Path>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    // SAFETY: first thing in this test's own process (nextest).
    unsafe { record::scratch_env(dir.path(), scenario) };
    dir
}

#[tokio::test]
async fn a_scripted_turn_reaches_the_app_as_blocks_and_the_meter() {
    let dir = scratch(Some(&scenario("read-and-reply")));
    let host = Arc::new(MemoryHost::default());
    let session = open(dir.path(), Arc::clone(&host)).await.expect("open");
    let prompts = ["read the notes".to_string()];
    let recording = record::record(&session, &host, &prompts)
        .await
        .expect("record");
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

#[tokio::test]
async fn an_approval_is_noted_with_badge_one_and_allowing_it_resumes_the_turn() {
    let dir = scratch(Some(&scenario("approve-write")));
    let host = Arc::new(MemoryHost::default());
    let session = open(dir.path(), Arc::clone(&host)).await.expect("open");
    let prompts = ["write a summary".to_string()];
    let recording = record::record(&session, &host, &prompts)
        .await
        .expect("record");

    let notes = recording["notes"].as_array().cloned().unwrap_or_default();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(notes[0]["badge"], 1);
    assert_eq!(notes[0]["item"]["need"]["type"], "approval");
    let kinds: Vec<BlockKind> = session.snapshot().into_iter().map(|b| b.kind).collect();
    let allowed = |k: &BlockKind| {
        matches!(
            k,
            BlockKind::Approval {
                decision: Some(_),
                ..
            }
        )
    };
    assert!(kinds.iter().any(allowed), "{kinds:?}");
    let ended = |k: &BlockKind| {
        matches!(
            k,
            BlockKind::TurnMeta {
                stop: Some(StopReason::EndTurn),
                ..
            }
        )
    };
    assert!(kinds.iter().any(ended), "{kinds:?}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("project/summary.md")).ok(),
        Some("hello\n".into())
    );
}
