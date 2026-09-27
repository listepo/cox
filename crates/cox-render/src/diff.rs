//! Diff view (T5.4, T24.5): a `ToolResult.diff` as coloured unified-diff
//! lines under a per-file `± path  +n −m` header, collapsible to that header
//! alone. A replaced line pair shows which words changed; a viewport of at
//! least `SIDE_MIN_WIDTH` columns splits old and new into two panes. The
//! edit card, the approval modal and `Ctrl+G` all print through `lines`.
//! Separate from `cells` because hunk parsing, pairing and pane fitting are
//! their own small machine and more than one surface prints a diff.

use cox_protocol::types::Diff;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use similar::{ChangeTag, TextDiff};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::Look;
use crate::markdown;

/// Below this many columns a pane is too narrow to read a line of code, so
/// the diff stays stacked whatever `tui.diff` says.
pub const SIDE_MIN_WIDTH: u16 = 120;

/// A pair with a longer line keeps its line colour instead of a word diff:
/// the diff's cost grows with the product of the two lengths, and a
/// minified line would be noise word by word anyway.
const WORD_DIFF_CAP: usize = 400;

/// `tui.diff`: whether a wide viewport may split a diff into two panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Side by side from `SIDE_MIN_WIDTH` columns, stacked below.
    #[default]
    Auto,
    /// Same threshold as `Auto`; named so a config can say what it wants.
    Side,
    /// Never split.
    Stacked,
}

impl Mode {
    /// An unknown value is `auto`: a typo costs a layout choice, not a start.
    pub fn parse(s: &str) -> Self {
        match s {
            "side" => Self::Side,
            "stacked" => Self::Stacked,
            _ => Self::Auto,
        }
    }
}

/// How the hunks are laid out; `left`/`right` are the panes' columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Stacked,
    Side { left: usize, right: usize },
}

impl Layout {
    pub fn pick(mode: Mode, width: u16) -> Self {
        if mode == Mode::Stacked || width < SIDE_MIN_WIDTH {
            return Self::Stacked;
        }
        // Two columns of indent (as stacked lines have) and one `│`.
        let usable = usize::from(width) - 3;
        let left = usable / 2;
        Self::Side {
            left,
            right: usable - left,
        }
    }
}

