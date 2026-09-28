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
    /// Prose, laid out: list markers and quote bars are runs of their own.
    Text {
        kind: TextKind,
        lines: Vec<StyledLine>,
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

#[cfg(test)]
mod tests {
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
                super::Block::Text { lines, .. } => lines.concat(),
                _ => Vec::new(),
            })
            .filter_map(|s| s.link.map(|l| (s.text, l)))
            .collect();
        let url = "https://x.dev/a".to_string();
        assert_eq!(links, [(url.clone(), url)]);
    }
}
