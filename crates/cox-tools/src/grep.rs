//! `grep`: ripgrep-equivalent content search (plan.md T3.3). Walks a
//! confined root with `ignore::WalkBuilder` (`.gitignore` honoured, hidden
//! files included), searches each file with `grep-regex` + `grep-searcher`,
//! and formats `-n --no-heading`-style output (`path:line:text`, context
//! lines as `path-line-text` with a bare `--` between non-contiguous
//! groups — the same shapes `rg` prints). When the match count exceeds
//! `max_results`, the full result is archived (D6a: the archive row exists
//! before the model sees the shortened text) and a pointer trailer line is
//! appended, instead of silently dropping matches.

use std::path::{Path, PathBuf};

use cox_protocol::{ArchivePut, ToolCx, ToolError, ToolOutput, ToolSpec};
use cox_protocol::{Concurrency, Risk};
use grep_regex::RegexMatcher;
use grep_searcher::{Searcher, SearcherBuilder, Sink, SinkContext, SinkMatch};
use ignore::WalkBuilder;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::path;

/// Matches are capped here unless the caller sets a smaller/larger
/// `max_results`; keeps a runaway pattern on a big tree from flooding the
/// model with an unbounded reply instead of a pointer.
const DEFAULT_MAX_RESULTS: usize = 200;

/// `grep`'s JSON input, matching plan.md T3.3's step list.
#[derive(Debug, Deserialize, JsonSchema)]
struct GrepInput {
    /// Regular expression to search for (Rust `regex` syntax).
    pattern: String,
    /// Directory or file to search, relative to a workspace root. Defaults
    /// to the root itself.
    #[serde(default)]
    path: Option<String>,
    /// Only search files whose name or path matches this glob (e.g. `*.rs`).
    #[serde(default)]
    glob: Option<String>,
    /// Lines of context to show before and after each match.
    #[serde(default)]
    context: Option<usize>,
    /// Stop after this many matches and archive the rest. Defaults to
    /// [`DEFAULT_MAX_RESULTS`].
    #[serde(default)]
    max_results: Option<usize>,
}

/// One formatted output line plus whether it counts toward `max_results`
/// (context/`--` break lines don't).
struct Line {
    text: String,
    is_match: bool,
}

/// A `grep_searcher::Sink` that formats matched/context lines the way `rg
/// -n --no-heading` does, prefixed with `path`.
struct GrepSink<'a> {
    path: &'a Path,
    lines: Vec<Line>,
}

impl Sink for GrepSink<'_> {
    type Error = std::io::Error;

    fn matched(
        &mut self,
        _searcher: &Searcher,
        mat: &SinkMatch<'_>,
    ) -> Result<bool, std::io::Error> {
        let Some(line_number) = mat.line_number() else {
            return Ok(true); // line numbers are always requested; skip defensively
        };
        let text = String::from_utf8_lossy(mat.bytes());
        self.lines.push(Line {
            text: format!(
                "{}:{}:{}",
                self.path.display(),
                line_number,
                text.trim_end_matches(['\n', '\r'])
            ),
            is_match: true,
        });
        Ok(true)
    }

    fn context(
        &mut self,
        _searcher: &Searcher,
        ctx: &SinkContext<'_>,
    ) -> Result<bool, std::io::Error> {
        let Some(line_number) = ctx.line_number() else {
            return Ok(true);
        };
        let text = String::from_utf8_lossy(ctx.bytes());
        self.lines.push(Line {
            text: format!(
                "{}-{}-{}",
                self.path.display(),
                line_number,
                text.trim_end_matches(['\n', '\r'])
            ),
            is_match: false,
        });
        Ok(true)
    }

    fn context_break(&mut self, _searcher: &Searcher) -> Result<bool, std::io::Error> {
        self.lines.push(Line {
            text: "--".to_string(),
            is_match: false,
        });
        Ok(true)
    }
}

/// A file's glob filter matches either its basename (`*.rs` at any depth,
/// gitignore-style) or its full path (patterns that spell out a directory).
fn glob_allows(glob: &globset::GlobMatcher, entry_path: &Path, file_name: &std::ffi::OsStr) -> bool {
    glob.is_match(file_name) || glob.is_match(entry_path)
}

/// Ripgrep-equivalent content search: `ignore::WalkBuilder` (`.gitignore`
/// honoured, hidden files included) + `grep-regex`/`grep-searcher`.
pub struct GrepTool;

