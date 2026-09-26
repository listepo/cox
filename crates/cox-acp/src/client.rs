//! cox as an ACP **client** (T35.3, EA§4): drives an external agent such as
//! Cursor's `agent acp` over the spawned process's stdio and answers the
//! requests it sends back. Beside `server.rs`, not inside it, because the
//! roles are reversed — there cox is the agent an editor drives, here cox
//! drives someone else's agent — while the same `agent-client-protocol`
//! crate serves both, so no new dependency.
//!
//! Everything the agent sends is untrusted, and no request here gets a
//! guard of its own: `session/request_permission` is decided by
//! `cox_permission::Engine` (via `cox_core::permission`), `fs/*` paths go
//! through `cox_sandbox::path::confine` plus the sandbox policy already
//! governing the process, and `terminal/*` is refused with the reason named.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    ClientCapabilities, CreateTerminalRequest, ErrorCode, InitializeRequest, PermissionOption,
    PermissionOptionKind, ReadTextFileRequest, ReadTextFileResponse, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, SelectedPermissionOutcome,
    SessionNotification, SessionUpdate, ToolKind, WriteTextFileRequest, WriteTextFileResponse,
};
use agent_client_protocol::{Agent, Client, ConnectTo, ConnectionTo, Error};
use cox_core::permission::{Engine, Outcome};
use cox_protocol::CallId;
use cox_protocol::types::{
    ApprovalPolicy, Decision, PermissionMode, Risk, SandboxMode, SandboxPolicy, ToolCall, Why,
};
use cox_sandbox::path::confine;

/// Where an `Ask` verdict reaches the user. The caller implements it over
/// the session's existing `ApprovalRequired` relay (labelled with the
/// subagent's `Source`, T34.3), so there is no second approval path.
#[async_trait::async_trait]
pub trait Approver: Send + Sync + 'static {
    /// The user's answer for `call`, which the engine escalated for `why`.
    async fn approve(&self, call: ToolCall, why: Why) -> Decision;
}

/// What the client decides with on the external agent's behalf.
pub struct ClientHost {
    /// Workspace roots the process was given; every `fs/*` path is confined to them.
    pub roots: Vec<PathBuf>,
    /// The session working directory relative paths resolve against.
    pub cwd: PathBuf,
    /// The sandbox policy the spawned process runs under (T35.2), or `None`
    /// when it was granted no sandbox (T33.42's opt-out).
    pub sandbox: Option<SandboxPolicy>,
    /// The single permission guard.
    pub engine: Arc<Engine>,
    /// The session's permission mode.
    pub mode: PermissionMode,
    /// The session's approval policy.
    pub approval: ApprovalPolicy,
    /// Session grants the engine already knows; `AllowForSession` adds more.
    pub grants: Vec<(String, String)>,
    /// Answers `Outcome::Ask`.
    pub approver: Arc<dyn Approver>,
    /// Where the agent's `session/update` notifications go (T35.13): the
    /// driver turns them into cox events. `None` drops them.
    pub updates: Option<tokio::sync::mpsc::UnboundedSender<SessionUpdate>>,
}

/// `ClientHost` plus the grants that grow while the connection lives.
struct Shared {
    host: ClientHost,
    grants: Mutex<Vec<(String, String)>>,
}

/// The `initialize` request that matches what `connect` serves: text-file
/// reads and writes, and no client terminal.
pub fn initialize_request() -> InitializeRequest {
    let mut caps = ClientCapabilities::default();
    caps.fs.read_text_file = true;
    caps.fs.write_text_file = true;
    caps.terminal = false;
    InitializeRequest::new(ProtocolVersion::V1).client_capabilities(caps)
}

