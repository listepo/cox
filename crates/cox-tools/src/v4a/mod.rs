//! `apply_patch`: Codex's V4A patch grammar (plan.md D4/D8, T3.5). Adopted
//! verbatim rather than invented because OpenAI models are trained to emit
//! it — a cox-specific grammar would turn every one of those completions
//! into an edit failure.
//!
//! Split in two because the halves fail differently and are tested
//! differently: [`parse`] is a pure text ↔ AST bijection (its property is
//! `parse(print(p)) == p`, and it is what the fuzz target drives), while
//! [`apply`] is the fallible part — it resolves each hunk against a file
//! that has moved on since the model read it, and owns the all-or-nothing
//! write.

pub mod apply;
pub mod parse;

pub use apply::ApplyPatchTool;
pub use parse::{Hunk, HunkLine, Op, Patch, parse};
