//! Markdown → ratatui lines (T5.3): pulldown-cmark events become styled
//! `Span`s, fenced code goes through syntect, tables become aligned text.
//! Separate from the cells so an assistant reply and a compaction summary
//! render through one path and a test can check the mapping on a string.

use std::sync::LazyLock;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use unicode_width::UnicodeWidthStr;

use crate::cells::Look;
use crate::glyph::Glyphs;

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEMES: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

/// The syntect theme to highlight with: `tui.syntax_theme` when it names one
/// of syntect's bundled themes, otherwise the `tui.theme` default (`auto`
/// reads as dark because most terminals are).
pub fn theme_name(dark: bool, chosen: &'static str) -> &'static str {
    if THEMES.themes.contains_key(chosen) {
        return chosen;
    }
    if dark {
        "base16-ocean.dark"
    } else {
        "base16-ocean.light"
    }
}

/// The bundled theme names, for `cox`'s startup warning about an unknown
/// `tui.syntax_theme` — a bad name falls back, it never fails the session.
pub fn themes() -> Vec<String> {
    THEMES.themes.keys().cloned().collect()
}

/// Renders `text` as lines, unwrapped; trailing blank lines are dropped so a
/// streaming reply never shows a gap under its last paragraph.
pub fn render(text: &str, look: &Look) -> Vec<Line<'static>> {
    let mut r = Renderer {
        theme: look.theme,
        glyphs: look.glyphs,
        ..Renderer::default()
    };
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    for ev in Parser::new_ext(text, opts) {
        r.event(ev);
    }
    r.flush();
    while r.lines.last().is_some_and(|l| l.spans.is_empty()) {
        r.lines.pop();
    }
    r.lines
}

#[derive(Default)]
struct Renderer {
    theme: &'static str,
    glyphs: Glyphs,
    lines: Vec<Line<'static>>,
    cur: Vec<Span<'static>>,
    styles: Vec<Style>,
    /// Next number per open ordered list (`None` for bullets), innermost last.
    lists: Vec<Option<u64>>,
    /// Open fenced block: language and body so far.
    code: Option<(String, String)>,
    /// Open table: rows so far and the cell being filled.
    table: Option<(Vec<Vec<String>>, String)>,
    quote: usize,
}

impl Renderer {
    fn style(&self) -> Style {
        self.styles.last().copied().unwrap_or_default()
    }

    fn push(&mut self, m: Modifier) {
        self.styles.push(self.style().add_modifier(m));
    }

    fn text(&mut self, s: &str) {
        if let Some((_, body)) = &mut self.code {
            body.push_str(s);
        } else if let Some((_, cell)) = &mut self.table {
            cell.push_str(s);
        } else {
            self.cur.push(Span::styled(s.to_string(), self.style()));
        }
    }

    fn flush(&mut self) {
        if self.cur.is_empty() {
            return;
        }
        let mut spans = std::mem::take(&mut self.cur);
        if self.quote > 0 {
            spans.insert(
                0,
                Span::styled(
                    format!("{} ", self.glyphs.quote).repeat(self.quote),
                    Style::default().add_modifier(Modifier::DIM),
                ),
            );
        }
        self.lines.push(Line::from(spans));
    }

    /// Ends the open line and separates it from the next block.
    fn blank(&mut self) {
        self.flush();
        if self.lines.last().is_some_and(|l| !l.spans.is_empty()) {
            self.lines.push(Line::default());
        }
    }

