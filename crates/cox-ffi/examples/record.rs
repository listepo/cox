//! Fixture recorder (T37.14, DT§8): plays a Scripted scenario through the
//! same `App` and `SessionHandle` the macOS app links and writes the patch
//! batches it pulled, plus the final snapshot, as JSON for the Swift tests.
//! Runs in a scratch `COX_HOME` and `HOME`, never the real `~/.cox` or
//! Keychain. The tests reuse its host and `record` (`tests/app.rs`).
//!
//! `cargo run -p cox-ffi --example record -- <scenario.toml> <out.json> <prompt>...`

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use cox_app::{BlockKind, InboxItem, Intent, TimelinePatch};
use cox_ffi::{App, AppError, AppHost, OpenRequest, SessionHandle};

/// The Keychain as a map; remembers what it was asked and told.
#[derive(Default)]
pub struct MemoryHost {
    pub secrets: HashMap<String, String>,
    pub asked: Mutex<Vec<String>>,
    pub notes: Mutex<Vec<(InboxItem, u32)>>,
}

impl AppHost for MemoryHost {
    fn notify(&self, item: InboxItem, badge: u32) {
        self.notes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((item, badge));
    }
    fn open_url(&self, _: String) {}
    fn secret(&self, section: String) -> Option<String> {
        let found = self.secrets.get(&section).cloned();
        self.asked
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(section);
        found
    }
}

/// Points cox at `dir` and, with a scenario, at the Scripted provider.
///
/// # Safety
/// Mutates the process environment: call before any other thread runs.
pub unsafe fn scratch_env(dir: &Path, scenario: Option<&Path>) {
    let home = dir.join("user");
    // SAFETY: the caller's contract.
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("COX_HOME", home.join(".cox"));
        std::env::remove_var("ANTHROPIC_API_KEY");
        if let Some(scenario) = scenario {
            std::env::set_var("COX_PROVIDER", "scripted");
            std::env::set_var("COX_SCENARIO", scenario);
        }
    }
}

/// A new session in `dir/project`, which holds the `notes.md` the
/// scenarios read.
pub async fn open(dir: &Path, host: Arc<MemoryHost>) -> Result<Arc<SessionHandle>, AppError> {
    let cwd = dir.join("project");
    std::fs::create_dir_all(&cwd).map_err(|e| AppError::Session {
        message: e.to_string(),
    })?;
    std::fs::write(cwd.join("notes.md"), "hello\n").map_err(|e| AppError::Session {
        message: e.to_string(),
    })?;
    let home = dir.join("user/.cox").to_string_lossy().into_owned();
    let app = App::new(Some(home), host)?;
    let request = OpenRequest {
        cwd: cwd.to_string_lossy().into_owned(),
        resume: None,
        theme: "base16-ocean.dark".into(),
    };
    app.open(request).await
}

/// Sends each prompt and pulls batches until its turn ends.
pub async fn record(
    session: &Arc<SessionHandle>,
    prompts: &[String],
) -> Result<serde_json::Value, AppError> {
    let mut batches = Vec::new();
    for text in prompts {
        let send = Intent::Send {
            text: text.clone(),
            attachments: vec![],
        };
        Arc::clone(session).send(send).await?;
        while let Some(batch) = Arc::clone(session).next_patches().await {
            let done = batch.iter().any(ends_turn);
            batches.push(batch);
            if done {
                break;
            }
        }
    }
    Ok(serde_json::json!({ "batches": batches, "snapshot": session.snapshot() }))
}

pub fn ends_turn(patch: &TimelinePatch) -> bool {
    matches!(patch, TimelinePatch::Upsert { block, .. }
        if matches!(block.kind, BlockKind::TurnMeta { stop: Some(_), .. }))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [scenario, out, prompts @ ..] = args.as_slice() else {
        return Err("usage: record <scenario.toml> <out.json> <prompt>...".into());
    };
    let dir = tempfile::tempdir()?;
    let scenario = std::fs::canonicalize(scenario)?;
    // SAFETY: nothing else runs yet; the runtime starts with the first call.
    unsafe { scratch_env(dir.path(), Some(&scenario)) };
    let session = open(dir.path(), Arc::default()).await?;
    let recording = record(&session, prompts).await?;
    session.end();
    let text = serde_json::to_string_pretty(&recording)?;
    // Scratch paths vary per run; the fixture names the project `/project`.
    let mut text = text;
    // The canonical `/private/var/…` first: it contains the other.
    for root in [std::fs::canonicalize(dir.path())?, dir.path().to_path_buf()] {
        text = text.replace(root.to_string_lossy().as_ref(), "");
    }
    std::fs::write(out, text + "\n")?;
    Ok(())
}
