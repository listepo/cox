//! `DiffModel` (T37.23.5, DT§4.3): a `ToolResult.diff` as hunks of lines,
//! each with its kind, its old and new line numbers and its highlighted
//! runs, so the desktop app draws an edit without parsing unified text.
//! Also the unified-text parse the TUI's `diff` draws from, so both surfaces
//! read a hunk the same way. Outside `diff` because that module draws with
//! ratatui, and `cox-app` builds without the `ratatui` feature.

use std::path::PathBuf;

use cox_protocol::types::Diff;
use serde::{Deserialize, Serialize};

use crate::doc::{StyledLine, StyledSpan};
use crate::markdown::highlight_runs;

/// One file's change, hunk by hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffModel {
    pub path: PathBuf,
    pub hunks: Vec<DiffHunk>,
}

/// An `@@` header and the lines under it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffHunk {
    /// `@@ -41,12 +41,26 @@ impl Backoff`; empty for lines before any header.
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    /// The line's number before the edit; `None` for an added line.
    pub old: Option<u32>,
    /// The line's number after the edit; `None` for a removed line.
    pub new: Option<u32>,
    /// The body without its marker, highlighted by the file's extension.
    pub spans: StyledLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffLineKind {
    Context,
    Add,
    Del,
}

/// A hunk line that carries file content: its marker and the source under it.
/// `+++`/`---` are file markers, not additions, so they are not content.
fn content(l: &str) -> Option<(&str, &str)> {
    if l.starts_with("+++") || l.starts_with("---") {
        return None;
    }
    match l.chars().next() {
        Some('+') | Some('-') | Some(' ') => Some(l.split_at(1)),
        _ => None,
    }
}

/// One line of the unified text: a content line knows its body's index and
/// its line numbers; anything else (`@@`, file markers, `index`, `\ No
/// newline`) is printed whole.
pub(crate) enum Row<'a> {
    Meta(&'a str),
    Body {
        /// The whole line, which only the TUI's `diff` prints.
        #[cfg_attr(not(feature = "ratatui"), allow(dead_code))]
        raw: &'a str,
        marker: &'a str,
        body: usize,
        old: Option<u32>,
        new: Option<u32>,
    },
}

/// `@@ -a,b +c,d @@` → `(a, c)`, the first old and new line numbers.
fn hunk_start(l: &str) -> Option<(u32, u32)> {
    let mut parts = l.split_whitespace().skip(1);
    let num = |p: Option<&str>, sign: char| -> Option<u32> {
        p?.strip_prefix(sign)?.split(',').next()?.parse().ok()
    };
    Some((num(parts.next(), '-')?, num(parts.next(), '+')?))
}

/// The rows of `unified` and the bodies of its content lines, in order.
pub(crate) fn parse(unified: &str) -> (Vec<Row<'_>>, Vec<&str>) {
    let (mut rows, mut bodies) = (Vec::new(), Vec::new());
    let (mut old, mut new) = (1, 1);
    for l in unified.lines() {
        if let Some((o, n)) = l.starts_with("@@").then(|| hunk_start(l)).flatten() {
            (old, new) = (o, n);
        }
        let Some((marker, text)) = content(l) else {
            rows.push(Row::Meta(l));
            continue;
        };
        let (o, n) = match marker {
            "-" => (Some(old), None),
            "+" => (None, Some(new)),
            _ => (Some(old), Some(new)),
        };
        old += u32::from(o.is_some());
        new += u32::from(n.is_some());
        rows.push(Row::Body {
            raw: l,
            marker,
            body: bodies.len(),
            old: o,
            new: n,
        });
        bodies.push(text);
    }
    (rows, bodies)
}

