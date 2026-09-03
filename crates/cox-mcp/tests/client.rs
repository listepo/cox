//! T7.6: the client namespaces and gates a server's tools over an
//! in-process duplex, survives a server that dies mid-session, and
//! discovers servers from config, `.mcp.json` and `~/.claude.json`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cox_mcp::client::{McpClient, connect_all};
use cox_mcp::discovery::discover;
use cox_mcp::server::{CxTemplate, Gate, ToolServer};
use cox_protocol::config::McpServerConfig;
use cox_protocol::errors::{StoreError, ToolError};
use cox_protocol::ids::{ArchiveId, CallId, SessionId};
use cox_protocol::traits::{Archive, ArchivePut, Tool, ToolCx};
use cox_protocol::types::{
    Concurrency, LinuxBackend, Risk, SandboxMode, SandboxPolicy, ToolCall, ToolOutput, ToolSpec,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

struct Echo;

#[async_trait]
impl Tool for Echo {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "echo".into(),
            description: "echo".into(),
            input_schema: json!({"type":"object","properties":{"text":{"type":"string"}}}),
            deferred: false,
            risk: Risk::ReadOnly,
            concurrency: Concurrency::Parallel,
        }
    }
    fn subject(&self, input: &Value) -> String {
        input["text"].as_str().unwrap_or_default().to_string()
    }
    async fn call(&self, input: Value, _cx: &ToolCx) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput {
            text: format!("echo: {}", self.subject(&input)),
            is_error: false,
            diff: None,
            structured: None,
        })
    }
}

struct NoArchive;

#[async_trait]
impl Archive for NoArchive {
    async fn put(&self, _put: ArchivePut) -> Result<ArchiveId, StoreError> {
        Ok(ArchiveId::new())
    }
    async fn get(&self, _id: &ArchiveId) -> Result<Vec<u8>, StoreError> {
        Ok(Vec::new())
    }
}

struct Open;

impl Gate for Open {
    fn check(&self, _call: &ToolCall) -> Result<(), String> {
        Ok(())
    }
}

fn cx() -> ToolCx {
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    ToolCx {
        roots: vec![PathBuf::from("/tmp")],
        cwd: PathBuf::from("/tmp"),
        sandbox: SandboxPolicy {
            mode: SandboxMode::ReadOnly,
            network: false,
            writable: Vec::new(),
            readonly_in_workspace: Vec::new(),
            linux_backend: LinuxBackend::Auto,
        },
        archive: Arc::new(NoArchive),
        cancel: CancellationToken::new(),
        output: tx,
        session: SessionId::new(),
        call: CallId::new(),
    }
}

/// A `ToolServer` on one end of a duplex; returns the client end and the
/// server task so a test can kill the server.
async fn serve() -> (McpClient, tokio::task::JoinHandle<()>) {
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);
    let (sr, sw) = tokio::io::split(server_io);
    let server = ToolServer::new(
        vec![Arc::new(Echo)],
        Arc::new(Open),
        CxTemplate {
            roots: vec![PathBuf::from("/tmp")],
            cwd: PathBuf::from("/tmp"),
            sandbox: cx().sandbox,
            archive: Arc::new(NoArchive),
            session: SessionId::new(),
        },
    );
    let task = tokio::spawn(async move {
        use rmcp::ServiceExt;
        let running = server.serve((sr, sw)).await.expect("server handshake");
        let _ = running.waiting().await;
    });
    let (cr, cw) = tokio::io::split(client_io);
    let client = McpClient::from_transport("t", (cr, cw), Duration::from_secs(2))
        .await
        .expect("client handshake");
    (client, task)
}

