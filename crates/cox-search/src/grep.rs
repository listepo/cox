//! Ripgrep-equivalent content search (plan.md T3.3): walks a root with
//! `ignore::WalkBuilder` (`.gitignore` honoured, hidden files included),
//! searches each file with `grep-regex` + `grep-searcher`, and formats
//! `-n --no-heading`-style output (`path:line:text`, context lines as
//! `path-line-text` with a bare `--` between non-contiguous groups — the
//! same shapes `rg` prints). Pure: [`search`] takes a root and returns the
//! formatted [`Line`]s; the caller (`cox_tools::grep`) owns `ToolCx`,
//! `path::confine` and archiving the over-cap tail.

use std::path::{Path, PathBuf};

use grep_regex::RegexMatcher;
use grep_searcher::{Searcher, SearcherBuilder, Sink, SinkContext, SinkMatch};
use ignore::WalkBuilder;

/// One formatted output line plus whether it counts toward a caller's
/// match cap (context/`--` break lines don't).
pub struct Line {
    pub text: String,
    pub is_match: bool,
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
pub(crate) fn glob_allows(
    glob: &globset::GlobMatcher,
    entry_path: &Path,
    file_name: &std::ffi::OsStr,
) -> bool {
    glob.is_match(file_name) || glob.is_match(entry_path)
}

/// An invalid regex or glob pattern handed to [`search`].
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("invalid pattern: {0}")]
    Pattern(grep_regex::Error),
    #[error("invalid glob: {0}")]
    Glob(globset::Error),
}

/// Ripgrep-equivalent content search over `root`: `ignore::WalkBuilder`
/// (`.gitignore` honoured, hidden files included) + `grep-regex`/
/// `grep-searcher`. `glob` filters which files are searched (matched
/// against the basename or the full path); `context` adds that many lines
/// of context before and after each match.
pub fn search(
    root: &Path,
    pattern: &str,
    glob: Option<&str>,
    context: Option<usize>,
) -> Result<Vec<Line>, SearchError> {
    let matcher = RegexMatcher::new(pattern).map_err(SearchError::Pattern)?;
    let glob_matcher = match glob {
        Some(g) => Some(
            globset::Glob::new(g)
                .map_err(SearchError::Glob)?
                .compile_matcher(),
        ),
        None => None,
    };

    let mut walker = WalkBuilder::new(root);
    walker.hidden(false).sort_by_file_path(|a, b| a.cmp(b));

    let mut all: Vec<Line> = Vec::new();
    for entry in walker.build() {
        let Ok(entry) = entry else { continue }; // unreadable dir entry: skip, not fatal
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let entry_path = entry.path();
        if let Some(gm) = &glob_matcher
            && !glob_allows(gm, entry_path, entry.file_name())
        {
            continue;
        }

        let mut builder = SearcherBuilder::new();
        builder.line_number(true);
        if let Some(n) = context {
            builder.before_context(n).after_context(n);
        }
        let mut searcher = builder.build();
        let mut sink = GrepSink {
            path: entry_path,
            lines: Vec::new(),
        };
        // A search error (binary content, unreadable file) just skips
        // that file rather than failing the whole call.
        if searcher
            .search_path(&matcher, entry_path, &mut sink)
            .is_ok()
        {
            all.extend(sink.lines);
        }
    }

    Ok(all)
}

/// The gitignore-aware walk shared by `grep` and `glob`: `.gitignore`
/// honoured, hidden files included. `require_git(false)`: a `.gitignore`
/// states intent whether or not a `.git` directory happens to sit above it,
/// and a worktree the agent is handed may not be a repository at all.
pub(crate) fn walker(root: &PathBuf) -> WalkBuilder {
    let mut w = WalkBuilder::new(root);
    w.hidden(false)
        .require_git(false)
        .sort_by_file_path(|a, b| a.cmp(b));
    w
}