/// Added and removed line counts of a unified diff; file markers do not count.
pub fn counts(unified: &str) -> (usize, usize) {
    unified.lines().fold((0, 0), |(a, r), l| {
        if l.starts_with("+++") || l.starts_with("---") {
            (a, r)
        } else if l.starts_with('+') {
            (a + 1, r)
        } else if l.starts_with('-') {
            (a, r + 1)
        } else {
            (a, r)
        }
    })
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
enum Row<'a> {
    Meta(&'a str),
    Body {
        raw: &'a str,
        marker: &'a str,
        body: usize,
        old: Option<usize>,
        new: Option<usize>,
    },
}

/// A side-by-side row: a whole-width meta line, or the rows (indices into
/// the `Row` list) shown in the old and the new pane.
enum Aligned {
    Meta(usize),
    Pair(Option<usize>, Option<usize>),
}

/// `@@ -a,b +c,d @@` → `(a, c)`, the first old and new line numbers.
fn hunk_start(l: &str) -> Option<(usize, usize)> {
    let mut parts = l.split_whitespace().skip(1);
    let num = |p: Option<&str>, sign: char| -> Option<usize> {
        p?.strip_prefix(sign)?.split(',').next()?.parse().ok()
    };
    Some((num(parts.next(), '-')?, num(parts.next(), '+')?))
}

fn parse(unified: &str) -> (Vec<Row<'_>>, Vec<&str>) {
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
        old += usize::from(o.is_some());
        new += usize::from(n.is_some());
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

/// Rows in pane order: a run of `-` lines and the `+` run right after it
/// are zipped line by line, so the n-th removed line faces the n-th added
/// one — that pair is also what the word diff compares.
fn align(rows: &[Row<'_>]) -> Vec<Aligned> {
    let marker = |i: usize| match rows.get(i) {
        Some(Row::Body { marker, .. }) => Some(*marker),
        _ => None,
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        match marker(i) {
            None => {
                out.push(Aligned::Meta(i));
                i += 1;
            }
            Some("-") | Some("+") => {
                let dels = (i..).take_while(|&j| marker(j) == Some("-")).count();
                let adds = (i + dels..).take_while(|&j| marker(j) == Some("+")).count();
                for k in 0..dels.max(adds) {
                    out.push(Aligned::Pair(
                        (k < dels).then_some(i + k),
                        (k < adds).then_some(i + dels + k),
                    ));
                }
                i += dels + adds;
            }
            Some(_) => {
                out.push(Aligned::Pair(Some(i), Some(i)));
                i += 1;
            }
        }
    }
    out
}

/// Appends `text` to the last span when it has the same style, so a word
/// diff is a handful of spans rather than one per token.
fn push(spans: &mut Vec<Span<'static>>, text: &str, style: Style) {
    match spans.last_mut() {
        Some(last) if last.style == style => last.content.to_mut().push_str(text),
        _ => spans.push(Span::styled(text.to_string(), style)),
    }
}

/// The old and the new line of a replaced pair, changed words in the
/// add/remove colour and the words both share dim.
fn word_spans(old: &str, new: &str, look: &Look) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
    let same = Style::default().add_modifier(Modifier::DIM);
    let del = Style::default().fg(look.colors.diff_del);
    let add = Style::default().fg(look.colors.diff_add);
    let (mut left, mut right) = (Vec::new(), Vec::new());
    for c in TextDiff::from_words(old, new).iter_all_changes() {
        match c.tag() {
            ChangeTag::Equal => {
                push(&mut left, c.value(), same);
                push(&mut right, c.value(), same);
            }
            ChangeTag::Delete => push(&mut left, c.value(), del),
            ChangeTag::Insert => push(&mut right, c.value(), add),
        }
    }
    (left, right)
}

/// The style of a whole line by its first characters.
fn line_style(l: &str, look: &Look) -> Style {
    if l.starts_with("+++") || l.starts_with("---") || l.starts_with('\\') {
        Style::default().add_modifier(Modifier::DIM)
    } else if l.starts_with("@@") {
        Style::default().fg(look.colors.diff_hunk)
    } else if l.starts_with('+') {
        Style::default().fg(look.colors.diff_add)
    } else if l.starts_with('-') {
        Style::default().fg(look.colors.diff_del)
    } else {
        Style::default()
    }
}

/// `spans` cut or padded to exactly `width` columns; tabs become four
/// spaces first so a pane's width is what the terminal draws.
fn fit(spans: Vec<Span<'static>>, width: usize, ellipsis: &'static str) -> Vec<Span<'static>> {
    let spans: Vec<Span<'static>> = spans
        .into_iter()
        .map(|s| Span::styled(s.content.replace('\t', "    "), s.style))
        .collect();
    let total: usize = spans.iter().map(|s| s.content.width()).sum();
    let room = match total > width {
        true => width.saturating_sub(ellipsis.width()),
        false => width,
    };
    let (mut out, mut used) = (Vec::new(), 0);
    'spans: for s in spans {
        let mut text = String::new();
        for c in s.content.chars() {
            let w = c.width().unwrap_or(0);
            if used + w > room {
                out.push(Span::styled(text, s.style));
                break 'spans;
            }
            used += w;
            text.push(c);
        }
        out.push(Span::styled(text, s.style));
    }
    if total > width {
        out.push(Span::raw(ellipsis));
        used += ellipsis.width();
    }
    out.push(Span::raw(" ".repeat(width.saturating_sub(used))));
    out
}

