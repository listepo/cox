//! The MCP client over `rmcp`: `.mcp.json`/config discovery, OAuth, and
//! tool namespacing. Separate from `cox-tools` because MCP servers are
//! untrusted network peers, not built-in tools.
//! `server` is the other direction: cox's own tools offered to another
//! agent over stdio (`cox mcp`). `elicit` maps a server's
//! `elicitation/create` form onto the questions a person answers (T47.1).

pub mod auth;
pub mod client;
pub mod discovery;
pub mod elicit;
pub mod server;
