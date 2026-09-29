//! `StyledDoc` (T37.7, DT§4.2): what markdown and syntax highlighting
//! produce before any UI draws them — blocks of runs tagged with the plugin
//! `StyleToken` roles, never a terminal style. Separate from `markdown` so a
//! non-terminal consumer (the desktop app through `cox-app`) can hold the
//! types without ratatui; the ratatui conversion lives in `markdown`, behind
//! the `ratatui` feature the TUI enables.

pub use cox_protocol::plugin::ui::StyleToken;
use serde::{Deserialize, Serialize};

/// A rendered markdown text: top-level blocks in order. A terminal puts one
/// blank line between blocks; a GUI spaces them as it likes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyledDoc {
    pub blocks: Vec<Block>,
}

/// One row of runs.
pub type StyledLine = Vec<StyledSpan>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    /// Prose. A heading's `#` run, a list item's marker and a quote's bars
    /// are not in the text: each surface draws them from `kind` and the
    /// line's own fields (A92).
    Text {
        kind: TextKind,
        lines: Vec<TextLine>,
    },
    /// A fenced or indented block, highlighted line by line.
    Code {
        lang: String,
        lines: Vec<StyledLine>,
    },
    /// Plain cell text, header row first; each surface lays out columns.
    Table { rows: Vec<Vec<String>> },
    /// A thematic break.
    Rule,
}

/// What a `Block::Text` started as — a hint for a GUI's spacing and font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextKind {
    Paragraph,
    Heading(u8),
    List,
    Quote,
}

/// One line of a `Block::Text`: its runs and where it sits. A terminal
/// prints the bars and the marker in front of the runs; a GUI draws a bar
/// per quote and hangs the marker in a gutter. A heading's level is its
/// block's `TextKind::Heading`, drawn on the block's first line only.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextLine {
    /// How many quotes the line sits in: one bar each.
    #[serde(skip_serializing_if = "is_default")]
    pub quote: u8,
    /// How deep in nested lists the line is, 0 for a top-level item and
    /// outside any list.
    #[serde(skip_serializing_if = "is_default")]
    pub depth: u8,
    /// A list item's bullet glyph or number (`3.`) on the item's first
    /// line; empty on any other line.
    #[serde(skip_serializing_if = "is_default")]
    pub marker: String,
    pub spans: StyledLine,
}

/// A run of text with one style.
///
/// Serialized fields left at their default are omitted, so a desktop
/// fixture (DT§8) of a streamed reply stays readable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StyledSpan {
    pub text: String,
    /// Its colour role.
    #[serde(skip_serializing_if = "is_default")]
    pub token: StyleToken,
    /// A syntect theme colour for highlighted code; wins over `token`. One
    /// highlighter and one theme serve every surface (DT§4.2).
    #[serde(skip_serializing_if = "is_default")]
    pub rgb: Option<[u8; 3]>,
    /// The same run's colour under the theme's light variant, set where a
    /// surface picks by the system appearance (a diff, A95); `rgb` then
    /// holds the dark variant's.
    #[serde(skip_serializing_if = "is_default")]
    pub light: Option<[u8; 3]>,
    #[serde(skip_serializing_if = "is_default")]
    pub bold: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub italic: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub strike: bool,
    #[serde(skip_serializing_if = "is_default")]
    pub underline: bool,
    /// An `http(s)` target. Set only on runs whose drawn text *is* the URL
    /// (T23.3), so a link never hides where it goes behind other words.
    #[serde(skip_serializing_if = "is_default")]
    pub link: Option<String>,
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

impl StyledSpan {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }
}

/// The doc back as Markdown (T58.4.26): a streamed `docTail` carries blocks,
/// not source, so Copy as Markdown on every client rebuilds the source here
/// rather than each client writing its own rules.
impl StyledDoc {
    /// Every block as Markdown, joined by a blank line.
    pub fn markdown(&self) -> String {
        let blocks: Vec<String> = self.blocks.iter().filter_map(Block::markdown).collect();
        blocks.join("\n\n")
    }
}

