//! Diff view (T5.4): a `ToolResult.diff` as coloured unified-diff lines under
//! a per-file `± path  +n −m` header, collapsible to that header alone.
//! Separate from `cells` because line classification is its own small table
//! and more than one cell (edit results now, `cox run` transcripts later)
//! prints a diff.

use cox_protocol::types::Diff;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;

use crate::glyph::Glyphs;

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

/// The header, plus the hunks when `expanded`.
pub fn lines(diff: &Diff, expanded: bool, g: &Glyphs) -> Vec<Line<'static>> {
    let (added, removed) = counts(&diff.unified);
    let mut out = vec![Line::styled(
        format!(
            "  {} {}  +{added} {}{removed}",
            g.diff,
            diff.path.display(),
            g.minus
        ),
        Style::default().add_modifier(Modifier::BOLD),
    )];
    if !expanded {
        return out;
    }
    out.extend(diff.unified.lines().map(|l| {
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
        Line::styled(format!("  {l}"), style)
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
}
