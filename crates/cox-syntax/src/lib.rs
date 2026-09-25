//! Tree-sitter and its five grammar crates (T32.4; `docs/design/crates.md`
//! C4): `outline`, the pure AST-signature engine behind `read`'s
//! `mode=outline`, and `parse_bash`, the one piece of parser setup
//! `bash/classify.rs` needs to turn a command line into a walkable tree.
//! Separate from `cox-tools` because each grammar is its own C build
//! (dependency (a)) and neither consumer needs the rest of the tool set.
//!
//! `cox-tools` re-exports `outline` at its old path. `classify.rs`'s risk
//! walk itself (domain logic, not parsing) stays in `cox-tools`; it reaches
//! this crate through `parse_bash` and the re-exported `Node` type it
//! walks.

pub mod outline;

pub use tree_sitter::Node;

/// Parses `command` as bash, for `bash/classify.rs`'s risk walk. `None`
/// when the grammar cannot be loaded (it always can here) or the parser
/// gives up outright; `classify` treats that the same as a command it
/// cannot make sense of (`Risk::Exec`).
pub fn parse_bash(command: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_bash::LANGUAGE.into())
        .ok()?;
    parser.parse(command, None)
}
