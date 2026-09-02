//! The five traits every other crate implements against instead of a
//! concrete type (plan.md §1.2). This is the enforcement point for
//! AGENTS.md's rule that anything touching the network, filesystem or a
//! process lives behind a trait defined here: `cox-core` depends only on
//! these signatures, never on `cox-provider`/`cox-tools`/`cox-mcp`/`cox-store`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::errors::{ProviderError, StoreError, ToolError};
use crate::ids::{ArchiveId, CallId, SessionId};
use crate::types::{
    Caps, ProviderEvent, ProviderId, Request, Risk, SandboxPolicy, ToolOutput, ToolSpec, Usage,
};

/// A row inserted for a new session (`Store::session_create`), matching the
/// `sessions` table (plan.md §1.7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionRow {
    /// The session's id.
    pub id: SessionId,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// The working directory the session started in.
    pub cwd: PathBuf,
    /// The project slug used for memory/config lookup.
    pub project_slug: String,
    /// The session's title, if one has been generated.
    pub title: Option<String>,
    /// The parent session, for subagents.
    pub parent_id: Option<SessionId>,
    /// Where the JSONL rollout for this session lives.
    pub rollout_path: PathBuf,
}

/// A row inserted for one provider call (`Store::usage_insert`), matching
/// the `usage` table (plan.md §1.7). "A cost that is not a `usage` row in
/// the ledger does not exist" — AGENTS.md.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageRow {
    /// The session this call belongs to.
    pub session_id: SessionId,
    /// Turn number within the session.
    pub turn: u32,
    /// The job this call served.
    pub job: crate::types::Job,
    /// The tier routed to.
    pub tier: crate::types::Tier,
    /// Which provider backend served the call.
    pub provider: ProviderId,
    /// The specific model used.
    pub model: crate::types::ModelId,
    /// The recorded usage (tokens, cost, latency).
    pub usage: Usage,
}

/// What `Store::archive_put`/`Archive::put` writes: the full, untruncated
/// bytes for one tool call's output, before the model ever sees the
/// truncated form (plan.md §1.7/D6a).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArchivePut {
    /// The session the call ran in.
    pub session: SessionId,
    /// The call being archived.
    pub call: CallId,
    /// The tool name.
    pub tool: String,
    /// The call's subject (path, command line, URL), if any.
    pub subject: Option<String>,
    /// The full output bytes.
    pub bytes: Vec<u8>,
}

/// One memory search hit (`Store::memory_search`), backed by the
/// `memory_fts` virtual table (plan.md §1.7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryHit {
    /// The memory fact's name.
    pub name: String,
    /// Where it lives under `projects/<slug>/memory/`.
    pub path: PathBuf,
    /// A short excerpt around the match.
    pub snippet: String,
}

/// A model provider: turns a `Request` into a stream of `ProviderEvent`s.
/// Implemented by `cox-provider` for Anthropic/OpenAI/local backends and by
/// `Scripted`/`Replay` fakes for tests (D12); `cox-core` never depends on
/// which.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Which provider this is.
    fn id(&self) -> ProviderId;
    /// What this provider implementation can do, so `cox-core` can avoid
    /// sending it a request shape it does not support.
    fn capabilities(&self) -> Caps;
    /// Streams a response, forwarding `ProviderEvent`s on `sink` as they
    /// arrive; returns the call's final `Usage` once the stream ends, or
    /// stops early if `cancel` fires.
    async fn stream(
        &self,
        req: Request,
        sink: mpsc::Sender<ProviderEvent>,
        cancel: CancellationToken,
    ) -> Result<Usage, ProviderError>;
    /// Counts tokens for a request without sending it, when the provider supports it.
    async fn count_tokens(&self, req: &Request) -> Result<u32, ProviderError>;
}

/// Context handed to a running `Tool::call`: everything it needs that is
/// not part of its own input, and nothing it could use to bypass a trust
/// boundary (AGENTS.md) — paths still go through `cox_tools::path::confine`,
/// commands still go through `cox_tools::sandbox`.
pub struct ToolCx {
    /// Workspace roots the call may read/write within.
    pub roots: Vec<PathBuf>,
    /// The call's working directory.
    pub cwd: PathBuf,
    /// The sandbox policy in effect for this call.
    pub sandbox: SandboxPolicy,
    /// Where to archive the call's full output before truncation.
    pub archive: Arc<dyn Archive>,
    /// Shared cancellation for `Submission::Interrupt`.
    pub cancel: CancellationToken,
    /// A channel for streaming partial output (`Event::ToolCallOutput`).
    pub output: mpsc::Sender<String>,
    /// The session this call belongs to.
    pub session: SessionId,
    /// This call's id.
    pub call: CallId,
}

