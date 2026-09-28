//! The session owner end to end (T37.39): `App` opens a real session built
//! by cox-session in a scratch `COX_HOME` over the Scripted provider, and
//! `LiveSession` runs intents as the macOS app sends them through
//! `cox-ffi`. The host is in memory, never the real Keychain (A49); nextest
//! runs each test in its own process, so each sets its own environment.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use cox_app::TimelinePatch;
use cox_app::app::{App, AppError, Host};
use cox_app::live::LiveSession;
use cox_app::{
    BlockId, BlockKind, CheckId, CheckStatus, FileChange, InboxItem, Intent, Layer, Need,
};
use cox_protocol::types::{Decision, StopReason};

/// Reads `notes.md`, then replies in markdown.
const READ_AND_REPLY: &str = r#"
[[turn]]
text = "Reading the notes."
tool_calls = [{ name = "read", input = { path = "notes.md" } }]

[[turn]]
text = "The file says **hello**."
"#;

/// A write the default policy asks about.
const WRITE: &str = r#"
[[turn]]
text = "Writing it."
tool_calls = [{ name = "write", input = { path = "out.txt", content = "approved\n" } }]

[[turn]]
text = "Done."
"#;

/// Two plain replies: one per turn.
const TWO_REPLIES: &str = r#"
[[turn]]
text = "One."

[[turn]]
text = "Two."
"#;

/// Edits `notes.md` and creates `new.rs` in one turn: one checkpoint.
const TWO_EDITS: &str = r#"
[[turn]]
text = "Changing two files."
tool_calls = [
  { name = "edit", input = { path = "notes.md", old = "hello", new = "bye" } },
  { name = "write", input = { path = "new.rs", content = "fn main() {}\n" } },
]

[[turn]]
text = "Done."
"#;

/// The Keychain as a map; remembers what it was asked and told.
#[derive(Default)]
struct MemoryHost {
    secrets: HashMap<String, String>,
    asked: Mutex<Vec<String>>,
    notes: Mutex<Vec<(InboxItem, u32)>>,
    badges: Mutex<Vec<u32>>,
}

impl Host for MemoryHost {
    fn notify(&self, item: InboxItem, badge: u32) {
        self.notes.lock().expect("notes").push((item, badge));
    }
    fn badge(&self, badge: u32) {
        self.badges.lock().expect("badges").push(badge);
    }
    fn open_url(&self, _: &str) {}
    fn secret(&self, section: &str) -> Option<String> {
        self.asked.lock().expect("asked").push(section.into());
        self.secrets.get(section).cloned()
    }
}

/// Points cox at a tempdir (and, with a script, at the Scripted provider)
/// and makes `project/notes.md`.
fn scratch(script: Option<&str>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("user");
    std::fs::create_dir_all(dir.path().join("project")).expect("project");
    std::fs::write(dir.path().join("project/notes.md"), "hello\n").expect("notes");
    // SAFETY: first thing in this test's own process (nextest).
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("COX_HOME", home.join(".cox"));
        std::env::remove_var("ANTHROPIC_API_KEY");
        if let Some(script) = script {
            let path = dir.path().join("scenario.toml");
            std::fs::write(&path, script).expect("scenario");
            std::env::set_var("COX_PROVIDER", "scripted");
            std::env::set_var("COX_SCENARIO", path);
        }
    }
    dir
}

fn app(dir: &Path, host: Arc<MemoryHost>) -> Arc<App> {
    App::new(Some(dir.join("user/.cox")), host).expect("app")
}

async fn open(dir: &Path, host: Arc<MemoryHost>) -> Result<Arc<LiveSession>, AppError> {
    let theme = "base16-ocean.dark".to_string();
    app(dir, host).open(dir.join("project"), None, theme).await
}

fn send(text: &str) -> Intent {
    Intent::Send {
        text: text.into(),
        attachments: vec![],
    }
}

fn ends_turn(patch: &TimelinePatch) -> bool {
    matches!(patch, TimelinePatch::Upsert { block, .. }
        if matches!(block.kind, BlockKind::TurnMeta { stop: Some(_), .. }))
}

/// Pulls until the running turn ends.
async fn finish(session: &LiveSession) {
    while let Some(batch) = session.next_patches().await {
        if batch.iter().any(ends_turn) {
            return;
        }
    }
    panic!("the stream closed before the turn ended");
}

