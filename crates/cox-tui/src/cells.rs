//! Transcript cells (T5.3): how each `Cell` prints at a given width, in the
//! viewport and in scrollback alike. Separate from `view` so the runtime's
//! `insert_before` and the test harness share one renderer, and from `state`
//! so the state machine knows nothing about columns or colours.

use cox_protocol::types::{Diff, Level};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::diff;
use crate::glyph::Glyphs;
use crate::markdown;
use crate::state::Cell;
use crate::text;

/// What rendering needs from the state besides the cell itself.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub width: u16,
    pub dark: bool,
    /// What the terminal can print; `glyph::resolve` decided it.
    pub glyphs: Glyphs,
    /// `Ctrl+T`: thinking expanded rather than a one-line count.
    pub show_thinking: bool,
    /// `Ctrl+O`: diffs in full rather than their `+n −m` header.
    pub show_diffs: bool,
    /// Ticks (100 ms) since start; drives the spinner and elapsed time.
    pub tick: u64,
    /// Leave `text::sanitize` markers where something was removed.
    pub marks: bool,
}

/// Output longer than head + tail + 1 lines is folded in the middle; the
/// archive keeps the rest (`cox expand <id>`).
const HEAD: usize = 6;
const TAIL: usize = 5;

fn dim(s: impl Into<String>) -> Line<'static> {
    Line::styled(s.into(), Style::default().add_modifier(Modifier::DIM))
}

/// `cell` as wrapped lines at `look.width`. Every string that came from the
/// model, a tool or a file passes `text::sanitize` here, at the boundary.
pub fn cell_lines(cell: &Cell, look: &Look) -> Vec<Line<'static>> {
    let clean = |s: &str| text::sanitize_with(s, look.marks);
    let g = look.glyphs;
    let lines = match cell {
        Cell::User { text, attachments } => {
            let mut lines = vec![Line::styled(
                format!("{} {}", g.user, clean(text)),
                Style::default().add_modifier(Modifier::BOLD),
            )];
            lines.extend(
                attachments
                    .iter()
                    .map(|a| dim(format!("  {} {}", g.attach, clean(a)))),
            );
            lines
        }
        Cell::Assistant { text, .. } => markdown::render(&clean(text), look.dark, &g),
        Cell::Thinking { text, done, .. } if !look.show_thinking => {
            // No tokenizer here; four bytes a token is the usual estimate.
            let tokens = text.len() / 4;
            let verb = match *done {
                true => "thought".to_string(),
                false => format!("thinking{}", g.ellipsis),
            };
            vec![dim(format!(
                "{} {verb} (~{tokens} tokens {} Ctrl+T)",
                g.think, g.sep
            ))]
        }
        Cell::Thinking { text, .. } => clean(text)
            .lines()
            .map(|l| dim(format!("{} {l}", g.think)))
            .collect(),
        Cell::Tool {
            call,
            output,
            result,
            started,
        } => {
            let header = format!("{} {} {}", g.tool, clean(&call.name), clean(&call.subject));
            let mut lines = vec![Line::styled(
                text::truncate(&header, usize::from(look.width.max(1))),
                Style::default().fg(Color::Cyan),
            )];
            let output = clean(output);
            let out: Vec<&str> = output.lines().collect();
            if out.len() > HEAD + TAIL + 1 {
                lines.extend(out[..HEAD].iter().map(|l| Line::raw(format!("  {l}"))));
                lines.push(dim(format!(
                    "  {} {} lines hidden {}",
                    g.ellipsis,
                    out.len() - HEAD - TAIL,
                    g.ellipsis
                )));
                lines.extend(
                    out[out.len() - TAIL..]
                        .iter()
                        .map(|l| Line::raw(format!("  {l}"))),
                );
            } else {
                lines.extend(out.iter().map(|l| Line::raw(format!("  {l}"))));
            }
            if let Some(d) = result.as_ref().and_then(|r| r.diff.as_ref()) {
                let d = Diff {
                    path: d.path.clone(),
                    unified: clean(&d.unified),
                };
                lines.extend(diff::lines(&d, look.show_diffs, &g));
            }
            match result {
                Some(r) => {
                    let mark = if r.ok { g.ok } else { g.fail };
                    let expand = r
                        .archive
                        .as_ref()
                        .map(|a| format!(" {} cox expand {}", g.sep, a.id))
                        .unwrap_or_default();
                    lines.push(dim(format!(
                        "  {mark} {}B {}ms{expand}",
                        r.bytes, r.duration_ms
                    )));
                }
                None => {
                    let elapsed = look.tick.saturating_sub(*started);
                    lines.push(dim(format!(
                        "  {} {}.{}s",
                        g.spin(look.tick),
                        elapsed / 10,
                        elapsed % 10
                    )));
                }
            }
            lines
        }
        Cell::Notice { level, text } => {
            let style = match level {
                Level::Info => Style::default().add_modifier(Modifier::DIM),
                Level::Warn => Style::default().fg(Color::Yellow),
                Level::Budget => Style::default().fg(Color::Magenta),
                Level::Security => Style::default().fg(Color::Red),
            };
            let tag = format!("[{}] ", format!("{level:?}").to_lowercase());
            let pad = " ".repeat(tag.width());
            clean(text)
                .lines()
                .enumerate()
                .map(|(i, l)| {
                    let prefix = if i == 0 { &tag } else { &pad };
                    Line::styled(format!("{prefix}{l}"), style)
                })
                .collect()
        }
        Cell::Error { text, fatal } => vec![Line::styled(
            format!(
                "{} {}{}",
                g.fail,
                clean(text),
                if *fatal { " (session ended)" } else { "" }
            ),
            Style::default().fg(Color::Red),
        )],
        Cell::Summary { text } => {
            let mut lines = vec![dim(format!(
                "{d} compacted; earlier turns summarised as {d}",
                d = g.dash
            ))];
            lines.extend(clean(text).lines().map(|l| dim(format!("  {l}"))));
            lines
        }
    };
    wrap(lines, look.width)
}