    fn event(&mut self, ev: Event<'_>) {
        match ev {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) | Event::Html(t) | Event::InlineHtml(t) => self.text(&t),
            Event::Code(c) if self.table.is_some() => self.text(&c),
            Event::Code(c) => self
                .cur
                .push(Span::styled(c.into_string(), self.style().fg(Color::Cyan))),
            Event::SoftBreak => self.text(" "),
            Event::HardBreak => self.flush(),
            Event::Rule => {
                self.blank();
                self.lines.push(Line::styled(
                    self.glyphs.rule.repeat(3),
                    Style::default().add_modifier(Modifier::DIM),
                ));
                self.lines.push(Line::default());
            }
            Event::TaskListMarker(done) => self.text(if done { "[x] " } else { "[ ] " }),
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { level, .. } => {
                self.blank();
                self.push(Modifier::BOLD);
                let hashes = "#".repeat(level as usize);
                self.cur
                    .push(Span::styled(format!("{hashes} "), self.style()));
            }
            Tag::BlockQuote(..) => {
                self.blank();
                self.quote += 1;
            }
            Tag::CodeBlock(kind) => {
                self.blank();
                let lang = match kind {
                    CodeBlockKind::Fenced(l) => l.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code = Some((lang, String::new()));
            }
            Tag::List(start) => {
                if self.lists.is_empty() {
                    self.blank();
                }
                self.lists.push(start);
            }
            Tag::Item => {
                self.flush();
                let depth = self.lists.len().saturating_sub(1);
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        let m = format!("{n}. ");
                        *n += 1;
                        m
                    }
                    _ => format!("{} ", self.glyphs.bullet),
                };
                self.cur
                    .push(Span::raw(format!("{}{marker}", "  ".repeat(depth))));
            }
            Tag::Emphasis => self.push(Modifier::ITALIC),
            Tag::Strong => self.push(Modifier::BOLD),
            Tag::Strikethrough => self.push(Modifier::CROSSED_OUT),
            Tag::Link { .. } => self.push(Modifier::UNDERLINED),
            Tag::Table(_) => {
                self.blank();
                self.table = Some((Vec::new(), String::new()));
            }
            Tag::TableHead | Tag::TableRow => {
                if let Some((rows, _)) = &mut self.table {
                    rows.push(Vec::new());
                }
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Heading(_) => {
                self.styles.pop();
                self.blank();
            }
            // Inside a list or quote a paragraph break is just a line break.
            TagEnd::Paragraph if self.lists.is_empty() && self.quote == 0 => self.blank(),
            TagEnd::Paragraph | TagEnd::Item => self.flush(),
            TagEnd::BlockQuote(..) => {
                self.flush();
                self.quote = self.quote.saturating_sub(1);
                self.blank();
            }
            TagEnd::CodeBlock => {
                if let Some((lang, body)) = self.code.take() {
                    let rows: Vec<&str> = body.lines().collect();
                    self.lines.extend(highlight(&lang, &rows, self.theme));
                    self.lines.push(Line::default());
                }
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
                if self.lists.is_empty() {
                    self.blank();
                }
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                self.styles.pop();
            }
            TagEnd::TableCell => {
                if let Some((rows, cell)) = &mut self.table {
                    let c = std::mem::take(cell);
                    if let Some(row) = rows.last_mut() {
                        row.push(c);
                    }
                }
            }
            TagEnd::Table => {
                if let Some((rows, _)) = self.table.take() {
                    self.lines.extend(table_lines(&rows, &self.glyphs));
                    self.lines.push(Line::default());
                }
            }
            _ => {}
        }
    }
}

/// `rows` through syntect, as one run so a multi-line string or comment
/// keeps its state. `token` is a language name or a file extension; an
/// unknown one, or a theme missing from the bundle, falls back to plain text
/// rather than failing. Shared by fenced blocks, file-shaped tool output and
/// diff hunks, so all three highlight identically.
pub fn highlight(token: &str, rows: &[&str], theme: &str) -> Vec<Line<'static>> {
    let syntax = SYNTAXES
        .find_syntax_by_token(token)
        .unwrap_or_else(|| SYNTAXES.find_syntax_plain_text());
    let Some(theme) = THEMES.themes.get(theme) else {
        return rows.iter().map(|l| Line::raw((*l).to_string())).collect();
    };
    let mut h = HighlightLines::new(syntax, theme);
    rows.iter()
        .map(|l| {
            // The newline-aware syntaxes want the terminator to close scopes.
            let with_nl = format!("{l}\n");
            match h.highlight_line(&with_nl, &SYNTAXES) {
                Ok(regions) => Line::from(
                    regions
                        .into_iter()
                        .map(|(st, s)| {
                            let fg = st.foreground;
                            Span::styled(
                                s.trim_end_matches('\n').to_string(),
                                Style::default().fg(Color::Rgb(fg.r, fg.g, fg.b)),
                            )
                        })
                        .collect::<Vec<_>>(),
                ),
                Err(_) => Line::raw(l.to_string()),
            }
        })
        .collect()
}

