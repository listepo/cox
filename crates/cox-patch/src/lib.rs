//! The V4A patch engine: Codex's `apply_patch` grammar (plan.md D4/D8),
//! adopted verbatim because OpenAI models are trained to emit it — a
//! cox-specific grammar would turn every one of those completions into an
//! edit failure. [`parse`] is a pure text ↔ AST bijection; [`stage`]
//! resolves a [`Patch`] against whatever a `read` closure returns, matching
//! each hunk progressively (exact, then ignoring trailing whitespace, then
//! ignoring all whitespace) because every failure here is the file having
//! moved on since the model read it.
//!
//! Pure — no filesystem, no `ToolCx`. The `apply_patch` `Tool` impl lives in
//! `cox_tools::v4a::tool`, not here: it resolves paths through
//! `cox_tools::path::confine` (the workspace-escape guard) and writes
//! through `cox_tools::write`, so it needs `cox-tools` underneath it.
//! Splitting the pure engine out (T32.6) keeps this crate a leaf —
//! `cox-protocol` only — while `confine` stays a single call site
//! (docs/design/crates.md, AGENTS.md trust boundaries).

pub mod apply;
pub mod parse;

pub use apply::{Change, stage};
pub use parse::{Hunk, HunkLine, Op, Patch, parse};