/// A built-in or MCP tool. Implemented by `cox-tools` (`read`, `edit`,
/// `bash`, …) and `cox-mcp` (one per discovered server tool).
#[async_trait]
pub trait Tool: Send + Sync {
    /// The tool's advertised name, schema, risk and concurrency.
    fn spec(&self) -> ToolSpec;
    /// What permission rules match this call on: the confined path, command
    /// line, URL, or namespaced MCP name.
    fn subject(&self, input: &Value) -> String;
    /// This *call's* risk, which is not always the tool's. `spec().risk` is
    /// a default: `apply_patch` is an ordinary write until the patch in
    /// front of it deletes more than five files, and only the input says
    /// which it is (plan.md §4 tool table, T3.5 step 4).
    fn risk(&self, _input: &Value) -> Risk {
        self.spec().risk
    }
    /// Runs the tool. `text` in the returned `ToolOutput` is untruncated;
    /// the core archives it and truncates what the model sees.
    async fn call(&self, input: Value, cx: &ToolCx) -> Result<ToolOutput, ToolError>;
}

/// The persistence layer: `~/.cox/cox.db` plus the JSONL rollouts
/// (plan.md §1.7). Sync on purpose (D9): hooks, tests and `cox stats` need
/// no async runtime to query it.
pub trait Store: Send + Sync {
    /// Opens (creating and migrating if needed) the store under `home`.
    fn open(home: &Path) -> Result<Self, StoreError>
    where
        Self: Sized;
    /// Records a new session.
    fn session_create(&self, s: &SessionRow) -> Result<(), StoreError>;
    /// Appends one event to a session's rollout, returning its sequence number.
    fn rollout_append(&self, id: &SessionId, ev: &crate::types::Event) -> Result<u64, StoreError>;
    /// Reads back a session's full rollout, in order.
    fn rollout_read(&self, id: &SessionId) -> Result<Vec<crate::types::Event>, StoreError>;
    /// Records one provider call's usage/cost.
    fn usage_insert(&self, row: &UsageRow) -> Result<(), StoreError>;
    /// Archives a tool call's full output, returning its id.
    fn archive_put(&self, a: &ArchivePut) -> Result<ArchiveId, StoreError>;
    /// Reads back archived bytes by id (`cox expand <id>`).
    fn archive_get(&self, id: &ArchiveId) -> Result<Vec<u8>, StoreError>;
    /// Full-text searches memory facts for a project.
    fn memory_search(&self, q: &str, limit: usize) -> Result<Vec<MemoryHit>, StoreError>;
}

/// Where a tool's full, pre-truncation output is written before the model
/// sees the shortened form (D6a: "the archive row exists before the model
/// sees truncated text"). A narrower, async-friendly view of `Store`'s
/// archive methods, since `Tool::call` runs in an async context and `Store`
/// is deliberately sync (D9); `cox-store`'s `Store` implementation is also
/// the concrete `Archive`, dispatched onto a blocking task.
#[async_trait]
pub trait Archive: Send + Sync {
    /// Archives bytes, returning their id.
    async fn put(&self, put: ArchivePut) -> Result<ArchiveId, StoreError>;
    /// Reads back archived bytes by id.
    async fn get(&self, id: &ArchiveId) -> Result<Vec<u8>, StoreError>;
}

/// A hook runner (`cox-ext`): executes one hook subprocess against the
/// Claude Code JSON protocol and reports its verdict. Never returns a
/// `Result` — a broken hook is always a `HookOutcome::Failed`, never a
/// panic or a fatal error (D14/AGENTS.md: "fail open on extensions").
#[async_trait]
pub trait Hook: Send + Sync {
    /// Runs the hook for `event` with `payload`, giving up after `timeout`.
    async fn run(
        &self,
        event: crate::types::HookEvent,
        payload: Value,
        timeout: Duration,
    ) -> crate::types::HookOutcome;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time check that the trait object types used in `ToolCx` and
    /// elsewhere are actually object-safe, since that is easy to break by
    /// accident (e.g. adding a generic method).
    #[test]
    fn archive_and_provider_are_object_safe() {
        fn assert_object_safe<T: ?Sized>() {}
        assert_object_safe::<dyn Archive>();
        assert_object_safe::<dyn Provider>();
        assert_object_safe::<dyn Tool>();
        assert_object_safe::<dyn Hook>();
    }
}