/// The user and assistant texts, in order.
fn texts(session: &LiveSession) -> Vec<String> {
    let blocks = session.snapshot().into_iter().map(|b| b.kind);
    blocks
        .filter_map(|k| match k {
            BlockKind::User { text, .. } | BlockKind::Assistant { text, .. } => Some(text),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_sent_turn_reaches_the_timeline_as_blocks() {
    let dir = scratch(Some(READ_AND_REPLY));
    let session = open(dir.path(), Arc::default()).await.expect("open");
    let returned = session.send(send("read the notes")).await.expect("send");
    assert!(returned.is_none(), "a turn is spawned, not a child");
    finish(&session).await;
    let kinds: Vec<BlockKind> = session.snapshot().into_iter().map(|b| b.kind).collect();
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, BlockKind::Tool { tool, .. } if tool == "read"))
    );
    assert!(kinds.iter().any(|k| matches!(
        k,
        BlockKind::TurnMeta {
            stop: Some(StopReason::EndTurn),
            ..
        }
    )));
    assert_eq!(
        texts(&session),
        [
            "read the notes",
            "Reading the notes.",
            "The file says **hello**."
        ]
    );
}

#[tokio::test]
async fn an_approval_reaches_the_inbox_and_the_host_and_the_intent_answers_it() {
    let dir = scratch(Some(WRITE));
    let host = Arc::new(MemoryHost::default());
    let app = app(dir.path(), Arc::clone(&host));
    let theme = "base16-ocean.dark".to_string();
    let session = app.open(dir.path().join("project"), None, theme).await;
    let session = session.expect("open");
    session.send(send("write it")).await.expect("send");
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
    let asks =
        |item: &InboxItem| matches!(&item.need, Need::Approval { call: c, .. } if c.id == call);
    assert!(matches!(&notes[..], [(item, 1)] if asks(item)), "{notes:?}");
    assert_eq!(app.badge(), 1);
    let approve = Intent::Approve {
        call,
        decision: Decision::Allow,
    };
    session.send(approve).await.expect("approve");
    finish(&session).await;
    let written = std::fs::read_to_string(dir.path().join("project/out.txt"));
    assert_eq!(written.ok().as_deref(), Some("approved\n"));
    assert_eq!(app.badge(), 0);
    assert_eq!(
        *host.badges.lock().expect("badges"),
        [0],
        "the Dock badge falls"
    );
}

#[tokio::test]
async fn fork_opens_the_child_with_the_parents_blocks() {
    let dir = scratch(Some(READ_AND_REPLY));
    let session = open(dir.path(), Arc::default()).await.expect("open");
    session.send(send("read the notes")).await.expect("send");
    finish(&session).await;
    let child = session
        .send(Intent::Fork { turn: None })
        .await
        .expect("fork");
    let child = child.expect("a fork returns its child");
    assert_ne!(child.id(), session.id());
    assert_eq!(texts(&child), texts(&session));
}

#[tokio::test]
async fn resume_reopens_the_session_with_its_blocks() {
    let dir = scratch(Some(READ_AND_REPLY));
    let app = app(dir.path(), Arc::default());
    let (cwd, theme) = (dir.path().join("project"), "base16-ocean.dark");
    let first = app.open(cwd.clone(), None, theme.into()).await;
    let first = first.expect("open");
    first.send(send("read the notes")).await.expect("send");
    finish(&first).await;
    let (id, before) = (first.id(), texts(&first));
    first.end();
    drop(first);
    let resumed = app.open(cwd, Some(id), theme.into()).await;
    let resumed = resumed.expect("resume");
    assert_eq!(resumed.id(), id);
    assert_eq!(texts(&resumed), before);
}

#[tokio::test]
async fn history_is_the_sessions_own_prompts_newest_first() {
    let dir = scratch(Some(TWO_REPLIES));
    let app = app(dir.path(), Arc::default());
    let (cwd, theme) = (dir.path().join("project"), "base16-ocean.dark");
    let other = app.open(cwd.clone(), None, theme.into()).await;
    let other = other.expect("open the other session");
    other.send(send("elsewhere")).await.expect("send");
    finish(&other).await;
    let session = app.open(cwd, None, theme.into()).await.expect("open");
    assert_eq!(session.history(10).expect("history"), Vec::<String>::new());
    for prompt in ["first prompt", "second prompt"] {
        session.send(send(prompt)).await.expect("send");
        finish(&session).await;
    }
    let history = session.history(10).expect("history");
    assert_eq!(history, ["second prompt", "first prompt"]);
    assert_eq!(session.history(1).expect("history"), ["second prompt"]);
}

#[tokio::test]
async fn an_empty_intent_fails_without_touching_the_session() {
    let dir = scratch(Some(READ_AND_REPLY));
    let session = open(dir.path(), Arc::default()).await.expect("open");
    let err = session.send(send("  ")).await.err();
    assert!(matches!(err, Some(AppError::Intent(_))), "{err:?}");
    assert!(texts(&session).is_empty());
}