/// The hunks of `diff` (no header) in `layout`. Bodies go through the same
/// syntect pass as a fenced block, highlighted by the patched file's
/// extension; a replaced pair shows its word diff instead. The add/remove
/// colour stays on the marker column so a theme can never hide what a
/// line does.
pub fn render(diff: &Diff, look: &Look, layout: Layout) -> Vec<Line<'static>> {
    let (rows, texts) = parse(&diff.unified);
    let aligned = align(&rows);
    let mut bodies: Vec<Option<Vec<Span<'static>>>> =
        match diff.path.extension().and_then(|e| e.to_str()) {
            Some(token) => markdown::highlight(token, &texts, look.theme)
                .into_iter()
                .map(|l| Some(l.spans))
                .collect(),
            None => Vec::new(),
        };
    bodies.resize(texts.len(), None);
    let body_of = |i: usize| match rows.get(i) {
        Some(Row::Body { body, .. }) => Some(*body),
        _ => None,
    };
    for a in &aligned {
        if let Aligned::Pair(Some(l), Some(r)) = *a
            && l != r
            && let (Some(l), Some(r)) = (body_of(l), body_of(r))
        {
            let within = texts[l].len() <= WORD_DIFF_CAP && texts[r].len() <= WORD_DIFF_CAP;
            let (old, new) = match within {
                true => {
                    let (o, n) = word_spans(texts[l], texts[r], look);
                    (Some(o), Some(n))
                }
                false => (None, None),
            };
            (bodies[l], bodies[r]) = (old, new);
        }
    }
    // A content line: `indent` and its marker in the line colour, then its
    // body spans, or the whole line in the line colour when there are none.
    let body_line = |indent: &str, raw: &str, marker: &str, body: usize| -> Vec<Span<'static>> {
        let style = line_style(raw, look);
        match &bodies[body] {
            Some(spans) => {
                let mut out = vec![Span::styled(format!("{indent}{marker}"), style)];
                out.extend(spans.iter().cloned());
                out
            }
            None => vec![Span::styled(format!("{indent}{raw}"), style)],
        }
    };
    let Layout::Side { left, right } = layout else {
        return rows
            .iter()
            .map(|r| match r {
                Row::Meta(l) => Line::styled(format!("  {l}"), line_style(l, look)),
                Row::Body {
                    raw, marker, body, ..
                } => Line::from(body_line("  ", raw, marker, *body)),
            })
            .collect();
    };
    let last = rows.iter().fold(0, |m, r| match r {
        Row::Body { old, new, .. } => m.max(old.unwrap_or(0)).max(new.unwrap_or(0)),
        Row::Meta(_) => m,
    });
    let digits = last.to_string().len();
    let gutter = Style::default().fg(look.colors.dim);
    let ellipsis = look.glyphs.ellipsis;
    // One pane: `  12 +body…`, exactly `width` columns, blank when absent.
    let pane = |row: Option<usize>, old_side: bool, width: usize| -> Vec<Span<'static>> {
        let Some(Row::Body {
            raw,
            marker,
            body,
            old,
            new,
        }) = row.and_then(|i| rows.get(i))
        else {
            return vec![Span::raw(" ".repeat(width))];
        };
        let no = if old_side { old } else { new };
        let no = no.map_or(String::new(), |n| n.to_string());
        let mut spans = vec![Span::styled(format!("{no:>digits$} "), gutter)];
        spans.extend(body_line("", raw, marker, *body));
        fit(spans, width, ellipsis)
    };
    aligned
        .iter()
        .map(|a| match *a {
            Aligned::Meta(i) => {
                let l = match rows.get(i) {
                    Some(Row::Meta(l)) => *l,
                    _ => "",
                };
                let width = left + right + 1;
                let mut spans = vec![Span::raw("  ")];
                spans.extend(fit(
                    vec![Span::styled(l.to_string(), line_style(l, look))],
                    width,
                    ellipsis,
                ));
                Line::from(spans)
            }
            Aligned::Pair(l, r) => {
                let mut spans = vec![Span::raw("  ")];
                spans.extend(pane(l, true, left));
                spans.push(Span::styled("│", gutter));
                spans.extend(pane(r, false, right));
                Line::from(spans)
            }
        })
        .collect()
}

/// The header, plus the hunks when `look.show_diffs`, laid out for
/// `look.width` and `look.diff`.
pub fn lines(diff: &Diff, look: &Look) -> Vec<Line<'static>> {
    let (added, removed) = counts(&diff.unified);
    let g = look.glyphs;
    let mut out = vec![Line::styled(
        format!(
            "  {} {}  +{added} {}{removed}",
            g.diff,
            diff.path.display(),
            g.minus
        ),
        Style::default().add_modifier(Modifier::BOLD),
    )];
    if look.show_diffs {
        out.extend(render(diff, look, Layout::pick(look.diff, look.width)));
    }
    out
}

/// A whole-tree patch split at its `diff --git a/x b/y` headers into the
/// per-file `Diff`s `lines` already renders (T15.3). The path is `y`; the
/// `index`/mode lines stay in the text, where `lines` prints them plain.
pub fn from_unified(text: &str) -> Vec<Diff> {
    let mut out: Vec<Diff> = Vec::new();
    for l in text.lines() {
        if let Some(rest) = l.strip_prefix("diff --git ") {
            let path = rest.rsplit_once(" b/").map_or(rest, |(_, p)| p);
            out.push(Diff {
                path: path.into(),
                unified: String::new(),
            });
        } else if let Some(d) = out.last_mut() {
            d.unified.push_str(l);
            d.unified.push('\n');
        }
    }
    out
}