impl Block {
    /// This block as Markdown; `None` for a table without rows.
    pub fn markdown(&self) -> Option<String> {
        match self {
            Block::Text { kind, lines } => Some(
                lines
                    .iter()
                    .enumerate()
                    .map(|(index, line)| line.markdown(*kind, index == 0))
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            Block::Code { lang, lines } => {
                let body: Vec<String> = lines
                    .iter()
                    .map(|line| line.iter().map(|s| s.text.as_str()).collect())
                    .collect();
                Some(fence(lang, &body.join("\n")))
            }
            Block::Table { rows } => {
                let head = rows.first()?;
                let row = |cells: &[String]| format!("| {} |", cells.join(" | "));
                let rule = vec!["---".to_string(); head.len()];
                let lines: Vec<String> = [row(head), row(&rule)]
                    .into_iter()
                    .chain(rows.iter().skip(1).map(|cells| row(cells)))
                    .collect();
                Some(lines.join("\n"))
            }
            Block::Rule => Some("---".into()),
        }
    }
}

impl TextLine {
    /// Its quotes as `>`, then an item's depth indent and its number or `-`,
    /// or a heading's `#` run on its first line; a list line that goes on an
    /// item is indented under it.
    fn markdown(&self, kind: TextKind, first: bool) -> String {
        let mut head = "> ".repeat(usize::from(self.quote));
        let mut heading = false;
        if !self.marker.is_empty() {
            let number = self.marker.chars().next().is_some_and(char::is_numeric);
            head += &"  ".repeat(usize::from(self.depth));
            head += if number { &self.marker } else { "-" };
            head.push(' ');
        } else if kind == TextKind::List {
            head += &"  ".repeat(usize::from(self.depth) + 1);
        } else if let TextKind::Heading(level) = kind {
            if first {
                head += &"#".repeat(usize::from(level));
                head.push(' ');
            }
            heading = true;
        }
        // A heading is bold by its level, so its spans' bold marks would only
        // double it.
        let spans = self
            .spans
            .iter()
            .map(|span| span.markdown(!heading && span.bold));
        head + &spans.collect::<String>()
    }
}

impl StyledSpan {
    /// The text with its `bold`, italic and strike marks; whitespace bare.
    fn markdown(&self, bold: bool) -> String {
        if self.text.chars().all(char::is_whitespace) {
            return self.text.clone();
        }
        let mut text = self.text.clone();
        if bold {
            text = format!("**{text}**");
        }
        if self.italic {
            text = format!("_{text}_");
        }
        if self.strike {
            text = format!("~~{text}~~");
        }
        text
    }
}

/// `body` fenced as `lang`, with a fence longer than any backtick run in it.
pub fn fence(lang: &str, body: &str) -> String {
    let mut ticks = "```".to_string();
    while body.contains(&ticks) {
        ticks.push('`');
    }
    format!("{ticks}{lang}\n{body}\n{ticks}")
}

#[cfg(test)]
mod tests {
    use super::{Block, StyledDoc, StyledSpan, TextKind, TextLine};
    use crate::glyph::UNICODE;
    use crate::markdown::{parse, theme_name};

    const FIXTURE: &str = "# Title\n\nSome `code` and **bold [docs](https://x.dev/a)** at \
        <https://x.dev/b>.\n\n## Steps\n\n- one\n- two with `ls`\n  - nested\n\n\
        1. first\n2. second\n\n```rust\nfn main() {}\n```\n\n> quoted\n\n---\n\n\
        | a | b |\n|---|---|\n| 1 | 2 |\n";

    #[test]
    fn markdown_parses_into_tagged_blocks_without_a_terminal() {
        insta::assert_debug_snapshot!(parse(FIXTURE, theme_name(true, ""), &UNICODE));
    }

    #[test]
    fn only_a_run_that_shows_its_url_carries_the_link() {
        let doc = parse("[docs](https://x.dev/a)", theme_name(true, ""), &UNICODE);
        let links: Vec<_> = doc
            .blocks
            .iter()
            .flat_map(|b| match b {
                super::Block::Text { lines, .. } => {
                    lines.iter().flat_map(|l| l.spans.clone()).collect()
                }
                _ => Vec::new(),
            })
            .filter_map(|s| s.link.map(|l| (s.text, l)))
            .collect();
        let url = "https://x.dev/a".to_string();
        assert_eq!(links, [(url.clone(), url)]);
    }