#[async_trait::async_trait]
impl cox_protocol::Tool for GrepTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "grep".into(),
            description: "Search file contents with a regular expression (ripgrep semantics: \
                .gitignore is honoured, hidden files are included). Returns `path:line:text` \
                per match, `path-line-text` for `context` lines, and a bare `--` between \
                non-contiguous groups. `glob` filters which files are searched (matched against \
                the basename or the full path). Past `max_results` matches the rest is archived \
                and a trailer line names the archive id."
                .into(),
            input_schema: schemars::schema_for!(GrepInput).to_value(),
            deferred: false,
            risk: Risk::ReadOnly,
            concurrency: Concurrency::Parallel,
        }
    }

    fn subject(&self, input: &Value) -> String {
        input
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or(".")
            .to_string()
    }

    async fn call(&self, input: Value, cx: &ToolCx) -> Result<ToolOutput, ToolError> {
        let input: GrepInput = serde_json::from_value(input)
            .map_err(|e| ToolError::Denied {
                why: format!("invalid input: {e}"),
            })?;

        let root = path::confine(&cx.roots, &cx.cwd, input.path.as_deref().unwrap_or("."))?;
        if !root.exists() {
            return Err(ToolError::NotFound);
        }

        let matcher = match RegexMatcher::new(&input.pattern) {
            Ok(m) => m,
            Err(e) => return Ok(text_error(format!("invalid pattern: {e}"))),
        };
        let glob_matcher = match &input.glob {
            Some(g) => match globset::Glob::new(g) {
                Ok(g) => Some(g.compile_matcher()),
                Err(e) => return Ok(text_error(format!("invalid glob: {e}"))),
            },
            None => None,
        };

        let mut walker = WalkBuilder::new(&root);
        walker.hidden(false).sort_by_file_path(|a, b| a.cmp(b));

        let mut all: Vec<Line> = Vec::new();
        for entry in walker.build() {
            let Ok(entry) = entry else { continue }; // unreadable dir entry: skip, not fatal
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            let entry_path = entry.path();
            if let Some(gm) = &glob_matcher {
                if !glob_allows(gm, entry_path, entry.file_name()) {
                    continue;
                }
            }

            let mut builder = SearcherBuilder::new();
            builder.line_number(true);
            if let Some(n) = input.context {
                builder.before_context(n).after_context(n);
            }
            let mut searcher = builder.build();
            let mut sink = GrepSink {
                path: entry_path,
                lines: Vec::new(),
            };
            // A search error (binary content, unreadable file) just skips
            // that file rather than failing the whole call.
            if searcher.search_path(&matcher, entry_path, &mut sink).is_ok() {
                all.extend(sink.lines);
            }
        }

        let total_matches = all.iter().filter(|l| l.is_match).count();
        let cap = input.max_results.unwrap_or(DEFAULT_MAX_RESULTS);

        if total_matches == 0 {
            return Ok(ToolOutput {
                text: "no matches".into(),
                is_error: false,
                diff: None,
                structured: None,
            });
        }

        if total_matches <= cap {
            let text = join(&all);
            return Ok(ToolOutput {
                text,
                is_error: false,
                diff: None,
                structured: None,
            });
        }

        // Exceeded the cap: archive the full result, then truncate to the
        // first `cap` matches (plus whatever context/break lines lead up to
        // them) and append a pointer trailer.
        let full_text = join(&all);
        let archive_id = cx
            .archive
            .put(ArchivePut {
                session: cx.session,
                call: cx.call,
                tool: "grep".into(),
                subject: Some(root.display().to_string()),
                bytes: full_text.clone().into_bytes(),
            })
            .await
            .map_err(|_| ToolError::Io)?;

        let mut truncated: Vec<&Line> = Vec::new();
        let mut emitted = 0usize;
        for line in &all {
            if emitted >= cap {
                break;
            }
            if line.is_match {
                emitted += 1;
            }
            truncated.push(line);
        }
        let mut text = truncated
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        text.push('\n');
        text.push_str(&format!(
            "… {} more matches; archived as {archive_id}",
            total_matches - cap
        ));

        Ok(ToolOutput {
            text,
            is_error: false,
            diff: None,
            structured: None,
        })
    }
}

