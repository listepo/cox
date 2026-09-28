//! The built-in tool set a session starts from, and the swaps that adapt
//! it to a surface: ACP client-backed file and shell tools (T11.1), an
//! `ask_user` that surfaces questions (T22.1), and a `tool_search` index
//! over the final list (D6d). Separate because `cox mcp` and ACP take the
//! same tools without the rest of `open`.

use std::path::PathBuf;
use std::sync::Arc;

use cox_protocol::traits::Tool;
use cox_store::Store;
use cox_tools::ask_user::{Answers, AskUserTool, Question as AskUserQuestion};
use cox_tools::bash::BashTool;
use cox_tools::edit::EditTool;
use cox_tools::expand::ExpandTool;
use cox_tools::glob::GlobTool;
use cox_tools::grep::GrepTool;
use cox_tools::memory::{MemorySaveTool, MemorySearchTool};
use cox_tools::read::ReadTool;
use cox_tools::todo::TodoTool;
use cox_tools::tool_search::ToolSearchTool;
use cox_tools::v4a::ApplyPatchTool;
use cox_tools::web_fetch::WebFetchTool;
use cox_tools::write::WriteTool;

/// Every built-in tool except `agent`, which the session adds itself.
pub fn tools(answer: Option<String>, store: &Arc<Store>, mdir: PathBuf) -> Vec<Arc<dyn Tool>> {
    let mem: Arc<dyn cox_protocol::Store> = store.clone();
    let mut tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(ReadTool),
        Arc::new(EditTool),
        Arc::new(WriteTool),
        Arc::new(ApplyPatchTool),
        Arc::new(BashTool),
        Arc::new(GrepTool),
        Arc::new(GlobTool),
        Arc::new(TodoTool),
        Arc::new(ExpandTool),
        Arc::new(WebFetchTool::new()),
        // Headless/ACP/MCP: `--answer` or nothing. `open` swaps this for
        // `Answers::Surface` when a caller passes `questions` (T22.1).
        Arc::new(AskUserTool::new(Answers::Fixed(answer))),
        Arc::new(MemorySaveTool::new(mem.clone(), mdir.clone())),
        Arc::new(MemorySearchTool::new(mem, mdir)),
    ];
    let specs: Vec<_> = tools.iter().map(|t| t.spec()).collect();
    tools.push(Arc::new(ToolSearchTool::new(specs)));
    tools
}

/// Swaps local file/shell tools for client-backed ones where the ACP
/// client offers `fs`/`terminal` (T11.1 step 4), so the editor's buffers
/// stay authoritative. Names, subjects and risk classes are unchanged.
pub fn with_client_tools(
    tools: Vec<Arc<dyn Tool>>,
    link: cox_acp::ClientLink,
    fs: bool,
    terminal: bool,
) -> Vec<Arc<dyn Tool>> {
    use cox_acp::client_tools::{FsEditTool, FsReadTool, FsWriteTool, TerminalBashTool};
    let link = std::sync::Arc::new(link);
    tools
        .into_iter()
        .map(|t| match t.spec().name.as_str() {
            "read" if fs => Arc::new(FsReadTool::new(link.clone())) as Arc<dyn Tool>,
            "edit" if fs => Arc::new(FsEditTool::new(link.clone())) as Arc<dyn Tool>,
            "write" if fs => Arc::new(FsWriteTool::new(link.clone())) as Arc<dyn Tool>,
            "bash" if terminal => Arc::new(TerminalBashTool::new(link.clone())) as Arc<dyn Tool>,
            _ => t,
        })
        .collect()
}

/// Rebuilds `tool_search` over the complete tool list — built-ins, the
/// deferred `skill` (T22.2), MCP and plugin tools (T33.12) — by the
/// swap-by-name shape of `with_question_surface`: it answers from the specs
/// it was built with, so a deferred tool missing here could never be
/// discovered (D6d).
pub(crate) fn with_tool_search_index(tools: Vec<Arc<dyn Tool>>) -> Vec<Arc<dyn Tool>> {
    let specs: Vec<_> = tools.iter().map(|t| t.spec()).collect();
    tools
        .into_iter()
        .map(|t| match t.spec().name.as_str() {
            "tool_search" => Arc::new(ToolSearchTool::new(specs.clone())) as Arc<dyn Tool>,
            _ => t,
        })
        .collect()
}

/// Swaps the fixed-answer `ask_user` `tools()` built for one that surfaces
/// each question instead (T22.1): only a surface with somewhere to show it.
/// Same swap-by-name shape as `with_client_tools`; the spec is unchanged
/// (`AskUserTool::spec` never reads `answers`), so `tool_search`'s cached
/// schema, built from the pre-swap tools, still matches.
pub(crate) fn with_question_surface(
    tools: Vec<Arc<dyn Tool>>,
    tx: tokio::sync::mpsc::Sender<AskUserQuestion>,
) -> Vec<Arc<dyn Tool>> {
    tools
        .into_iter()
        .map(|t| match t.spec().name.as_str() {
            "ask_user" => Arc::new(AskUserTool::new(Answers::Surface(tx.clone()))) as Arc<dyn Tool>,
            _ => t,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cox_protocol::ids::SessionId;
    use cox_protocol::traits::Store as _;

    struct NoopArchive;

    #[async_trait::async_trait]
    impl cox_protocol::Archive for NoopArchive {
        async fn put(
            &self,
            _put: cox_protocol::ArchivePut,
        ) -> Result<cox_protocol::ArchiveId, cox_protocol::StoreError> {
            Ok(cox_protocol::ArchiveId::new())
        }
        async fn get(
            &self,
            _id: &cox_protocol::ArchiveId,
        ) -> Result<Vec<u8>, cox_protocol::StoreError> {
            Ok(Vec::new())
        }
    }

    /// T22.1: `open` with `questions` swaps `tools()`'s fixed-answer `ask_user`
    /// for one whose answers surface on the channel it is given, so a
    /// question the tool asks reaches whoever is listening on `tx` and the
    /// reply they send back is what the call returns.
    #[tokio::test]
    async fn tui_question_surface_is_wired() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(Store::open(tmp.path()).expect("open store"));
        let mdir = tmp.path().join("memory");
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let built = with_question_surface(tools(None, &store, mdir), tx);
        let ask_user = built
            .iter()
            .find(|t| t.spec().name == "ask_user")
            .expect("ask_user tool present")
            .clone();

        let (out_tx, _out_rx) = tokio::sync::mpsc::channel(1);
        let cx = cox_tools::tool_cx(
            vec![tmp.path().to_path_buf()],
            tmp.path().to_path_buf(),
            cox_protocol::SandboxPolicy {
                mode: cox_protocol::types::SandboxMode::ReadOnly,
                network: false,
                writable: vec![],
                readonly_in_workspace: vec![],
                linux_backend: Default::default(),
            },
            Arc::new(NoopArchive) as Arc<dyn cox_protocol::Archive>,
            tokio_util::sync::CancellationToken::new(),
            out_tx,
            SessionId::new(),
            cox_protocol::ids::CallId::new(),
        );

        let surface = tokio::spawn(async move {
            let q = rx
                .recv()
                .await
                .expect("question surfaced on the swapped channel");
            assert_eq!(q.question, "pick one");
            let _ = q.reply.send("b".into());
        });
        let out = ask_user
            .call(
                serde_json::json!({"question": "pick one", "options": ["a", "b"]}),
                &cx,
            )
            .await
            .expect("answered through the surface");
        assert_eq!(out.text, "b");
        surface.await.expect("surface task");
    }
}