/// Columns padded to their widest cell, header bold over a rule.
fn table_lines(rows: &[Vec<String>], g: &Glyphs) -> Vec<Line<'static>> {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..cols)
        .map(|c| {
            rows.iter()
                .filter_map(|r| r.get(c))
                .map(|s| s.width())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let fmt = |r: &Vec<String>| {
        (0..cols)
            .map(|c| {
                let s = r.get(c).map_or("", String::as_str);
                format!("{s}{}", " ".repeat(widths[c].saturating_sub(s.width())))
            })
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end()
            .to_string()
    };
    let mut out = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        if i == 0 {
            out.push(Line::styled(
                fmt(r),
                Style::default().add_modifier(Modifier::BOLD),
            ));
            out.push(Line::styled(
                widths
                    .iter()
                    // Repeated to the column width, not past it: an
                    // overridden rule glyph may be two columns wide.
                    .map(|w| g.rule.repeat(w / g.rule.width().max(1)))
                    .collect::<Vec<_>>()
                    .join("  "),
                Style::default().add_modifier(Modifier::DIM),
            ));
        } else {
            out.push(Line::raw(fmt(r)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[Line<'_>]) -> Vec<String> {
        lines.iter().map(ToString::to_string).collect()
    }

    fn look(glyphs: Glyphs) -> Look {
        Look {
            width: 80,
            theme: theme_name(true, ""),
            glyphs,
            show_thinking: false,
            show_diffs: true,
            tick: 0,
            marks: false,
        }
    }

    #[test]
    fn headings_lists_and_inline_code_keep_their_markers() {
        let lines = render(
            "# Title\n\nSome `code` here\n\n- one\n- two\n  - nested\n\n1. a\n2. b",
            &look(crate::glyph::UNICODE),
        );
        assert_eq!(
            text(&lines),
            [
                "# Title",
                "",
                "Some code here",
                "",
                "• one",
                "• two",
                "  • nested",
                "",
                "1. a",
                "2. b"
            ]
        );
        assert!(
            lines[0].spans[0]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
    }

    #[test]
    fn fenced_code_is_highlighted_per_line() {
        let lines = render(
            "```rust\nfn main() {}\nlet x = 1;\n```",
            &look(crate::glyph::UNICODE),
        );
        assert_eq!(text(&lines), ["fn main() {}", "let x = 1;"]);
        // `fn` is a keyword, so syntect gave it a colour of its own.
        assert!(lines[0].spans.len() > 1);
    }

    #[test]
    fn tables_align_columns_under_a_bold_header() {
        let lines = render(
            "| a | bb |\n|---|---|\n| ccc | d |",
            &look(crate::glyph::UNICODE),
        );
        assert_eq!(text(&lines), ["a    bb", "───  ──", "ccc  d"]);
    }

    #[test]
    fn the_ascii_set_replaces_every_markdown_glyph() {
        let lines = render(
            "- one\n\n---\n\n> quoted\n\n| a | bb |\n|---|---|\n| ccc | d |",
            &look(crate::glyph::ASCII),
        );
        let rendered = text(&lines).join("\n");
        assert!(rendered.is_ascii(), "{rendered:?}");
        assert!(rendered.contains("- one"), "{rendered:?}");
        assert!(rendered.contains("| quoted"), "{rendered:?}");
        assert!(rendered.contains("---  --"), "{rendered:?}");
    }

    #[test]
    fn open_fence_while_streaming_still_renders_as_code() {
        let lines = render("text\n\n```sh\necho hi", &look(crate::glyph::UNICODE));
        assert_eq!(text(&lines), ["text", "", "echo hi"]);
    }

    #[test]
    fn a_file_extension_highlights_like_a_language_token() {
        let by_ext = highlight("rs", &["fn main() {}"], theme_name(true, ""));
        let by_lang = highlight("rust", &["fn main() {}"], theme_name(true, ""));
        assert!(by_ext[0].spans.len() > 1);
        assert_eq!(by_ext[0].spans[0].style, by_lang[0].spans[0].style);
    }

    #[test]
    fn an_unknown_theme_renders_plain_instead_of_failing() {
        assert_eq!(theme_name(true, "no-such-theme"), "base16-ocean.dark");
        assert_eq!(theme_name(false, "no-such-theme"), "base16-ocean.light");
        assert_eq!(theme_name(true, "InspiredGitHub"), "InspiredGitHub");
        let lines = highlight("rs", &["fn main() {}"], "no-such-theme");
        assert_eq!(text(&lines), ["fn main() {}"]);
        assert_eq!(lines[0].spans.len(), 1);
    }
}