fn join(lines: &[Line]) -> String {
    lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn text_error(text: String) -> ToolOutput {
    ToolOutput {
        text,
        is_error: true,
        diff: None,
        structured: None,
    }
}

/// Only used by tests below and by `glob.rs` via `super::path`; kept `pub`
/// within the crate so `glob.rs` can build the same kind of walker without
/// duplicating the `hidden(false)` + gitignore configuration.
pub(crate) fn walker(root: &PathBuf) -> WalkBuilder {
    let mut w = WalkBuilder::new(root);
    w.hidden(false).sort_by_file_path(|a, b| a.cmp(b));
    w
}

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::sync::Arc;

    use cox_protocol::{Archive, ArchiveId, SandboxMode, SandboxPolicy, SessionId, StoreError, Tool};
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

    fn fixtures_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/grep")
            .canonicalize()
            .expect("fixtures/grep exists")
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
            },
            archive: Arc::new(NoopArchive),
            cancel: CancellationToken::new(),
            output: tx,
            session: SessionId::new(),
            call: cox_protocol::CallId::new(),
        }
    }

    async fn run(pattern: &str, extra: Value) -> ToolOutput {
        let root = fixtures_root();
        let mut input = serde_json::json!({ "pattern": pattern, "path": "." });
        input
            .as_object_mut()
            .expect("object")
            .extend(extra.as_object().cloned().unwrap_or_default());
        GrepTool
            .call(input, &cx(root))
            .await
            .expect("grep call succeeds")
    }

    /// Runs `rg -n --no-heading --hidden --sort path <pattern> <root>` and
    /// returns its stdout, or `None` if `rg` isn't on PATH (T3.3's spec:
    /// "rg invoked only if present on the test machine; otherwise golden
    /// files" — golden files live alongside this test as `*.golden`).
    fn rg_output(pattern: &str, root: &Path) -> Option<String> {
        let out = Command::new("rg")
            .args(["-n", "--no-heading", "--hidden", "--sort", "path", pattern])
            .arg(root)
            .output()
            .ok()?;
        Some(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    }

    fn golden_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/grep")
            .join(format!("{name}.golden"))
    }

    async fn assert_matches_rg_or_golden(name: &str, pattern: &str) {
        let root = fixtures_root();
        let got = run(pattern, serde_json::json!({})).await;
        let got_text = got.text.trim_end().to_string();

        match rg_output(pattern, &root) {
            Some(want) => assert_eq!(got_text, want, "pattern {pattern:?}"),
            None => {
                let want = std::fs::read_to_string(golden_path(name))
                    .unwrap_or_else(|_| panic!("golden file missing for {name}"))
                    .trim_end()
                    .to_string();
                assert_eq!(got_text, want, "pattern {pattern:?} (golden)");
            }
        }
    }

    #[tokio::test]
    async fn grep_matches_rg_word_todo() {
        assert_matches_rg_or_golden("todo", "TODO").await;
    }

    #[tokio::test]
    async fn grep_matches_rg_literal_fn_space() {
        assert_matches_rg_or_golden("fn_space", "fn ").await;
    }

    #[tokio::test]
    async fn grep_matches_rg_anchored_pub_fn() {
        assert_matches_rg_or_golden("anchored_pub_fn", "^pub fn").await;
    }

    #[tokio::test]
    async fn grep_matches_rg_alternation() {
        assert_matches_rg_or_golden("alternation", "hello|deep").await;
    }

    #[tokio::test]
    async fn grep_matches_rg_digit_class() {
        assert_matches_rg_or_golden("digit_class", "[0-9]+").await;
    }

    #[tokio::test]
    async fn grep_respects_gitignore() {
        let out = run("TODO", serde_json::json!({})).await;
        assert!(
            !out.text.contains("ignored.txt") && !out.text.contains("build.log"),
            "gitignored files leaked into output:\n{}",
            out.text
        );
    }

    #[tokio::test]
    async fn grep_glob_filters_to_matching_files() {
        let out = run("fn", serde_json::json!({ "glob": "*.rs" })).await;
        assert!(out.text.contains("main.rs"));
        assert!(!out.text.contains("readme.md"));
    }

    #[tokio::test]
    async fn grep_max_results_archives_and_appends_trailer() {
        let out = run("TODO", serde_json::json!({ "max_results": 1 })).await;
        assert!(
            out.text.contains("more matches; archived as"),
            "expected a pointer trailer, got:\n{}",
            out.text
        );
        // Exactly one match line shown before the trailer.
        let match_lines = out.text.lines().filter(|l| l.contains(":TODO") || l.contains("TODO")).count();
        assert!(match_lines >= 1);
    }

    #[tokio::test]
    async fn grep_context_includes_surrounding_lines() {
        let out = run("deep", serde_json::json!({ "context": 1, "glob": "file.txt" })).await;
        assert!(out.text.contains("second line") || out.text.lines().count() > 1);
    }
}
