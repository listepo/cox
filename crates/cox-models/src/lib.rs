//! The model catalog (plan.md T30.24; `docs/design/providers.md` §
//! "Target shape" item 3): one pure crate mapping a model id to its
//! context window, max output, efforts, capabilities and price, merged
//! from three layers, each overriding the last by id — built-in rows
//! (embedded, written only by `cox-vendor models`, AGENTS.md A48) < a
//! `Config`'s `[providers.*].models` entries < a user-supplied
//! `prices.toml`.
//!
//! Depends only on `cox-protocol` (AGENTS.md layout table;
//! `crates/cox/tests/deps.rs` enforces it) and does no I/O beyond parsing
//! an embedded or caller-supplied string — a file on disk is the caller's
//! job (`cox-provider::usage::load_price_table`).

#![warn(missing_docs)]

mod catalog;
mod price;

pub use catalog::{Capabilities, Catalog, CatalogError, ModelRow};
pub use price::{Price, PriceError, PriceTable};