/// Runs the client over `transport` until `main_fn` returns. The spawned
/// process's stdio is `ByteStreams::new(stdin, stdout)` (any
/// `futures::AsyncWrite`/`AsyncRead` pair); `main_fn` sends `initialize`
/// (use `initialize_request`), `session/new` and prompts through the
/// `ConnectionTo<Agent>` it is given.
pub async fn connect<R>(
    transport: impl ConnectTo<Client> + 'static,
    host: ClientHost,
    main_fn: impl AsyncFnOnce(ConnectionTo<Agent>) -> Result<R, Error>,
) -> Result<R, Error> {
    let grants = Mutex::new(host.grants.clone());
    let shared = Arc::new(Shared { host, grants });
    let (perm, read, write) = (shared.clone(), shared.clone(), shared.clone());
    let sandboxed = shared.host.sandbox.is_some();
    let updates = shared.host.updates.clone();
    Client
        .builder()
        .name("cox")
        .on_receive_notification(
            async move |note: SessionNotification, _cx| {
                // A driver that stopped listening loses the update, never the turn.
                if let Some(tx) = &updates {
                    let _ = tx.send(note.update);
                }
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |req: RequestPermissionRequest, responder, cx: ConnectionTo<Agent>| {
                // An `Ask` waits on the user; off the dispatch loop so the
                // agent's `session/update`s keep flowing meanwhile.
                let shared = perm.clone();
                cx.spawn(async move { responder.respond(permission(&shared, req).await) })
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: ReadTextFileRequest, responder, _cx| {
                responder.respond_with_result(read_text_file(&read.host, req).await)
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: WriteTextFileRequest, responder, _cx| {
                responder.respond_with_result(write_text_file(&write.host, req).await)
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_req: CreateTerminalRequest, responder, _cx| {
                responder.respond_with_error(refused(terminal_refusal(sandboxed)))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(transport, main_fn)
        .await
}

/// Engine verdict → the option the agent offered that matches it.
async fn permission(shared: &Shared, req: RequestPermissionRequest) -> RequestPermissionResponse {
    let host = &shared.host;
    let call = tool_call_for(host, &req);
    let sandbox = host
        .sandbox
        .as_ref()
        .map_or(SandboxMode::DangerFullAccess, |p| p.mode);
    let grants = shared.grants.lock().map(|g| g.clone()).unwrap_or_default();
    let decision = match host
        .engine
        .decide(&call, host.mode, host.approval, sandbox, &grants)
    {
        Outcome::Allow { .. } => Decision::Allow,
        Outcome::Deny { reason, .. } => Decision::Deny { reason },
        Outcome::Ask(why) => host.approver.approve(call.clone(), why).await,
    };
    if matches!(decision, Decision::AllowForSession)
        && let Ok(mut g) = shared.grants.lock()
    {
        g.push((call.name, call.subject));
    }
    RequestPermissionResponse::new(pick(&req.options, &decision))
}

/// Never grants the agent more than the engine allowed: a one-off `Allow`
/// takes only an allow-once option, and an edited input cannot be handed
/// back to the agent, so it counts as a refusal. No matching option means
/// `Cancelled`.
fn pick(options: &[PermissionOption], decision: &Decision) -> RequestPermissionOutcome {
    use PermissionOptionKind as K;
    let wanted: &[K] = match decision {
        Decision::Allow => &[K::AllowOnce],
        Decision::AllowForSession => &[K::AllowAlways, K::AllowOnce],
        Decision::Deny { .. } | Decision::Edit { .. } => &[K::RejectOnce, K::RejectAlways],
    };
    wanted
        .iter()
        .find_map(|kind| options.iter().find(|o| o.kind == *kind))
        .map_or(RequestPermissionOutcome::Cancelled, |o| {
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(o.option_id.clone()))
        })
}

/// The agent's tool call in the shape the engine judges. Its kind picks the
/// cox tool whose rules apply; an unknown kind cannot be shown read-only, so
/// it is judged as `bash`. The subject is sanitized because the approval
/// prompt shows it.
fn tool_call_for(host: &ClientHost, req: &RequestPermissionRequest) -> ToolCall {
    let f = &req.tool_call.fields;
    let (name, risk) = match f.kind {
        Some(ToolKind::Read) => ("read", Risk::ReadOnly),
        Some(ToolKind::Search) => ("grep", Risk::ReadOnly),
        Some(ToolKind::Fetch) => ("web_fetch", Risk::ReadOnly),
        Some(ToolKind::Edit | ToolKind::Move) => ("edit", Risk::Write),
        Some(ToolKind::Delete) => ("edit", Risk::Destructive),
        _ => ("bash", Risk::Exec),
    };
    let path = f
        .locations
        .as_ref()
        .and_then(|l| l.first())
        .map(|l| host.cwd.join(&l.path).to_string_lossy().into_owned());
    let command = f
        .raw_input
        .as_ref()
        .and_then(|v| v.get("command"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let subject = match risk {
        Risk::Exec => command.or(path),
        _ => path.or(command),
    }
    .or_else(|| f.title.clone())
    .unwrap_or_default();
    ToolCall {
        id: CallId::new(),
        name: name.into(),
        input: f.raw_input.clone().unwrap_or_default(),
        risk,
        subject: cox_sanitize::sanitize(&subject),
    }
}

async fn read_text_file(
    host: &ClientHost,
    req: ReadTextFileRequest,
) -> Result<ReadTextFileResponse, Error> {
    let path = confined(host, &req.path)?;
    let text = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| refused(format!("fs/read_text_file {}: {e}", path.display())))?;
    let text = if req.line.is_none() && req.limit.is_none() {
        text
    } else {
        // `line` is 1-based per the ACP schema.
        let skip = req.line.map_or(0, |l| l.saturating_sub(1) as usize);
        let take = req.limit.map_or(usize::MAX, |l| l as usize);
        text.lines()
            .skip(skip)
            .take(take)
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(ReadTextFileResponse::new(text))
}

/// Writes only what the process's own sandbox would let it write: nothing
/// under a read-only sandbox, nothing under `readonly_in_workspace`.
async fn write_text_file(
    host: &ClientHost,
    req: WriteTextFileRequest,
) -> Result<WriteTextFileResponse, Error> {
    let path = confined(host, &req.path)?;
    if let Some(policy) = &host.sandbox {
        if policy.mode == SandboxMode::ReadOnly {
            return Err(refused(
                "fs/write_text_file refused: the external agent's sandbox is read-only",
            ));
        }
        let locked = host.roots.iter().flat_map(|root| {
            let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.clone());
            policy
                .readonly_in_workspace
                .iter()
                .map(move |sub| root.join(sub))
        });
        for locked in locked {
            if path.starts_with(&locked) {
                return Err(refused(format!(
                    "fs/write_text_file refused: {} stays read-only inside the workspace",
                    locked.display()
                )));
            }
        }
    }
    tokio::fs::write(&path, req.content)
        .await
        .map_err(|e| refused(format!("fs/write_text_file {}: {e}", path.display())))?;
    Ok(WriteTextFileResponse::new())
}

fn confined(host: &ClientHost, path: &Path) -> Result<PathBuf, Error> {
    let raw = path
        .to_str()
        .ok_or_else(|| refused("fs request refused: the path is not UTF-8"))?;
    confine(&host.roots, &host.cwd, raw).map_err(|e| refused(format!("fs request refused: {e}")))
}

/// cox never runs a command for the agent outside the sandbox, and with one
/// it still offers no client terminal (`initialize_request` says so): the
/// agent's own shell already runs under that same sandbox.
fn terminal_refusal(sandboxed: bool) -> &'static str {
    if sandboxed {
        "terminal/create refused: cox offers external agents no client terminal; \
         the agent's own shell already runs under its sandbox"
    } else {
        "terminal/create refused: the external agent runs without a sandbox grant, \
         and cox never runs a command outside the sandbox"
    }
}

fn refused(reason: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidParams.into(), reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::Channel;
    use agent_client_protocol::schema::v1::{
        PermissionOptionId, SessionId, ToolCallId, ToolCallUpdate, ToolCallUpdateFields,
    };
    use cox_protocol::config::PermissionsConfig;
    use serde_json::json;
    use tokio::sync::oneshot;

    /// Records every escalation and answers with a fixed decision.
    struct Recorder(Mutex<Vec<ToolCall>>, Decision);

    #[async_trait::async_trait]
    impl Approver for Recorder {
        async fn approve(&self, call: ToolCall, _why: Why) -> Decision {
            if let Ok(mut seen) = self.0.lock() {
                seen.push(call);
            }
            self.1.clone()
        }
    }

    fn host(root: &Path, sandbox: Option<SandboxPolicy>, approver: Arc<Recorder>) -> ClientHost {
        let cfg = PermissionsConfig {
            allow: vec!["Bash(git status)".into()],
            deny: vec!["Bash(rm:*)".into()],
            ask: vec![],
            ..PermissionsConfig::default()
        };
        ClientHost {
            roots: vec![root.to_path_buf()],
            cwd: root.to_path_buf(),
            sandbox,
            engine: Arc::new(Engine::compile(&cfg, None, root).expect("rules compile")),
            mode: PermissionMode::Default,
            approval: ApprovalPolicy::OnRequest,
            grants: vec![],
            approver,
            updates: None,
        }
    }

    fn policy(mode: SandboxMode) -> SandboxPolicy {
        SandboxPolicy {
            mode,
            network: false,
            writable: vec![],
            readonly_in_workspace: vec![PathBuf::from(".git")],
            linux_backend: Default::default(),
        }
    }

    /// Plays the external agent: `script` sends requests over a duplex
    /// channel to a live `connect`, which stays up until the script ends.
    async fn as_agent<T: Send + 'static>(
        host: ClientHost,
        script: impl AsyncFnOnce(ConnectionTo<Client>) -> Result<T, Error>,
    ) -> T {
        let (to_client, to_agent) = Channel::duplex();
        let (done, wait) = oneshot::channel::<()>();
        let client = tokio::spawn(connect(to_client, host, async move |_cx| {
            let _ = wait.await;
            Ok(())
        }));
        let out = Agent
            .builder()
            .connect_with(to_agent, async move |cx| {
                let out = script(cx).await;
                let _ = done.send(());
                out
            })
            .await
            .expect("agent side");
        client.await.expect("join").expect("client side");
        out
    }

    fn ask(command: &str) -> RequestPermissionRequest {
        let fields = ToolCallUpdateFields::new()
            .kind(Some(ToolKind::Execute))
            .title(Some(format!("run {command}")))
            .raw_input(Some(json!({ "command": command })));
        let options = vec![
            PermissionOption::new("once", "Allow", PermissionOptionKind::AllowOnce),
            PermissionOption::new("always", "Always", PermissionOptionKind::AllowAlways),
            PermissionOption::new("no", "Reject", PermissionOptionKind::RejectOnce),
        ];
        RequestPermissionRequest::new(
            SessionId::new("s"),
            ToolCallUpdate::new(ToolCallId::new("t"), fields),
            options,
        )
    }

    fn selected(outcome: &RequestPermissionOutcome) -> Option<PermissionOptionId> {
        match outcome {
            RequestPermissionOutcome::Selected(s) => Some(s.option_id.clone()),
            _ => None,
        }
    }

    #[tokio::test]
    async fn acp_client_forwards_session_updates_to_the_driver() {
        use agent_client_protocol::schema::v1::ContentChunk;

        let dir = tempfile::tempdir().expect("tempdir");
        let approver = Arc::new(Recorder(Mutex::new(vec![]), Decision::Allow));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut host = host(dir.path(), None, approver);
        host.updates = Some(tx);
        std::fs::write(dir.path().join("a.txt"), "a").expect("file");
        let path = dir.path().join("a.txt");
        as_agent(host, async |cx| {
            cx.send_notification(SessionNotification::new(
                SessionId::new("s"),
                SessionUpdate::AgentMessageChunk(ContentChunk::new("hi".into())),
            ))?;
            // A round trip after it, the way the prompt response follows a
            // turn's updates: the client has dispatched the update by then.
            let read = ReadTextFileRequest::new(SessionId::new("s"), path);
            cx.send_request(read).block_task().await.map(|_| ())
        })
        .await;
        let got = rx.recv().await.expect("one update");
        assert!(
            matches!(&got, SessionUpdate::AgentMessageChunk(c) if c.content == "hi".into()),
            "{got:?}"
        );
    }

    #[tokio::test]
    async fn acp_client_relays_request_permission_through_the_engine() {
        let dir = tempfile::tempdir().expect("tempdir");
        let approver = Arc::new(Recorder(
            Mutex::new(vec![]),
            Decision::Deny {
                reason: "no".into(),
            },
        ));
        let host = host(dir.path(), None, approver.clone());
        let outcomes = as_agent(host, async |cx| {
            let mut out = vec![];
            for command in ["git status", "rm -rf src", "cargo \u{1b}[2Jpublish"] {
                let r = cx.send_request(ask(command)).block_task().await?;
                out.push(selected(&r.outcome));
            }
            Ok(out)
        })
        .await;
        // Allow rule → allow-once; deny rule → reject without asking; no
        // rule → the engine's `Ask` reaches the approver, whose `Deny` rejects.
        let ids: Vec<_> = outcomes
            .iter()
            .map(|o| o.as_ref().map(|id| id.0.to_string()))
            .collect();
        let want = |s: &str| Some(s.to_string());
        assert_eq!(ids, [want("once"), want("no"), want("no")]);
        let seen = approver.0.lock().expect("lock");
        assert_eq!(seen.len(), 1, "only the unmatched call is escalated");
        assert_eq!(seen[0].name, "bash");
        assert_eq!(seen[0].subject, "cargo publish", "subject is sanitized");
    }

    #[tokio::test]
    async fn acp_client_fs_request_is_confined_to_the_workspace() {
        let ws = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("tempdir");
        let secret = outside.path().join("secret.txt");
        std::fs::write(&secret, "top secret").expect("seed");
        std::fs::create_dir(ws.path().join(".git")).expect("mkdir");
        std::fs::write(ws.path().join("a.txt"), "one\ntwo\nthree").expect("seed");
        let approver = Arc::new(Recorder(Mutex::new(vec![]), Decision::Allow));
        let sandbox = Some(policy(SandboxMode::WorkspaceWrite));
        let host = host(ws.path(), sandbox, approver);
        let root = ws.path().to_path_buf();
        let sibling = PathBuf::from("..")
            .join(outside.path().file_name().expect("name"))
            .join("secret.txt");
        let (inside, escape, write_out, write_git, write_ok) = as_agent(host, async |cx| {
            let s = SessionId::new("s");
            let read = |p: PathBuf| ReadTextFileRequest::new(s.clone(), p);
            let write = |p: PathBuf| WriteTextFileRequest::new(s.clone(), p, "x");
            let inside = cx
                .send_request(read(root.join("a.txt")).line(2).limit(1))
                .block_task()
                .await;
            let escape = cx.send_request(read(sibling.clone())).block_task().await;
            let write_out = cx.send_request(write(secret.clone())).block_task().await;
            let write_git = cx
                .send_request(write(root.join(".git/config")))
                .block_task()
                .await;
            let write_ok = cx
                .send_request(write(root.join("b.txt")))
                .block_task()
                .await;
            Ok((inside, escape, write_out, write_git, write_ok))
        })
        .await;
        assert_eq!(inside.expect("read inside").content, "two");
        let escape = escape.expect_err("read outside is refused");
        assert!(escape.message.contains("refused"), "{}", escape.message);
        assert!(write_out.is_err(), "write outside is refused");
        let git = write_git.expect_err("write under .git is refused");
        assert!(git.message.contains("read-only"), "{}", git.message);
        write_ok.expect("write inside");
        assert_eq!(
            std::fs::read_to_string(&secret).expect("read"),
            "top secret"
        );
        assert_eq!(
            std::fs::read_to_string(ws.path().join("b.txt")).expect("read"),
            "x"
        );
    }

    #[tokio::test]
    async fn acp_client_terminal_request_without_sandbox_grant_is_refused() {
        let dir = tempfile::tempdir().expect("tempdir");
        let approver = Arc::new(Recorder(Mutex::new(vec![]), Decision::Allow));
        let host = host(dir.path(), None, approver);
        let err = as_agent(host, async |cx| {
            let req =
                CreateTerminalRequest::new(SessionId::new("s"), "touch").args(vec!["pwned".into()]);
            Ok(cx.send_request(req).block_task().await)
        })
        .await
        .expect_err("terminal is refused");
        assert!(
            err.message.contains("without a sandbox grant"),
            "{}",
            err.message
        );
        assert!(!dir.path().join("pwned").exists());
        assert!(!initialize_request().client_capabilities.terminal);
    }
}
