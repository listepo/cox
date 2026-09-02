//! One provider call and its tool batch. Kept separate from `Session` so
//! the loop's `step` stays a state transition, not a grab-bag of I/O.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use cox_protocol::errors::CoreError;
use cox_protocol::ids::{CallId, ItemId};
use cox_protocol::traits::{Tool, ToolCx};
use cox_protocol::types::{
    Concurrency, Content, Event, Message, ModelId, Request, Risk, Role, SandboxPolicy, SystemBlock,
    Thinking, Tier, ToolCall, ToolOutput, ToolResult, Usage,
};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::session::Session;

/// Builds the provider-neutral request for this call (T2.3 will pin cache
/// order; here the history is sent as-is so the loop is testable).
pub(crate) fn assemble(
    history: &[Message],
    tools: &[Arc<dyn Tool>],
    model: ModelId,
    effort: cox_protocol::types::Effort,
    max_tokens: u32,
    thinking: Thinking,
) -> Request {
    Request {
        tier: Tier::Code,
        job: cox_protocol::types::Job::Main,
        model,
        system: vec![SystemBlock {
            text: String::new(),
            cache: true,
        }],
        tools: tools.iter().map(|t| t.spec()).collect(),
        messages: history.to_vec(),
        effort,
        max_tokens,
        thinking,
        cache_breakpoints: vec![],
        stop_sequences: vec![],
    }
}

#[derive(Default)]
pub(crate) struct Streamed {
    pub text: String,
    pub calls: Vec<(CallId, String, Value)>,
    pub usage: Option<Usage>,
}

struct Acc {
    id: CallId,
    name: String,
    input: String,
}

/// Forwards `ProviderEvent`s as `Event`s and collects tool-use blocks.
pub(crate) async fn consume_provider(
    session: &Session,
    rx: &mut mpsc::Receiver<cox_protocol::types::ProviderEvent>,
    assistant_item: ItemId,
) -> Result<Streamed, CoreError> {
    use cox_protocol::types::ProviderEvent as P;
    let mut out = Streamed::default();
    let mut current: Option<Acc> = None;
    while let Some(ev) = rx.recv().await {
        match ev {
            P::MessageStart { .. } => {}
            P::TextDelta { text } => {
                out.text.push_str(&text);
                session
                    .emit(Event::TextDelta {
                        item: assistant_item,
                        text,
                    })
                    .await?;
            }
            P::ThinkingDelta { text } => {
                session
                    .emit(Event::ThinkingDelta {
                        item: assistant_item,
                        text,
                    })
                    .await?;
            }
            P::ToolUseStart { id, name } => {
                current = Some(Acc {
                    id,
                    name,
                    input: String::new(),
                });
            }
            P::ToolUseInputDelta { text } => {
                if let Some(acc) = current.as_mut() {
                    acc.input.push_str(&text);
                }
            }
            P::ToolUseEnd => {
                if let Some(acc) = current.take() {
                    let input = serde_json::from_str(&acc.input).unwrap_or(Value::Null);
                    out.calls.push((acc.id, acc.name, input));
                }
            }
            P::Stop { .. } => {}
            P::Usage { usage } => out.usage = Some(usage),
            P::Retrying { .. } => {}
            P::Error { error } => {
                return Err(CoreError::Provider { error });
            }
        }
    }
    Ok(out)
}

