//! The ACP server over cox sessions (T11.1): `initialize`, `authenticate`,
//! `session/new`, `session/load`, `session/prompt` and `session/cancel`
//! handlers over one `Event` stream per session. Request handlers never
//! block the dispatch loop: `session/prompt` spawns its turn driver and
//! answers late through the moved `Responder`, so `session/cancel` and
//! permission replies keep flowing mid-turn.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    AuthenticateRequest, AuthenticateResponse, ClientCapabilities, ContentBlock, InitializeRequest,
    InitializeResponse, LoadSessionRequest, LoadSessionResponse, NewSessionRequest,
    NewSessionResponse, PermissionOption, PermissionOptionKind, PromptRequest, PromptResponse,
    RequestPermissionOutcome, RequestPermissionRequest, SessionId, SessionNotification,
};
use agent_client_protocol::{Agent, Client, ConnectionTo};
use cox_protocol::types::{Decision, Event, Submission, ToolCall, Why};
use tokio::sync::{broadcast, mpsc};

use crate::client_tools::ClientLink;
use crate::map::{self, CallTable};

/// Builds live cox sessions for ACP sessions. Implemented in the binary
/// (which owns config, providers, tools and the store); the server only
/// drives the returned `Session` through `Submission`s and `Event`s.
pub trait SessionFactory: Send + Sync + 'static {
    /// Creates the cox session for an ACP `session/new`.
    fn create(&self, req: FactoryRequest) -> anyhow::Result<cox_core::Session>;
}

/// What the factory needs: where the session lives, which client tools to
/// use, and the link those tools call back through.
pub struct FactoryRequest {
    /// Session working directory from `session/new`.
    pub cwd: PathBuf,
    /// Additional workspace roots from the client.
    pub roots: Vec<PathBuf>,
    /// The client offers `fs/read_text_file` + `fs/write_text_file`.
    pub client_fs: bool,
    /// The client offers `terminal/*`.
    pub client_terminal: bool,
    /// Client link for this ACP session's proxy tools.
    pub link: ClientLink,
}

/// One live ACP session: the cox session, its event broadcast for prompt
/// drivers, and a mutex serialising prompts.
#[derive(Clone)]
struct LiveSession {
    cox: cox_core::Session,
    bcast: broadcast::Sender<Event>,
    prompt_lock: Arc<tokio::sync::Mutex<()>>,
    cwd: PathBuf,
}

/// Shared server state behind every handler.
#[derive(Clone)]
pub struct ServerState {
    factory: Arc<dyn SessionFactory>,
    sessions: Arc<Mutex<HashMap<String, LiveSession>>>,
    client_caps: Arc<Mutex<ClientCapabilities>>,
}

impl ServerState {
    /// New server state over `factory`.
    pub fn new(factory: Arc<dyn SessionFactory>) -> Self {
        Self {
            factory,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            client_caps: Arc::new(Mutex::new(ClientCapabilities::default())),
        }
    }

    fn caps(&self) -> ClientCapabilities {
        self.client_caps
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default()
    }
}

/// Runs the ACP server on stdio until the client goes away.
pub async fn serve_stdio(factory: Arc<dyn SessionFactory>) -> anyhow::Result<()> {
    serve_transport(factory, agent_client_protocol::Stdio::new()).await
}

/// Runs the ACP server over an in-process channel pair (tests).
pub async fn serve_channel(
    factory: Arc<dyn SessionFactory>,
    channel: agent_client_protocol::Channel,
) -> anyhow::Result<()> {
    serve_transport(factory, channel).await
}