/// Word-wraps styled lines to `width` columns by display width, keeping each
/// span's style; continuation rows repeat the line's leading indent so
/// wrapped list items stay under their marker. Words wider than a row are
/// split at a character boundary rather than overflowing.
pub fn wrap(lines: Vec<Line<'static>>, width: u16) -> Vec<Line<'static>> {
    let width = usize::from(width.max(1));
    let mut out = Vec::new();
    for line in lines {
        let indent: String = line
            .spans
            .first()
            .map(|s| s.content.chars().take_while(|c| *c == ' ').collect())
            .unwrap_or_default();
        let indent = if indent.width() >= width {
            String::new()
        } else {
            indent
        };
        let mut cur: Vec<Span<'static>> = Vec::new();
        let mut col = 0;
        for span in line.spans {
            let style = span.style;
            for word in span.content.split_inclusive(' ') {
                let w = word.trim_end().width();
                if col > 0 && col + w > width {
                    out.push(Line::from(std::mem::take(&mut cur)));
                    if !indent.is_empty() {
                        cur.push(Span::raw(indent.clone()));
                    }
                    col = indent.width();
                }
                let mut rest = word;
                while rest.width() > width - col {
                    let mut take = 0;
                    let mut acc = 0;
                    for (i, ch) in rest.char_indices() {
                        let cw = ch.width().unwrap_or(0);
                        if acc + cw > width - col {
                            break;
                        }
                        acc += cw;
                        take = i + ch.len_utf8();
                    }
                    if take == 0 {
                        break;
                    }
                    cur.push(Span::styled(rest[..take].to_string(), style));
                    out.push(Line::from(std::mem::take(&mut cur)));
                    if !indent.is_empty() {
                        cur.push(Span::raw(indent.clone()));
                    }
                    col = indent.width();
                    rest = &rest[take..];
                }
                col += rest.width();
                cur.push(Span::styled(rest.to_string(), style));
            }
        }
        out.push(Line::from(cur));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[Line<'_>]) -> Vec<String> {
        lines.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn wrap_breaks_at_words_and_keeps_the_indent() {
        let lines = wrap(vec![Line::raw("  • one two three four")], 12);
        assert_eq!(text(&lines), ["  • one two ", "  three four"]);
    }

    #[test]
    fn wrap_splits_a_word_wider_than_the_row_by_display_width() {
        let lines = wrap(vec![Line::raw("ééééééé 漢字漢字")], 4);
        assert_eq!(text(&lines), ["éééé", "ééé ", "漢字", "漢字"]);
    }

    #[test]
    fn wrap_keeps_span_styles_across_the_break() {
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let lines = wrap(vec![Line::from(vec![Span::styled("aa bb", bold)])], 3);
        assert_eq!(text(&lines), ["aa ", "bb"]);
        assert_eq!(lines[1].spans[0].style, bold);
    }
}