/// Runs one batch of tool calls; results are returned in emission order.
pub(crate) async fn run_tools(
    session: &Session,
    calls: Vec<(CallId, String, Value)>,
) -> Result<Vec<(CallId, ToolResult)>, CoreError> {
    let tools: HashMap<String, Arc<dyn Tool>> = session
        .tools
        .iter()
        .map(|t| (t.spec().name, t.clone()))
        .collect();
    let order: Vec<CallId> = calls.iter().map(|(id, _, _)| *id).collect();
    for (id, name, input) in &calls {
        let spec = tools.get(name).map(|t| t.spec());
        let subject = tools
            .get(name)
            .map(|t| t.subject(input))
            .unwrap_or_default();
        let call = ToolCall {
            id: *id,
            name: name.clone(),
            input: input.clone(),
            risk: spec.as_ref().map(|s| s.risk).unwrap_or(Risk::ReadOnly),
            subject,
        };
        session.emit(Event::ToolCallRequested { call }).await?;
    }
    let mut serial = Vec::new();
    let mut parallel = Vec::new();
    let mut unknown = HashMap::new();
    for (id, name, input) in calls {
        let Some(tool) = tools.get(&name) else {
            unknown.insert(id, failed_result(&format!("unknown tool {name}")));
            continue;
        };
        if tool.spec().concurrency == Concurrency::Exclusive {
            serial.push((id, tool.clone(), input));
        } else {
            parallel.push((id, tool.clone(), input));
        }
    }
    let mut done = unknown;
    for (id, tool, input) in serial {
        let (id, result) = run_one(session, id, tool, input).await;
        done.insert(id, result);
    }
    let cap = session.config.core.parallel_tools.max(1) as usize;
    let mut set = JoinSet::new();
    let mut inflight = 0usize;
    let mut rest = parallel.into_iter();
    loop {
        while inflight < cap {
            let Some((id, tool, input)) = rest.next() else {
                break;
            };
            let session = session.clone_handle();
            set.spawn(async move { run_one(&session, id, tool, input).await });
            inflight += 1;
        }
        let Some(joined) = set.join_next().await else {
            break;
        };
        inflight -= 1;
        match joined {
            Ok((id, result)) => {
                done.insert(id, result);
            }
            Err(_) => return Err(CoreError::Interrupted),
        }
    }
    let results: Vec<(CallId, ToolResult)> = order
        .into_iter()
        .map(|id| {
            let result = done
                .remove(&id)
                .unwrap_or_else(|| failed_result("tool did not return"));
            (id, result)
        })
        .collect();
    for (id, result) in &results {
        session
            .emit(Event::ToolCallDone {
                call_id: *id,
                result: result.clone(),
            })
            .await?;
    }
    Ok(results)
}

async fn run_one(
    session: &Session,
    id: CallId,
    tool: Arc<dyn Tool>,
    input: Value,
) -> (CallId, ToolResult) {
    let started = Instant::now();
    let (out_tx, mut out_rx) = mpsc::channel::<String>(32);
    let cx = ToolCx {
        roots: session.config.core.workspace_roots.clone(),
        cwd: session.cwd.clone(),
        sandbox: SandboxPolicy {
            mode: session.config.sandbox.mode,
            network: session.config.sandbox.network,
            writable: session.config.sandbox.writable.clone(),
            readonly_in_workspace: session.config.sandbox.readonly_in_workspace.clone(),
        },
        archive: session.archive.clone(),
        cancel: session.cancel_token(),
        output: out_tx,
        session: session.id,
        call: id,
    };
    let pump = session.clone_handle();
    let pump_id = id;
    tokio::spawn(async move {
        while let Some(delta) = out_rx.recv().await {
            let _ = pump
                .emit(Event::ToolCallOutput {
                    call_id: pump_id,
                    delta,
                })
                .await;
        }
    });
    let output = match tool.call(input, &cx).await {
        Ok(o) => o,
        Err(e) => ToolOutput {
            text: e.to_string(),
            is_error: true,
            diff: None,
            structured: None,
        },
    };
    let result = ToolResult {
        ok: !output.is_error,
        visible: output.text.clone(),
        archive: None,
        bytes: output.text.len() as u64,
        duration_ms: started.elapsed().as_millis() as u64,
        diff: output.diff,
    };
    (id, result)
}

fn failed_result(msg: &str) -> ToolResult {
    ToolResult {
        ok: false,
        visible: msg.into(),
        archive: None,
        bytes: msg.len() as u64,
        duration_ms: 0,
        diff: None,
    }
}

pub(crate) fn results_message(results: Vec<(CallId, ToolResult)>) -> Message {
    Message {
        role: Role::User,
        content: results
            .into_iter()
            .map(|(id, result)| Content::ToolResult {
                call_id: id,
                content: result.visible,
                is_error: !result.ok,
            })
            .collect(),
    }
}
