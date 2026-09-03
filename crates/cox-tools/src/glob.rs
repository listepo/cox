//! `glob`: find files by name (plan.md T3.3). Walks a confined root with the
//! same `ignore::WalkBuilder` configuration `grep` uses (`.gitignore`
//! honoured, hidden files included — see [`crate::grep::walker`]), keeps the
//! entries a `globset` pattern matches, and returns them newest-first so the
//! files a session has been touching sort to the top.
//!
//! An optional `query` re-ranks that set by `nucleo`'s fuzzy score instead of
//! mtime, which is what makes "the auth handler, wherever it lives" a single
//! call rather than a guess at the path.

use std::path::Path;
use std::time::SystemTime;

use cox_protocol::{Concurrency, Risk};
use cox_protocol::{ToolCx, ToolError, ToolOutput, ToolSpec};
use nucleo::pattern::{CaseMatching, Normalization, Pattern};
use nucleo::{Config, Matcher, Utf32String};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::grep::{glob_allows, walker};
use crate::path;

/// Keeps a bare `**/*` on a large tree from returning every file in the
/// repository; the caller raises it deliberately.
const DEFAULT_LIMIT: usize = 100;

/// `glob`'s JSON input, matching plan.md T3.3 step 2.
#[derive(Debug, Deserialize, JsonSchema)]
struct GlobInput {
    /// Glob matched against each file's path and basename (e.g. `**/*.rs`).
    pattern: String,
    /// Directory to search, relative to a workspace root. Defaults to the
    /// root itself.
    #[serde(default)]
    path: Option<String>,
    /// Fuzzy-rank the matched paths by this query instead of by mtime.
    #[serde(default)]
    query: Option<String>,
    /// Maximum paths to return. Defaults to [`DEFAULT_LIMIT`].
    #[serde(default)]
    limit: Option<usize>,
}

/// A candidate path with the two keys it can be ordered by.
struct Candidate {
    display: String,
    mtime: SystemTime,
}

/// `glob`: name-based file lookup over the gitignore-aware walk.
pub struct GlobTool;

#[async_trait::async_trait]
impl cox_protocol::Tool for GlobTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "glob".into(),
            description: "Find files by glob pattern (.gitignore honoured, hidden files \
                 included). Returns one path per line, most recently modified first. \
                 `query` re-ranks the matches by fuzzy relevance instead of mtime. \
                 Past `limit` paths the list is truncated with a count of the rest."
                .into(),
            input_schema: schemars::schema_for!(GlobInput).to_value(),
            risk: Risk::ReadOnly,
            concurrency: Concurrency::Parallel,
            deferred: false,
        }
    }

    fn subject(&self, input: &Value) -> String {
        input
            .get("pattern")
            .and_then(Value::as_str)
            .unwrap_or("*")
            .to_string()
    }

    async fn call(&self, input: Value, cx: &ToolCx) -> Result<ToolOutput, ToolError> {
        let input: GlobInput = serde_json::from_value(input).map_err(|e| ToolError::Denied {
            why: format!("invalid input: {e}"),
        })?;

        let root = path::confine(&cx.roots, &cx.cwd, input.path.as_deref().unwrap_or("."))?;
        if !root.exists() {
            return Err(ToolError::NotFound);
        }

        let matcher = match globset::Glob::new(&input.pattern) {
            Ok(g) => g.compile_matcher(),
            Err(e) => {
                return Ok(ToolOutput {
                    text: format!("invalid glob: {e}"),
                    is_error: true,
                    diff: None,
                    structured: None,
                });
            }
        };

        let mut found: Vec<Candidate> = Vec::new();
        for entry in walker(&root).build() {
            let Ok(entry) = entry else { continue }; // unreadable dir entry: skip, not fatal
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            let path = entry.path();
            if !glob_allows(&matcher, path, entry.file_name()) {
                continue;
            }
            // A file that vanished between the walk and the stat, or whose
            // mtime the platform withholds, still belongs in the list; it
            // just sorts as oldest.
            let mtime = entry
                .metadata()
                .and_then(|m| m.modified().map_err(Into::into))
                .unwrap_or(SystemTime::UNIX_EPOCH);
            found.push(Candidate {
                display: path.display().to_string(),
                mtime,
            });
        }

        let total = found.len();
        if total == 0 {
            return Ok(ToolOutput {
                text: "no files matched".into(),
                is_error: false,
                diff: None,
                structured: None,
            });
        }

        match input.query.as_deref().filter(|q| !q.is_empty()) {
            Some(query) => rank_by_query(&mut found, query),
            // Newest first: the files this session has been editing lead.
            None => found.sort_by(|a, b| {
                b.mtime
                    .cmp(&a.mtime)
                    .then_with(|| a.display.cmp(&b.display))
            }),
        }

        let limit = input.limit.unwrap_or(DEFAULT_LIMIT);
        let shown = found.len().min(limit);
        let mut text = found
            .iter()
            .take(shown)
            .map(|c| c.display.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if total > shown {
            // Unlike `grep`, the omitted entries are just names already
            // derivable by re-running with a larger limit, so this is a
            // count rather than an archive pointer.
            text.push_str(&format!("\n… {} more (raise `limit`)", total - shown));
        }

        Ok(ToolOutput {
            text,
            is_error: false,
            diff: None,
            structured: None,
        })
    }
}

