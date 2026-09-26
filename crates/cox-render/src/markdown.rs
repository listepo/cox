//! Markdown → ratatui lines (T5.3): pulldown-cmark events become styled
//! `Span`s, fenced code goes through syntect, tables become aligned text.
//! Separate from the cells so an assistant reply and a compaction summary
//! render through one path and a test can check the mapping on a string.

use std::path::Path;
use std::sync::{LazyLock, OnceLock};

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use unicode_width::UnicodeWidthStr;

use crate::Look;
use crate::glyph::Glyphs;

static SYNTAXES: LazyLock<syntect::parsing::SyntaxSet> =
    LazyLock::new(two_face::syntax::extra_newlines);
static THEMES: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);
/// `.tmTheme` files under `~/.cox/themes/` (T24.2 step 4), merged on top of
/// the bundled set by `load_user_themes` once at startup; empty until then,
/// same as an unconfigured `tui.syntax_theme`.
static USER_THEMES: OnceLock<ThemeSet> = OnceLock::new();

/// Reads every `.tmTheme` in `dir` into `USER_THEMES`, so a name that
/// collides with a bundled theme picks the user's file. Fails open: a
/// missing `dir` or any file `syntect` rejects leaves the bundled set as
/// the whole story, never a startup error. Idempotent — a second call
/// (e.g. from a test) is a no-op, matching `glyph`'s "decided once" leak.
pub fn load_user_themes(dir: &Path) {
    if let Ok(set) = ThemeSet::load_from_folder(dir) {
        let _ = USER_THEMES.set(set);
    }
}

/// The syntect theme to highlight with: `tui.syntax_theme` when it names one
/// of syntect's bundled or user (T24.2) themes, otherwise the `tui.theme`
/// default (`auto` reads as dark because most terminals are).
pub fn theme_name(dark: bool, chosen: &'static str) -> &'static str {
    let known = |set: &ThemeSet| set.themes.contains_key(chosen);
    if THEMES.themes.contains_key(chosen) || USER_THEMES.get().is_some_and(known) {
        return chosen;
    }
    if dark {
        "base16-ocean.dark"
    } else {
        "base16-ocean.light"
    }
}

/// The bundled plus user (T24.2) theme names, for `cox`'s startup warning
/// about an unknown `tui.syntax_theme` — a bad name falls back, it never
/// fails the session.
pub fn themes() -> Vec<String> {
    let mut names: Vec<String> = THEMES.themes.keys().cloned().collect();
    if let Some(user) = USER_THEMES.get() {
        names.extend(user.themes.keys().cloned());
    }
    names
}

/// Renders `text` as lines, unwrapped; trailing blank lines are dropped so a
/// streaming reply never shows a gap under its last paragraph.
pub fn render(text: &str, look: &Look) -> Vec<Line<'static>> {
    let mut r = Renderer {
        theme: look.theme,
        glyphs: look.glyphs,
        width: usize::from(look.width),
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
    /// The viewport; a table wider than it becomes records (T24.7).
    width: usize,
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
    /// Open `http(s)` link (T23.3): its URL and where its text starts in `cur`.
    link: Option<(String, usize)>,
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

    /// T23.3: the URL is what becomes a hyperlink, so it is always on
    /// screen — an autolink is its own text, any other link gets ` (url)`.
    /// A link cannot hide where it goes behind friendlier words.
    fn end_link(&mut self) {
        let Some((url, at)) = self.link.take() else {
            return;
        };
        let text: String = self
            .cur
            .get(at..)
            .unwrap_or_default()
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        if text == url {
            for span in self.cur.iter_mut().skip(at) {
                *span = crate::link::mark(std::mem::take(span));
            }
            return;
        }
        let style = self.style();
        self.cur.push(Span::styled(" (", style));
        self.cur.push(crate::link::mark(Span::styled(url, style)));
        self.cur.push(Span::styled(")", style));
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
            Tag::Link { dest_url, .. } => {
                self.push(Modifier::UNDERLINED);
                if dest_url.starts_with("https://") || dest_url.starts_with("http://") {
                    self.link = Some((dest_url.into_string(), self.cur.len()));
                }
            }
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
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                self.styles.pop();
            }
            TagEnd::Link => {
                self.styles.pop();
                self.end_link();
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
                    self.lines
                        .extend(table_lines(&rows, &self.glyphs, self.width));
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
    let user = USER_THEMES.get().and_then(|set| set.themes.get(theme));
    let Some(theme) = user.or_else(|| THEMES.themes.get(theme)) else {
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

/// Columns padded to their widest cell, header bold over a rule; wider
/// than `width`, `Header: value` records instead, since a wrapped table row
/// no longer lines up with anything.
fn table_lines(rows: &[Vec<String>], g: &Glyphs, width: usize) -> Vec<Line<'static>> {
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
    let natural = widths.iter().sum::<usize>() + 2 * cols.saturating_sub(1);
    if let Some((head, body)) = rows.split_first()
        && !body.is_empty()
        && natural > width
    {
        return record_lines(head, body);
    }
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

/// Each row as one `Header: value` line per column, a blank line between
/// rows (Codex's narrow-table fallback).
fn record_lines(head: &[String], body: &[Vec<String>]) -> Vec<Line<'static>> {
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let mut out = Vec::new();
    for (i, row) in body.iter().enumerate() {
        if i > 0 {
            out.push(Line::default());
        }
        for (c, value) in row.iter().enumerate() {
            let key = head.get(c).map_or("", String::as_str);
            out.push(Line::from(vec![
                Span::styled(format!("{key}:"), bold),
                Span::raw(format!(" {value}")),
            ]));
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
            diff: crate::diff::Mode::Auto,
            tick: 0,
            still: false,
            marks: false,
            colors: crate::theme::Theme::dark(),
            expand_last: None,
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