async fn serve_transport(
    factory: Arc<dyn SessionFactory>,
    transport: impl agent_client_protocol::ConnectTo<Agent>,
) -> anyhow::Result<()> {
    let state = ServerState::new(factory);
    let s = state.clone();
    Agent
        .builder()
        .name("cox")
        .on_receive_request(
            async move |req: InitializeRequest, responder, _conn| {
                handle_initialize(&s, req, responder)
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let s = state.clone();
                async move |req: AuthenticateRequest, responder, _conn| {
                    handle_authenticate(&s, req, responder)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let s = state.clone();
                async move |req: NewSessionRequest, responder, conn| {
                    handle_new_session(&s, req, responder, conn).await
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let s = state.clone();
                async move |req: LoadSessionRequest, responder, _conn| {
                    handle_load_session(&s, req, responder)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let s = state.clone();
                async move |req: PromptRequest, responder, conn| {
                    handle_prompt(&s, req, responder, conn).await
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            {
                let s = state.clone();
                async move |notif: agent_client_protocol::schema::v1::CancelNotification, _conn| {
                    handle_cancel(&s, notif)
                }
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_to(transport)
        .await
        .map_err(|e| anyhow::anyhow!("acp transport closed: {e}"))
}

fn handle_initialize(
    state: &ServerState,
    req: InitializeRequest,
    responder: agent_client_protocol::Responder<InitializeResponse>,
) -> Result<(), agent_client_protocol::Error> {
    if let Ok(mut caps) = state.client_caps.lock() {
        *caps = req.client_capabilities.clone();
    }
    // v1-only server: this build has no V2 surface, so answer V1.
    let version = ProtocolVersion::V1;
    let mut caps = agent_client_protocol::schema::v1::AgentCapabilities::new();
    caps.load_session = true;
    responder.respond(InitializeResponse::new(version).agent_capabilities(caps))
}

fn handle_authenticate(
    _state: &ServerState,
    _req: AuthenticateRequest,
    responder: agent_client_protocol::Responder<AuthenticateResponse>,
) -> Result<(), agent_client_protocol::Error> {
    // No authentication: cox trusts the local client (same policy as the
    // TUI and headless surfaces on this machine).
    responder.respond(AuthenticateResponse::new())
}

async fn handle_new_session(
    state: &ServerState,
    req: NewSessionRequest,
    responder: agent_client_protocol::Responder<NewSessionResponse>,
    conn: ConnectionTo<Client>,
) -> Result<(), agent_client_protocol::Error> {
    let acp_id = SessionId::new(cox_protocol::SessionId::new().to_string());
    let caps = state.caps();
    let link = ClientLink {
        conn: conn.clone(),
        session: acp_id.clone(),
        cwd: req.cwd.clone(),
    };
    let factory_req = FactoryRequest {
        cwd: req.cwd.clone(),
        roots: req.additional_directories.clone(),
        client_fs: caps.fs.read_text_file && caps.fs.write_text_file,
        client_terminal: caps.terminal,
        link: link.clone(),
    };
    let cox = match state.factory.create(factory_req) {
        Ok(session) => session,
        Err(_) => {
            return responder.respond_with_error(agent_client_protocol::Error::internal_error());
        }
    };
    let rx = cox
        .events()
        .ok_or_else(agent_client_protocol::Error::internal_error)?;
    let (bcast, _) = broadcast::channel(256);
    let live = LiveSession {
        cox: cox.clone(),
        bcast: bcast.clone(),
        prompt_lock: Arc::new(tokio::sync::Mutex::new(())),
        cwd: req.cwd.clone(),
    };
    state
        .sessions
        .lock()
        .map_err(|_| agent_client_protocol::Error::internal_error())?
        .insert(acp_id.to_string(), live);
    tokio::spawn(forward_events(rx, bcast, link, req.cwd));
    responder.respond(NewSessionResponse::new(acp_id))
}

fn handle_load_session(
    state: &ServerState,
    req: LoadSessionRequest,
    responder: agent_client_protocol::Responder<LoadSessionResponse>,
) -> Result<(), agent_client_protocol::Error> {
    // Resume within the server's lifetime: the live session keeps its full
    // history. A restart drops sessions (core has no rehydration API), which
    // is an explicit error rather than an empty session.
    let sessions = state
        .sessions
        .lock()
        .map_err(|_| agent_client_protocol::Error::internal_error())?;
    match sessions.get(&req.session_id.to_string()) {
        Some(live) if live.cwd == req.cwd => responder.respond(LoadSessionResponse::new()),
        Some(_) => responder.respond_with_error(agent_client_protocol::Error::internal_error()),
        None => responder.respond_with_error(agent_client_protocol::Error::internal_error()),
    }
}

/// Forwards one session's events as `session/update` notifications and onto
/// the prompt drivers' broadcast.
async fn forward_events(
    mut rx: mpsc::Receiver<Event>,
    bcast: broadcast::Sender<Event>,
    link: ClientLink,
    cwd: PathBuf,
) {
    let mut calls = CallTable::new();
    while let Some(ev) = rx.recv().await {
        let _ = bcast.send(ev.clone());
        for update in map::updates_for(&mut calls, &ev, &cwd) {
            let notif = SessionNotification::new(link.session.clone(), update);
            if link.conn.send_notification(notif).is_err() {
                return;
            }
        }
    }
}

async fn handle_prompt(
    state: &ServerState,
    req: PromptRequest,
    responder: agent_client_protocol::Responder<PromptResponse>,
    conn: ConnectionTo<Client>,
) -> Result<(), agent_client_protocol::Error> {
    let live = {
        let sessions = state
            .sessions
            .lock()
            .map_err(|_| agent_client_protocol::Error::internal_error())?;
        let Some(live) = sessions.get(&req.session_id.to_string()).cloned() else {
            return responder.respond_with_error(agent_client_protocol::Error::internal_error());
        };
        live
    };
    // The turn runs outside the dispatch loop so `session/cancel` and
    // permission replies keep flowing; the moved responder answers late.
    let link_session = req.session_id.clone();
    let s = state.clone();
    let conn2 = conn.clone();
    conn.spawn(async move {
        let response = drive_prompt(&s, &live, &link_session, conn2, req).await;
        responder.respond_with_result(response)
    })
    .map_err(|_| agent_client_protocol::Error::internal_error())?;
    Ok(())
}

/// Prompt text out of ACP content blocks; only text survives.
fn prompt_text(prompt: &[ContentBlock]) -> String {
    prompt
        .iter()
        .map(|block| match block {
            ContentBlock::Text(text) => text.text.clone(),
            _ => "[unsupported content block]".to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Submits the turn and drives it to `TurnDone`, answering approvals.
async fn drive_prompt(
    _state: &ServerState,
    live: &LiveSession,
    acp_id: &SessionId,
    conn: ConnectionTo<Client>,
    req: PromptRequest,
) -> Result<PromptResponse, agent_client_protocol::Error> {
    let _guard = live.prompt_lock.lock().await;
    let mut bcast = live.bcast.subscribe();
    let text = prompt_text(&req.prompt);
    let turn = tokio::spawn({
        let live = live.clone();
        async move {
            live.cox
                .submit(Submission::UserTurn {
                    text,
                    attachments: Vec::new(),
                    confirm_think: false,
                })
                .await
        }
    });
    let stop = loop {
        match bcast.recv().await {
            Ok(Event::TurnDone { stop, .. }) => break stop,
            Ok(Event::ApprovalRequired { call, why }) => {
                let decision = ask_permission(&conn, acp_id, &call, &why).await;
                let _ = live
                    .cox
                    .submit(Submission::Approve {
                        call_id: call.id,
                        decision,
                    })
                    .await;
            }
            Ok(_) => {}
            Err(_) => {
                let _ = turn.await;
                return Err(agent_client_protocol::Error::internal_error());
            }
        }
    };
    let _ = turn.await;
    if let Some(detail) = map::stop_detail(&stop) {
        let _ = conn.send_notification(SessionNotification::new(
            acp_id.clone(),
            agent_client_protocol::schema::v1::SessionUpdate::AgentMessageChunk(
                agent_client_protocol::schema::v1::ContentChunk::new(ContentBlock::Text(
                    agent_client_protocol::schema::v1::TextContent::new(detail),
                )),
            ),
        ));
    }
    Ok(PromptResponse::new(map::map_stop(&stop)))
}

/// `ApprovalRequired` → `session/request_permission` with allow,
/// allow-always and reject options.
async fn ask_permission(
    conn: &ConnectionTo<Client>,
    acp_id: &SessionId,
    call: &ToolCall,
    why: &Why,
) -> Decision {
    let _ = why;
    let fields = agent_client_protocol::schema::v1::ToolCallUpdateFields::new()
        .title(Some(format!("{} {}", call.name, call.subject)))
        .status(Some(
            agent_client_protocol::schema::v1::ToolCallStatus::Pending,
        ));
    let update = agent_client_protocol::schema::v1::ToolCallUpdate::new(
        agent_client_protocol::schema::v1::ToolCallId::new(call.id.to_string()),
        fields,
    );
    let options = vec![
        PermissionOption::new("allow", "Allow", PermissionOptionKind::AllowOnce),
        PermissionOption::new(
            "allow-always",
            "Allow always",
            PermissionOptionKind::AllowAlways,
        ),
        PermissionOption::new("reject", "Reject", PermissionOptionKind::RejectOnce),
    ];
    let req = RequestPermissionRequest::new(acp_id.clone(), update, options);
    let outcome = conn.send_request(req).block_task().await.map(|r| r.outcome);
    match outcome {
        Ok(RequestPermissionOutcome::Selected(selected)) => match selected.option_id.0.as_ref() {
            "allow" => Decision::Allow,
            "allow-always" => Decision::AllowForSession,
            _ => Decision::Deny {
                reason: "rejected by client".to_string(),
            },
        },
        _ => Decision::Deny {
            reason: "permission request failed".to_string(),
        },
    }
}

fn handle_cancel(
    state: &ServerState,
    notif: agent_client_protocol::schema::v1::CancelNotification,
) -> Result<(), agent_client_protocol::Error> {
    let sessions = state
        .sessions
        .lock()
        .map_err(|_| agent_client_protocol::Error::internal_error())?;
    if let Some(live) = sessions.get(&notif.session_id.to_string()) {
        live.cox.interrupt();
    }
    Ok(())
}
