//! Diff view (T5.4): a `ToolResult.diff` as coloured unified-diff lines under
//! a per-file `± path  +n −m` header, collapsible to that header alone.
//! Separate from `cells` because line classification is its own small table
//! and more than one cell (edit results now, `cox run` transcripts later)
//! prints a diff.

use cox_protocol::types::Diff;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::cells::Look;
use crate::markdown;

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

/// The header, plus the hunks when `look.show_diffs`. Hunk bodies go through
/// the same syntect pass as a fenced block, highlighted by the patched
/// file's extension; the add/remove colour stays on the marker column so a
/// theme can never hide what a line does.
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
    if !look.show_diffs {
        return out;
    }
    let rows: Vec<&str> = diff.unified.lines().collect();
    let bodies: Vec<&str> = rows
        .iter()
        .filter_map(|l| content(l))
        .map(|c| c.1)
        .collect();
    let mut painted = match diff.path.extension().and_then(|e| e.to_str()) {
        Some(token) => markdown::highlight(token, &bodies, look.theme).into_iter(),
        None => Vec::new().into_iter(),
    };
    out.extend(rows.iter().map(|l| {
        let style = if l.starts_with("+++") || l.starts_with("---") || l.starts_with('\\') {
            Style::default().add_modifier(Modifier::DIM)
        } else if l.starts_with("@@") {
            Style::default().fg(Color::Cyan)
        } else if l.starts_with('+') {
            Style::default().fg(Color::Green)
        } else if l.starts_with('-') {
            Style::default().fg(Color::Red)
        } else {
            Style::default()
        };
        // `painted` advances only on a content line, so a header never eats
        // a body and shifts every later line's highlighting.
        match content(l).and_then(|(marker, _)| painted.next().map(|body| (marker, body))) {
            Some((marker, body)) => {
                let mut spans = vec![Span::styled(format!("  {marker}"), style)];
                spans.extend(body.spans);
                Line::from(spans)
            }
            None => Line::styled(format!("  {l}"), style),
        }
    }));
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
            tick: 0,
            marks: false,
        }
    }

    #[test]
    fn a_hunk_body_is_highlighted_under_a_coloured_marker() {
        let d = Diff {
            path: "src/x.rs".into(),
            unified: "@@ -1 +1 @@\n-let x = 1;\n+fn main() {}\n".to_string(),
        };
        let out = lines(&d, &look());
        let added = &out[3];
        assert_eq!(added.to_string(), "  +fn main() {}");
        assert_eq!(added.spans[0].style.fg, Some(Color::Green));
        // `fn` is a keyword: syntect split the body into spans of its own.
        assert!(added.spans.len() > 2, "{:?}", added.spans);
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
