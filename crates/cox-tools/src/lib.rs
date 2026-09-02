//! Built-in tools and the sandbox (Seatbelt, Landlock/bwrap): read, edit,
//! write, bash, grep, glob, outline, web, todo, ask_user, agent. Separate
//! from `cox-core` because every tool touches the filesystem or a process
//! and must go through a trait, never called directly by the loop.

pub mod ask_user;
pub mod bash;
pub mod edit;
pub mod expand;
pub mod glob;
pub mod grep;
pub mod outline;
pub mod path;
pub mod read;
pub mod sandbox;
pub mod todo;
pub mod tool_search;
pub mod v4a;
pub mod web_fetch;
pub mod write;

use std::path::PathBuf;
use std::sync::Arc;

use cox_protocol::{Archive, CallId, SandboxPolicy, SessionId, ToolCx};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Assembles a `ToolCx` from its parts. Every field on `ToolCx`
/// (`cox-protocol::traits`) is already `pub`, so this is a thin, named
/// constructor rather than a real builder — it exists so callers (session
/// setup in `cox-core`, tool tests here) have one place to look instead of
/// repeating the struct literal. Wiring this up from live session config
/// (`T2.2`/`T0.3`) is a later, separate task: every argument here is a
/// plain value the caller must already have in hand.
#[allow(clippy::too_many_arguments)]
pub fn tool_cx(
    roots: Vec<PathBuf>,
    cwd: PathBuf,
    sandbox: SandboxPolicy,
    archive: Arc<dyn Archive>,
    cancel: CancellationToken,
    output: mpsc::Sender<String>,
    session: SessionId,
    call: CallId,
) -> ToolCx {
    ToolCx {
        roots,
        cwd,
        sandbox,
        archive,
        cancel,
        output,
        session,
        call,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use cox_protocol::{ArchiveId, ArchivePut, SandboxMode, StoreError};

    use super::*;

    struct NoopArchive;

    #[async_trait::async_trait]
    impl Archive for NoopArchive {
        async fn put(&self, _put: ArchivePut) -> Result<ArchiveId, StoreError> {
            Ok(ArchiveId::new())
        }
        async fn get(&self, _id: &ArchiveId) -> Result<Vec<u8>, StoreError> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn tool_cx_wires_every_field_through() {
        let (tx, _rx) = mpsc::channel(1);
        let roots = vec![PathBuf::from("/tmp/root")];
        let cwd = PathBuf::from("/tmp/root");
        let sandbox = SandboxPolicy {
            mode: SandboxMode::WorkspaceWrite,
            network: false,
            writable: vec![],
            readonly_in_workspace: vec![],
        };
        let session = SessionId::new();
        let call = CallId::new();

        let cx = tool_cx(
            roots.clone(),
            cwd.clone(),
            sandbox,
            Arc::new(NoopArchive),
            CancellationToken::new(),
            tx,
            session,
            call,
        );

        assert_eq!(cx.roots, roots);
        assert_eq!(cx.cwd, Path::new("/tmp/root"));
        assert_eq!(cx.session, session);
        assert_eq!(cx.call, call);
    }
}