    /// Each text line as (kind, quote, depth, marker, its runs' text).
    fn lines(markdown: &str) -> Vec<(TextKind, u8, u8, String, String)> {
        let doc = parse(markdown, theme_name(true, ""), &UNICODE);
        doc.blocks
            .into_iter()
            .flat_map(|b| match b {
                Block::Text { kind, lines } => lines
                    .into_iter()
                    .map(|l| {
                        let text = l.spans.iter().map(|s| s.text.as_str()).collect();
                        (kind, l.quote, l.depth, l.marker, text)
                    })
                    .collect(),
                _ => Vec::new(),
            })
            .collect()
    }

    #[test]
    fn headings_quotes_and_items_carry_level_and_marker_apart_from_their_text() {
        let s = String::from;
        assert_eq!(
            lines("## Plan\n\n> outer\n>\n> > inner\n\n- one\n  - two\n\n3. three"),
            [
                (TextKind::Heading(2), 0, 0, s(""), s("Plan")),
                (TextKind::Quote, 1, 0, s(""), s("outer")),
                (TextKind::Quote, 2, 0, s(""), s("inner")),
                (TextKind::List, 0, 0, s("•"), s("one")),
                (TextKind::List, 0, 1, s("•"), s("two")),
                (TextKind::List, 0, 0, s("3."), s("three")),
            ]
        );
    }

    #[test]
    fn a_list_in_a_quote_keeps_its_bars_and_the_quote_resumes_after_it() {
        let s = String::from;
        assert_eq!(
            lines("> - item\n>\n> after"),
            [
                (TextKind::List, 1, 0, s("•"), s("item")),
                (TextKind::Quote, 1, 0, s(""), s("after")),
            ]
        );
    }

    fn line(quote: u8, depth: u8, marker: &str, spans: Vec<StyledSpan>) -> TextLine {
        TextLine {
            quote,
            depth,
            marker: marker.into(),
            spans,
        }
    }

    #[test]
    fn a_fence_outgrows_the_longest_backtick_run() {
        let code = Block::Code {
            lang: "md".into(),
            lines: vec![
                vec![StyledSpan::plain("```rust")],
                vec![StyledSpan::plain("````")],
            ],
        };
        assert_eq!(
            code.markdown().as_deref(),
            Some("`````md\n```rust\n````\n`````")
        );
    }

    #[test]
    fn a_nested_list_keeps_its_depth() {
        let list = Block::Text {
            kind: TextKind::List,
            lines: vec![
                line(0, 0, "•", vec![StyledSpan::plain("one")]),
                line(0, 1, "•", vec![StyledSpan::plain("two")]),
                line(0, 1, "", vec![StyledSpan::plain("more")]),
                line(1, 0, "3.", vec![StyledSpan::plain("three")]),
            ],
        };
        assert_eq!(
            list.markdown().as_deref(),
            Some("- one\n  - two\n    more\n> 3. three")
        );
    }

    #[test]
    fn a_heading_drops_its_bold_marks_and_a_run_keeps_italic_and_strike() {
        let bold = StyledSpan {
            bold: true,
            ..StyledSpan::plain("Plan")
        };
        let marked = StyledSpan {
            bold: true,
            italic: true,
            strike: true,
            ..StyledSpan::plain("x")
        };
        let space = StyledSpan {
            bold: true,
            ..StyledSpan::plain(" ")
        };
        let doc = StyledDoc {
            blocks: vec![
                Block::Text {
                    kind: TextKind::Heading(2),
                    lines: vec![line(0, 0, "", vec![bold])],
                },
                Block::Text {
                    kind: TextKind::Paragraph,
                    lines: vec![line(0, 0, "", vec![marked, space])],
                },
                Block::Rule,
            ],
        };
        assert_eq!(doc.markdown(), "## Plan\n\n~~_**x**_~~ \n\n---");
    }

    #[test]
    fn a_table_gets_its_rule_row_and_an_empty_one_is_left_out() {
        let s = String::from;
        let doc = StyledDoc {
            blocks: vec![
                Block::Table { rows: Vec::new() },
                Block::Table {
                    rows: vec![vec![s("a"), s("b")], vec![s("1"), s("2")]],
                },
            ],
        };
        assert_eq!(doc.markdown(), "| a | b |\n| --- | --- |\n| 1 | 2 |");
    }
}
