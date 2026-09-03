//! MCP client (plan.md T7.6): one rmcp session per server, its tools
//! exposed as `mcp__<server>__<tool>` `Tool` impls that the core gates like
//! any other. A server that will not start is a notice and no tools (D14);
//! a call that fails is an error result the model can read, never a crash.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cox_protocol::config::{CHILD_ENV_ALLOWLIST, McpServerConfig};
use cox_protocol::errors::ToolError;
use cox_protocol::traits::{Tool, ToolCx};
use cox_protocol::types::{Concurrency, Risk, ToolOutput, ToolSpec};
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransport;
use rmcp::transport::{IntoTransport, TokioChildProcess};
use rmcp::{RmcpError, ServiceExt};
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("server `{name}` has neither `command` nor `url`")]
    NoTransport { name: String },
    #[error("spawn {command}: {source}")]
    Spawn {
        command: String,
        source: std::io::Error,
    },
    #[error("handshake: {0}")]
    Handshake(Box<RmcpError>),
    #[error("tools/list: {0}")]
    List(rmcp::ServiceError),
}

/// A connected server. Cloning shares the session; `close` ends it.
#[derive(Clone)]
pub struct McpClient {
    name: String,
    service: Arc<RunningService<RoleClient, ()>>,
    timeout: Duration,
}

impl McpClient {
    /// Spawns a stdio server or opens a Streamable HTTP one and handshakes.
    pub async fn connect(
        name: &str,
        cfg: &McpServerConfig,
        timeout: Duration,
    ) -> Result<Self, ClientError> {
        match (&cfg.command, &cfg.url) {
            (Some(command), _) => {
                let mut cmd = tokio::process::Command::new(command);
                cmd.args(&cfg.args).env_clear();
                for key in CHILD_ENV_ALLOWLIST {
                    if let Ok(v) = std::env::var(key) {
                        cmd.env(key, v);
                    }
                }
                cmd.envs(&cfg.env);
                let transport =
                    TokioChildProcess::new(cmd).map_err(|source| ClientError::Spawn {
                        command: command.clone(),
                        source,
                    })?;
                Self::from_transport(name, transport, timeout).await
            }
            (None, Some(url)) => {
                let transport = StreamableHttpClientTransport::from_uri(url.as_str());
                Self::from_transport(name, transport, timeout).await
            }
            (None, None) => Err(ClientError::NoTransport {
                name: name.to_string(),
            }),
        }
    }

    /// Handshakes over any rmcp transport (tests use an in-process duplex).
    pub async fn from_transport<T, E, A>(
        name: &str,
        transport: T,
        timeout: Duration,
    ) -> Result<Self, ClientError>
    where
        T: IntoTransport<RoleClient, E, A>,
        E: std::error::Error + Send + Sync + 'static,
    {
        let service =
            ().serve(transport)
                .await
                .map_err(|e| ClientError::Handshake(Box::new(e.into())))?;
        Ok(Self {
            name: name.to_string(),
            service: Arc::new(service),
            timeout,
        })
    }

    /// Every tool the server lists, namespaced and (by default) deferred.
    pub async fn tools(&self, deferred: bool) -> Result<Vec<Arc<dyn Tool>>, ClientError> {
        let listed = self
            .service
            .list_all_tools()
            .await
            .map_err(ClientError::List)?;
        Ok(listed
            .into_iter()
            .map(|tool| {
                Arc::new(McpTool {
                    client: self.clone(),
                    tool,
                    deferred,
                }) as Arc<dyn Tool>
            })
            .collect())
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Ends the session; a stdio server's process is killed with it.
    pub async fn close(self) {
        if let Ok(service) = Arc::try_unwrap(self.service) {
            let _ = service.cancel().await;
        }
    }
}

/// Connects every server; one that fails is a notice, not an error (step 5).
pub async fn connect_all(
    servers: &HashMap<String, McpServerConfig>,
    timeout: Duration,
    deferred: bool,
) -> (Vec<McpClient>, Vec<Arc<dyn Tool>>, Vec<String>) {
    let mut clients = Vec::new();
    let mut tools = Vec::new();
    let mut notices = Vec::new();
    let mut names: Vec<&String> = servers.keys().collect();
    names.sort();
    for name in names {
        let connect =
            tokio::time::timeout(timeout, McpClient::connect(name, &servers[name], timeout));
        let listed = match connect.await {
            Ok(Ok(client)) => client.tools(deferred).await.map(|t| (client, t)),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                notices.push(format!(
                    "mcp server `{name}` skipped: no handshake within {}s",
                    timeout.as_secs()
                ));
                continue;
            }
        };
        match listed {
            Ok((client, list)) => {
                tools.extend(list);
                clients.push(client);
            }
            Err(e) => notices.push(format!("mcp server `{name}` skipped: {e}")),
        }
    }
    (clients, tools, notices)
}

/// One server tool as the core sees it.
pub struct McpTool {
    client: McpClient,
    tool: rmcp::model::Tool,
    deferred: bool,
}

impl McpTool {
    fn qualified(&self) -> String {
        format!("mcp__{}__{}", self.client.name, self.tool.name)
    }
}

#[async_trait]
impl Tool for McpTool {
    fn spec(&self) -> ToolSpec {
        // Step 4: annotations are hints from an untrusted server, so only
        // `readOnlyHint` lowers the risk; `destructiveHint` raises it and
        // silence means `Write`.
        let ann = self.tool.annotations.as_ref();
        let risk = match (
            ann.and_then(|a| a.read_only_hint),
            ann.and_then(|a| a.destructive_hint),
        ) {
            (Some(true), _) => Risk::ReadOnly,
            (_, Some(true)) => Risk::Destructive,
            _ => Risk::Write,
        };
        ToolSpec {
            name: self.qualified(),
            description: self
                .tool
                .description
                .as_deref()
                .unwrap_or_default()
                .to_string(),
            input_schema: Value::Object((*self.tool.input_schema).clone()),
            deferred: self.deferred,
            risk,
            concurrency: if risk == Risk::ReadOnly {
                Concurrency::Parallel
            } else {
                Concurrency::Exclusive
            },
        }
    }

    fn subject(&self, _input: &Value) -> String {
        self.qualified()
    }

    async fn call(&self, input: Value, cx: &ToolCx) -> Result<ToolOutput, ToolError> {
        let mut params = CallToolRequestParams::new(self.tool.name.clone());
        if let Some(args) = input.as_object() {
            params = params.with_arguments(args.clone());
        }
        let call = self.client.service.call_tool(params);
        let result = tokio::select! {
            _ = cx.cancel.cancelled() => return Err(ToolError::Cancelled),
            r = tokio::time::timeout(self.client.timeout, call) => match r {
                Ok(Ok(result)) => result,
                Ok(Err(e)) => return Ok(ToolOutput {
                    text: format!("mcp server `{}`: {e}", self.client.name),
                    is_error: true,
                    diff: None,
                    structured: None,
                }),
                Err(_) => return Err(ToolError::Timeout),
            },
        };
        Ok(output_of(result))
    }
}

fn output_of(result: CallToolResult) -> ToolOutput {
    let text = result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    ToolOutput {
        text,
        is_error: result.is_error.unwrap_or(false),
        diff: None,
        structured: result.structured_content,
    }
}