#[tokio::test]
async fn the_host_supplies_the_provider_key_and_its_absence_fails() {
    let dir = scratch(None);
    let empty = Arc::new(MemoryHost::default());
    let err = open(dir.path(), Arc::clone(&empty)).await.err();
    assert!(matches!(err, Some(AppError::Session(_))), "{err:?}");
    assert_eq!(*empty.asked.lock().expect("asked"), ["anthropic"]);

    let mut keyed = MemoryHost::default();
    keyed.secrets.insert("anthropic".into(), "sk-test".into());
    let session = open(dir.path(), Arc::new(keyed)).await;
    assert!(session.is_ok(), "{:?}", session.err());
}

#[test]
fn the_checklist_asks_the_host_for_the_provider_key() {
    let dir = scratch(None);
    let project = dir.path().join("project");
    let empty = Arc::new(MemoryHost::default());
    let rows = app(dir.path(), Arc::clone(&empty))
        .checklist(&project)
        .expect("checklist");
    assert_eq!(rows[0].id, CheckId::ProviderKey);
    assert_eq!(rows[0].status, CheckStatus::Fail, "{}", rows[0].detail);
    assert_eq!(*empty.asked.lock().expect("asked"), ["anthropic"]);
    // `load_login_env` never ran in this process.
    let shell = rows.last().expect("shell row");
    assert_eq!(
        (shell.id, shell.status),
        (CheckId::ShellEnv, CheckStatus::Warn)
    );

    let mut keyed = MemoryHost::default();
    keyed.secrets.insert("anthropic".into(), "sk-test".into());
    let rows = app(dir.path(), Arc::new(keyed))
        .checklist(&project)
        .expect("checklist");
    assert_eq!(rows[0].status, CheckStatus::Ok, "{}", rows[0].detail);
}

#[tokio::test]
async fn changes_lists_the_edited_and_created_files_and_the_turn_to_rewind_to() {
    let dir = scratch(Some(TWO_EDITS));
    let config = "[permissions]\nallow = [\"edit\", \"write\"]\n";
    std::fs::create_dir_all(dir.path().join("user/.cox")).expect("home");
    std::fs::write(dir.path().join("user/.cox/config.toml"), config).expect("config");
    let session = open(dir.path(), Arc::default()).await.expect("open");
    session.send(send("change them")).await.expect("send");
    finish(&session).await;

    let changes = session.changes().await.expect("changes");
    let mut files = changes.files.clone();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let got: Vec<_> = files
        .iter()
        .map(|f| (f.path.to_str(), f.change, f.added, f.removed, f.turn))
        .collect();
    assert_eq!(
        got,
        [
            (Some("new.rs"), FileChange::Created, 0, 0, 1),
            (Some("notes.md"), FileChange::Edited, 1, 1, 1),
        ]
    );
    let blocks = session.snapshot();
    for file in &files {
        let id = BlockId(format!("call:{}", file.call));
        let tool = blocks.iter().find(|b| b.id == id).map(|b| &b.kind);
        assert!(matches!(tool, Some(BlockKind::Tool { .. })), "{file:?}");
    }
    let [checkpoint] = changes.checkpoints.as_slice() else {
        panic!("one turn changed files: {:?}", changes.checkpoints);
    };
    assert_eq!(checkpoint.turn, 1);
    assert!(
        checkpoint.label.starts_with("Turn 1 · before "),
        "{checkpoint:?}"
    );
    assert!(checkpoint.label.ends_with(" and 1 more"), "{checkpoint:?}");
    assert!(checkpoint.time.starts_with("20"), "{checkpoint:?}");
    assert_eq!(changes.worktree, None, "a tempdir is no linked worktree");
}

#[tokio::test]
async fn info_names_the_session_its_cwd_rollout_and_the_user_config_it_read() {
    let dir = scratch(Some(READ_AND_REPLY));
    let user = dir.path().join("user/.cox/config.toml");
    std::fs::create_dir_all(dir.path().join("user/.cox")).expect("home");
    std::fs::write(&user, "[tui]\nvim = true\n").expect("config");
    let session = open(dir.path(), Arc::default()).await.expect("open");
    session.send(send("read it")).await.expect("send");
    finish(&session).await;

    let info = session.info().await.expect("info");
    assert_eq!(info.session, session.id());
    assert_eq!(info.cwd, dir.path().join("project"));
    assert_eq!(info.worktree, None, "a tempdir is no linked worktree");
    let layers: Vec<_> = info.config.iter().map(|c| c.layer).collect();
    assert_eq!(layers.first(), Some(&Layer::Default), "{layers:?}");
    let from_user = info.config.iter().find(|c| c.layer == Layer::User);
    let from_user = from_user.map(|c| (c.file.clone(), c.keys));
    assert_eq!(from_user, Some((Some(user), 1)));
    let rollout = dir
        .path()
        .join(format!("user/.cox/sessions/{}.jsonl", session.id()));
    assert_eq!(info.rollout, rollout);
    assert!(info.rollout.is_file(), "the turn was appended to it");
}
