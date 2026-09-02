//! T3.5 step 5: the golden corpus. Each `fixtures/v4a/<case>.patch` is
//! applied to a copy of `<case>.before/` and the result must equal
//! `<case>.after/`, byte for byte and file for file.
//!
//! Driven through the real `Tool` surface rather than `v4a::apply::stage`,
//! because the corpus is the only place `confine`, the archive write and
//! the atomic rename are exercised together — the unit tests in
//! `src/v4a/apply.rs` deliberately stop at the pure staging step.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cox_protocol::{
    Archive, ArchiveId, ArchivePut, CallId, SandboxMode, SandboxPolicy, SessionId, StoreError,
    Tool, ToolCx,
};
use cox_tools::v4a::ApplyPatchTool;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

struct NoopArchive;

#[async_trait::async_trait]
impl Archive for NoopArchive {
    async fn put(&self, _put: ArchivePut) -> Result<ArchiveId, StoreError> {
        Ok(ArchiveId::new())
    }
    async fn get(&self, _id: &ArchiveId) -> Result<Vec<u8>, StoreError> {
        Ok(Vec::new())
    }
}

fn cx(root: PathBuf) -> ToolCx {
    let (tx, _rx) = mpsc::channel(16);
    ToolCx {
        roots: vec![root.clone()],
        cwd: root,
        sandbox: SandboxPolicy {
            mode: SandboxMode::WorkspaceWrite,
            network: false,
            writable: vec![],
            readonly_in_workspace: vec![],
        },
        archive: Arc::new(NoopArchive),
        cancel: CancellationToken::new(),
        output: tx,
        session: SessionId::new(),
        call: CallId::new(),
    }
}

/// Every file under `root`, keyed by its path relative to `root`. Content is
/// bytes so a fixture that differs only in a trailing newline still fails.
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let (Ok(rel), Ok(bytes)) = (path.strip_prefix(root), std::fs::read(&path)) {
                out.insert(rel.display().to_string(), bytes);
            }
        }
    }
    out
}

fn copy_tree(from: &Path, to: &Path) {
    for (rel, bytes) in tree(from) {
        let dest = to.join(&rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(&dest, bytes).expect("copy");
    }
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/v4a")
}

#[test]
fn v4a_golden_corpus_applies_every_patch() {
    let dir = fixtures();
    let mut cases: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("fixtures/v4a must exist")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "patch"))
        .collect();
    cases.sort();
    assert_eq!(cases.len(), 25, "the corpus is 25 golden patches");

    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime");

    for case in cases {
        let name = case
            .file_stem()
            .expect("stem")
            .to_string_lossy()
            .to_string();
        let patch = std::fs::read_to_string(&case).expect("read patch");
        let work = tempfile::tempdir().expect("tempdir");
        copy_tree(&dir.join(format!("{name}.before")), work.path());

        let out = rt
            .block_on(ApplyPatchTool.call(
                serde_json::json!({ "patch": patch }),
                &cx(work.path().to_path_buf()),
            ))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!out.is_error, "{name}: tool reported an error");

        assert_eq!(
            tree(work.path()),
            tree(&dir.join(format!("{name}.after"))),
            "{name}: applied tree does not match the golden `.after/`"
        );
    }
}