/// The `Ctrl+G` view: a key line, then every file's headed block, or
/// `no changes`. Always expanded, whatever `Ctrl+O` last did.
pub fn view_lines(text: &str, look: &Look) -> Vec<Line<'static>> {
    let look = Look {
        show_diffs: true,
        ..*look
    };
    let sep = look.glyphs.sep;
    let dim = Style::default().add_modifier(Modifier::DIM);
    let mut out = vec![Line::styled(
        format!(" git diff HEAD {sep} PageUp/PageDown scroll {sep} Esc closes"),
        dim,
    )];
    let files = from_unified(text);
    if files.is_empty() {
        out.push(Line::styled("  no changes", dim));
    }
    out.extend(files.iter().flat_map(|d| lines(d, &look)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_skip_file_markers() {
        let d = "--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n-old\n+new\n+more\n context\n";
        assert_eq!(counts(d), (2, 1));
    }

    fn look() -> Look {
        Look {
            width: 80,
            theme: markdown::theme_name(true, ""),
            glyphs: crate::glyph::UNICODE,
            show_thinking: false,
            show_diffs: true,
            diff: Mode::Auto,
            tick: 0,
            still: false,
            marks: false,
            colors: crate::theme::Theme::dark(),
            expand_last: None,
        }
    }

    #[test]
    fn an_added_hunk_body_is_highlighted_under_a_coloured_marker() {
        let d = Diff {
            path: "src/x.rs".into(),
            unified: "@@ -1 +1,2 @@\n let x = 1;\n+fn main() {}\n".to_string(),
        };
        let out = lines(&d, &look());
        let added = &out[3];
        assert_eq!(added.to_string(), "  +fn main() {}");
        assert_eq!(added.spans[0].style.fg, Some(look().colors.diff_add));
        // `fn` is a keyword: syntect split the body into spans of its own.
        assert!(added.spans.len() > 2, "{:?}", added.spans);
    }

    #[test]
    fn a_pair_past_the_word_diff_cap_keeps_its_line_colour() {
        let long = "x".repeat(WORD_DIFF_CAP + 1);
        let d = Diff {
            path: "NOTES".into(),
            unified: format!("@@ -1 +1 @@\n-{long}\n+{long}y\n"),
        };
        let out = lines(&d, &look());
        assert_eq!(out[3].spans.len(), 1, "{:?}", out[3].spans);
        assert_eq!(out[3].spans[0].style.fg, Some(look().colors.diff_add));
    }

    #[test]
    fn align_zips_a_removed_run_against_the_added_run_after_it() {
        let (rows, _) = parse("@@ -1,3 +1,2 @@\n-a\n-b\n-c\n+x\n+y\n k\n");
        let pairs: Vec<_> = align(&rows)
            .into_iter()
            .map(|a| match a {
                Aligned::Meta(i) => (Some(i), None),
                Aligned::Pair(l, r) => (l, r),
            })
            .collect();
        assert_eq!(
            pairs,
            vec![
                (Some(0), None),
                (Some(1), Some(4)),
                (Some(2), Some(5)),
                (Some(3), None),
                (Some(6), Some(6)),
            ]
        );
    }

    #[test]
    fn fit_cuts_with_an_ellipsis_and_pads_to_the_exact_width() {
        let w = |spans: &[Span<'_>]| spans.iter().map(|s| s.content.width()).sum::<usize>();
        let cut = fit(vec![Span::raw("abcdef\tg")], 5, "…");
        assert_eq!(w(&cut), 5);
        assert_eq!(Line::from(cut).to_string(), "abcd…");
        let padded = fit(vec![Span::raw("ab")], 5, "…");
        assert_eq!(Line::from(padded).to_string(), "ab   ");
    }

    #[test]
    fn layout_is_side_only_from_120_columns_and_never_when_stacked() {
        assert_eq!(Layout::pick(Mode::Auto, 119), Layout::Stacked);
        assert_eq!(
            Layout::pick(Mode::Side, 121),
            Layout::Side {
                left: 59,
                right: 59
            }
        );
        assert_eq!(Layout::pick(Mode::Stacked, 200), Layout::Stacked);
        assert_eq!(Mode::parse("bogus"), Mode::Auto);
    }

    #[test]
    fn from_unified_splits_a_patch_at_its_headers_and_keeps_hunks() {
        let patch = "diff --git a/src/x.rs b/src/x.rs\nindex 1..2 100644\n--- a/src/x.rs\n+++ b/src/x.rs\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/new file b/new file\nnew file mode 100644\n--- /dev/null\n+++ b/new file\n@@ -0,0 +1 @@\n+hello\n";
        let files = from_unified(patch);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, std::path::Path::new("src/x.rs"));
        assert!(files[0].unified.starts_with("index 1..2"));
        assert_eq!(counts(&files[0].unified), (1, 1));
        assert_eq!(files[1].path, std::path::Path::new("new file"));
        assert_eq!(counts(&files[1].unified), (1, 0));
        assert!(from_unified("").is_empty());
    }

    #[test]
    fn a_diff_of_an_unknown_file_type_stays_plain() {
        let d = Diff {
            path: "NOTES".into(),
            unified: "@@ -1 +1 @@\n+hello\n".to_string(),
        };
        let out = lines(&d, &look());
        assert_eq!(out[2].to_string(), "  +hello");
        assert_eq!(out[2].spans.len(), 1);
    }
}