/// Reorders `found` by `nucleo`'s fuzzy score, best first, dropping paths the
/// query does not match at all.
fn rank_by_query(found: &mut Vec<Candidate>, query: &str) {
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);

    let mut scored: Vec<(u32, Candidate)> = std::mem::take(found)
        .into_iter()
        .filter_map(|c| {
            let haystack = Utf32String::from(c.display.as_str());
            pattern
                .score(haystack.slice(..), &mut matcher)
                .map(|score| (score, c))
        })
        .collect();
    // Ties broken by path so the order is stable across runs.
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.display.cmp(&b.1.display)));
    *found = scored.into_iter().map(|(_, c)| c).collect();
}

/// Every file under `root` the ignore rules allow, relative to it and sorted:
/// the TUI's `@` picker candidates. Same walk as the tool, so what the picker
/// offers is what `glob` would find. The binary calls this and hands the list
/// to `cox-tui`, which may not depend on this crate (plan.md §1.1).
pub fn workspace_files(root: &Path) -> Vec<String> {
    let mut files: Vec<String> = walker(&root.to_path_buf())
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .filter_map(|e| {
            e.path()
                .strip_prefix(root)
                .ok()
                .map(|p| p.display().to_string())
        })
        .collect();
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;

    use cox_protocol::{
        Archive, ArchiveId, ArchivePut, SandboxMode, SandboxPolicy, SessionId, StoreError, Tool,
    };
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use super::*;

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
                mode: SandboxMode::ReadOnly,
                network: false,
                writable: vec![],
                readonly_in_workspace: vec![],
                linux_backend: Default::default(),
            },
            archive: Arc::new(NoopArchive),
            cancel: CancellationToken::new(),
            output: tx,
            session: SessionId::new(),
            call: cox_protocol::CallId::new(),
        }
    }

    /// A tree with a deliberate mtime order — `old.rs` oldest, `new.rs`
    /// newest — plus a gitignored file and a non-matching extension.
    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("src/auth")).expect("mkdir");
        fs::write(root.join(".gitignore"), "ignored.rs\n").expect("write gitignore");
        for rel in [
            "src/old.rs",
            "src/auth/handler.rs",
            "src/new.rs",
            "src/ignored.rs",
            "src/notes.md",
        ] {
            fs::write(root.join(rel), "fn f() {}").expect("write");
        }
        // Stamped rather than inferred from write order: filesystem mtime
        // resolution is coarser than the gap between two `fs::write` calls.
        let base = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        for (rel, offset) in [
            ("src/old.rs", 0),
            ("src/auth/handler.rs", 100),
            ("src/new.rs", 200),
        ] {
            let f = fs::OpenOptions::new()
                .write(true)
                .open(root.join(rel))
                .expect("open for set_modified");
            f.set_modified(base + std::time::Duration::from_secs(offset))
                .expect("set mtime");
        }
        dir
    }

    async fn run(dir: &tempfile::TempDir, input: Value) -> String {
        GlobTool
            .call(input, &cx(dir.path().to_path_buf()))
            .await
            .expect("glob call")
            .text
    }

    #[tokio::test]
    async fn glob_matches_extension_and_sorts_newest_first() {
        let dir = tree();
        let out = run(&dir, serde_json::json!({ "pattern": "*.rs" })).await;
        let names: Vec<&str> = out
            .lines()
            .map(|l| l.rsplit('/').next().unwrap_or(l))
            .collect();
        assert_eq!(
            names,
            vec!["new.rs", "handler.rs", "old.rs"],
            "newest first; .md and the gitignored file excluded: {out}"
        );
    }

    #[tokio::test]
    async fn glob_honours_gitignore() {
        let dir = tree();
        let out = run(&dir, serde_json::json!({ "pattern": "ignored.rs" })).await;
        assert_eq!(
            out, "no files matched",
            "a gitignored file must not surface"
        );
    }

    #[tokio::test]
    async fn glob_query_ranks_by_fuzzy_relevance_not_mtime() {
        let dir = tree();
        // `handler.rs` is the *oldest but one*, so mtime order alone would
        // never put it first; only the query can.
        let out = run(
            &dir,
            serde_json::json!({ "pattern": "*.rs", "query": "authhandler" }),
        )
        .await;
        let first = out.lines().next().unwrap_or_default();
        assert!(
            first.ends_with("auth/handler.rs"),
            "query should rank the auth handler first, got: {out}"
        );
    }

    #[tokio::test]
    async fn glob_limit_truncates_with_a_count_of_the_rest() {
        let dir = tree();
        let out = run(&dir, serde_json::json!({ "pattern": "*.rs", "limit": 1 })).await;
        // Suffix, not prefix: `confine` canonicalises, and on macOS a
        // tempdir's `/var/...` resolves to `/private/var/...`.
        let first = out.lines().next().unwrap_or_default();
        assert!(first.ends_with("src/new.rs"), "got: {out}");
        assert!(
            out.ends_with("… 2 more (raise `limit`)"),
            "truncation must say how many were withheld, got: {out}"
        );
    }

    #[tokio::test]
    async fn glob_outside_the_root_is_confined() {
        let dir = tree();
        let err = GlobTool
            .call(
                serde_json::json!({ "pattern": "*.rs", "path": "../.." }),
                &cx(dir.path().to_path_buf()),
            )
            .await
            .expect_err("a path above the root must be refused");
        assert!(matches!(err, ToolError::Confined { .. }), "got {err:?}");
    }
}
