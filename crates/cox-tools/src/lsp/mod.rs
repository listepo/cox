//! Language-server support for the `diagnostics` tool (P41). Lives in
//! `cox-tools` because talking to a server means a process and its pipes,
//! which the core never touches (D2); split into modules so the wire, the
//! server lifecycle and the tool can each be tested alone.

pub mod client;
pub mod diag;