/// `diff` as hunks. Bodies go through the one syntect pass the TUI's diff
/// and fenced blocks use, highlighted by the file's extension with `theme`;
/// a file without one stays plain, as it does in the TUI. File markers,
/// `index` and `\ No newline` lines are dropped: the UI draws the path.
pub fn model(diff: &Diff, theme: &str) -> DiffModel {
    let (rows, texts) = parse(&diff.unified);
    let mut spans = match diff.path.extension().and_then(|e| e.to_str()) {
        Some(token) => highlight_runs(token, &texts, theme),
        None => Vec::new(),
    };
    spans.resize_with(texts.len(), Vec::new);
    for (line, text) in spans.iter_mut().zip(&texts) {
        if line.is_empty() && !text.is_empty() {
            *line = vec![StyledSpan::plain(*text)];
        }
    }
    let mut hunks: Vec<DiffHunk> = Vec::new();
    for row in rows {
        match row {
            Row::Meta(l) if l.starts_with("@@") => hunks.push(DiffHunk {
                header: l.to_owned(),
                lines: Vec::new(),
            }),
            Row::Meta(_) => {}
            Row::Body {
                marker,
                body,
                old,
                new,
                ..
            } => {
                if hunks.is_empty() {
                    hunks.push(DiffHunk {
                        header: String::new(),
                        lines: Vec::new(),
                    });
                }
                let kind = match marker {
                    "+" => DiffLineKind::Add,
                    "-" => DiffLineKind::Del,
                    _ => DiffLineKind::Context,
                };
                let line = DiffLine {
                    kind,
                    old,
                    new,
                    spans: spans.get_mut(body).map(std::mem::take).unwrap_or_default(),
                };
                if let Some(hunk) = hunks.last_mut() {
                    hunk.lines.push(line);
                }
            }
        }
    }
    DiffModel {
        path: diff.path.clone(),
        hunks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::theme_name;

    fn diff(path: &str, unified: &str) -> Diff {
        Diff {
            path: path.into(),
            unified: unified.to_owned(),
        }
    }

    type Numbered = (DiffLineKind, Option<u32>, Option<u32>);

    fn numbers(m: &DiffModel) -> Vec<Vec<Numbered>> {
        let line = |l: &DiffLine| (l.kind, l.old, l.new);
        m.hunks
            .iter()
            .map(|h| h.lines.iter().map(line).collect())
            .collect()
    }

    #[test]
    fn each_hunk_numbers_its_lines_from_its_own_header() {
        let unified = "--- a/x.rs\n+++ b/x.rs\n@@ -3,2 +3,2 @@ fn a\n ctx\n-old\n+new\n\
            @@ -40 +40,2 @@\n keep\n+more\n\\ No newline at end of file\n";
        let m = model(&diff("x.rs", unified), theme_name(true, ""));
        use DiffLineKind::{Add, Context, Del};
        assert_eq!(
            m.hunks
                .iter()
                .map(|h| h.header.as_str())
                .collect::<Vec<_>>(),
            ["@@ -3,2 +3,2 @@ fn a", "@@ -40 +40,2 @@"]
        );
        assert_eq!(
            numbers(&m),
            [
                vec![
                    (Context, Some(3), Some(3)),
                    (Del, Some(4), None),
                    (Add, None, Some(4))
                ],
                vec![(Context, Some(40), Some(40)), (Add, None, Some(41))],
            ]
        );
    }

    #[test]
    fn a_known_extension_highlights_the_body_without_its_marker() {
        let m = model(
            &diff("x.rs", "@@ -1 +1 @@\n+fn main() {}\n"),
            "base16-ocean.dark",
        );
        let spans = &m.hunks[0].lines[0].spans;
        let text: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(text, "fn main() {}");
        assert!(spans.len() > 1 && spans.iter().all(|s| s.rgb.is_some()));
    }

    #[test]
    fn a_file_without_an_extension_stays_plain() {
        let m = model(
            &diff("NOTES", "@@ -1 +1,2 @@\n-a\n+b\n+\n"),
            "base16-ocean.dark",
        );
        let lines = &m.hunks[0].lines;
        assert_eq!(lines[0].spans, [StyledSpan::plain("a")]);
        assert!(lines[2].spans.is_empty());
    }
}