#[tokio::test]
async fn client_round_trips_a_namespaced_call_over_a_duplex() {
    let (client, task) = serve().await;
    let tools = client.tools(true).await.expect("list");
    assert_eq!(tools.len(), 1);
    let spec = tools[0].spec();
    assert_eq!(spec.name, "mcp__t__echo");
    assert!(spec.deferred);
    // No annotations from the server: the default risk is `Write`.
    assert_eq!(spec.risk, Risk::Write);
    assert_eq!(spec.input_schema["type"], "object");
    assert_eq!(tools[0].subject(&json!({})), "mcp__t__echo");
    let out = tools[0]
        .call(json!({ "text": "hi" }), &cx())
        .await
        .expect("call");
    assert!(!out.is_error);
    assert_eq!(out.text, "echo: hi");
    client.close().await;
    task.abort();
}

#[tokio::test]
async fn client_server_crash_does_not_end_session() {
    let (client, task) = serve().await;
    let tools = client.tools(true).await.expect("list");
    task.abort();
    let _ = task.await;
    // The dead server is an error the model reads, not a panic or a hang.
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        tools[0].call(json!({ "text": "hi" }), &cx()),
    )
    .await
    .expect("no hang");
    match result {
        Ok(out) => assert!(out.is_error, "{}", out.text),
        Err(e) => assert!(matches!(e, ToolError::Timeout | ToolError::Io), "{e:?}"),
    }
    // A server that never starts is a notice, not an error.
    let mut servers = HashMap::new();
    servers.insert(
        "ghost".to_string(),
        McpServerConfig {
            command: Some("/definitely/not/a/server".into()),
            ..McpServerConfig::default()
        },
    );
    let (clients, tools, notices) = connect_all(&servers, Duration::from_secs(2), true).await;
    assert!(clients.is_empty() && tools.is_empty());
    assert_eq!(notices.len(), 1);
    assert!(
        notices[0].starts_with("mcp server `ghost` skipped:"),
        "{notices:?}"
    );
}

#[test]
fn client_discovery_prefers_config_over_mcp_json_over_claude_json() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        home.path().join(".claude.json"),
        json!({
            "mcpServers": { "a": { "command": "claude-a" }, "c": { "type": "http", "url": "https://c/${MCP_C_PATH:-mcp}" } },
            "projects": { project.path().display().to_string(): { "mcpServers": { "d": { "command": "claude-d" } } } }
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        project.path().join(".mcp.json"),
        json!({ "mcpServers": { "a": { "command": "json-a", "args": ["--token", "${COX_TEST_TOKEN}"] }, "b": { "command": "json-b", "env": { "K": "v" } } } })
            .to_string(),
    )
    .unwrap();
    let mut config = HashMap::new();
    config.insert(
        "b".to_string(),
        McpServerConfig {
            command: Some("config-b".into()),
            ..McpServerConfig::default()
        },
    );
    // SAFETY: this test is the only one in the binary touching this variable.
    unsafe { std::env::set_var("COX_TEST_TOKEN", "sekrit") };
    let found = discover(&config, Some(project.path()), Some(home.path()));
    assert!(found.notices.is_empty(), "{:?}", found.notices);
    assert_eq!(found.servers["a"].command.as_deref(), Some("json-a"));
    assert_eq!(found.servers["a"].args, ["--token", "sekrit"]);
    assert_eq!(found.servers["b"].command.as_deref(), Some("config-b"));
    assert!(found.servers["b"].env.is_empty());
    assert_eq!(found.servers["c"].url.as_deref(), Some("https://c/mcp"));
    assert_eq!(found.servers["d"].command.as_deref(), Some("claude-d"));
    assert_eq!(found.sources["a"], ".mcp.json");
    assert_eq!(found.sources["b"], "config");
    assert_eq!(found.sources["c"], "~/.claude.json");

    let broken = tempfile::tempdir().unwrap();
    std::fs::write(broken.path().join(".mcp.json"), "{").unwrap();
    let found = discover(&HashMap::new(), Some(broken.path()), None);
    assert!(found.servers.is_empty());
    assert_eq!(found.notices.len(), 1);
}
