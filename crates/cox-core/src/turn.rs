//! One provider call and its tool batch. Kept separate from `Session` so
//! the loop's `step` stays a state transition, not a grab-bag of I/O.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use cox_protocol::ArchivePut;
use cox_protocol::errors::CoreError;
use cox_protocol::ids::{CallId, ItemId};
use cox_protocol::traits::{Tool, ToolCx};
use cox_protocol::types::{
    Concurrency, Content, DecidedBy, Decision, Event, Message, Risk, Role, SandboxPolicy, ToolCall,
    ToolOutput, ToolResult, Usage,
};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::permission::Outcome;
use crate::session::{Session, State};

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
    let calls: Vec<ToolCall> = calls
        .into_iter()
        .map(|(id, name, input)| {
            let tool = tools.get(&name);
            ToolCall {
                id,
                subject: tool.map(|t| t.subject(&input)).unwrap_or_default(),
                // Per call, not per tool: `apply_patch` escalates to
                // `Destructive` on the patches that delete a lot of files.
                risk: tool.map(|t| t.risk(&input)).unwrap_or(Risk::ReadOnly),
                name,
                input,
            }
        })
        .collect();
    for call in &calls {
        session
            .emit(Event::ToolCallRequested { call: call.clone() })
            .await?;
    }
    // Gate serially so the user answers one prompt at a time and an
    // `AllowForSession` grant covers the calls behind it in the same batch.
    let mut serial = Vec::new();
    let mut parallel = Vec::new();
    let mut done = HashMap::new();
    for call in calls {
        let Some(tool) = tools.get(&call.name).cloned() else {
            done.insert(
                call.id,
                failed_result(&format!("unknown tool {}", call.name)),
            );
            continue;
        };
        let id = call.id;
        let call = match gate(session, tool.as_ref(), call).await? {
            Ok(call) => call,
            Err(result) => {
                done.insert(id, result);
                continue;
            }
        };
        session.dedup_invalidate(call.risk, &call.subject).await;
        if tool.spec().concurrency == Concurrency::Exclusive {
            serial.push((id, tool, call.input));
        } else {
            parallel.push((id, tool, call.input));
        }
    }
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

/// Asks the permission engine and, when it escalates, the user. Returns the
/// call to run (the user may have edited its input) or the failed result the
/// model sees. Auto-allows emit nothing: they are the common case and the
/// rollout already carries `ToolCallRequested`.
async fn gate(
    session: &Session,
    tool: &dyn Tool,
    mut call: ToolCall,
) -> Result<Result<ToolCall, ToolResult>, CoreError> {
    let id = call.id;
    let denied = |reason: &str| failed_result(&format!("permission denied: {reason}"));
    loop {
        let why = match session.decide(&call).await {
            Outcome::Allow { .. } => return Ok(Ok(call)),
            Outcome::Deny { reason, by } => {
                session
                    .emit(Event::ApprovalDecided {
                        call_id: id,
                        decision: Decision::Deny {
                            reason: reason.clone(),
                        },
                        by,
                    })
                    .await?;
                return Ok(Err(denied(&reason)));
            }
            Outcome::Ask(why) => why,
        };
        let rx = session.await_decision(id).await;
        session
            .emit(Event::ApprovalRequired {
                call: call.clone(),
                why,
            })
            .await?;
        let cancel = session.cancel_token();
        let decision = tokio::select! {
            biased;
            _ = cancel.cancelled() => Decision::Deny { reason: "interrupted".into() },
            d = rx => d.unwrap_or(Decision::Deny { reason: "session closed".into() }),
        };
        session.set_state(State::RunningTools).await;
        session
            .emit(Event::ApprovalDecided {
                call_id: id,
                decision: decision.clone(),
                by: DecidedBy::User,
            })
            .await?;
        match decision {
            Decision::Allow => return Ok(Ok(call)),
            Decision::AllowForSession => {
                session.grant(call.name.clone(), call.subject.clone()).await;
                return Ok(Ok(call));
            }
            Decision::Deny { reason } => return Ok(Err(denied(&reason))),
            // A rewritten input is a new call as far as the rules go: its
            // risk and subject change, so it goes back through `decide`.
            Decision::Edit { input } => {
                call.risk = tool.risk(&input);
                call.subject = tool.subject(&input);
                call.input = input;
            }
        }
    }
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
    // Only read-only calls are dedup candidates; keep what the key needs.
    let read_key =
        (tool.risk(&input) == Risk::ReadOnly).then(|| (input.clone(), tool.subject(&input)));
    let output = match tool.call(input, &cx).await {
        Ok(o) => o,
        Err(e) => ToolOutput {
            text: e.to_string(),
            is_error: true,
            diff: None,
            structured: None,
        },
    };
    let bytes = output.text.len() as u64;
    let archive = session
        .archive
        .put(ArchivePut {
            session: session.id,
            call: id,
            tool: tool.spec().name,
            subject: None,
            bytes: output.text.as_bytes().to_vec(),
        })
        .await;
    let (archive, visible) = match archive {
        Ok(id) => {
            let mut pointer = None;
            if let Some((input, subject)) = read_key.filter(|_| !output.is_error) {
                pointer = session
                    .dedup_observe(
                        &tool.spec().name,
                        &input,
                        &subject,
                        id,
                        output.text.as_bytes(),
                    )
                    .await;
            }
            let visible = pointer.unwrap_or_else(|| {
                crate::truncate::visible(
                    &output.text,
                    id,
                    session.config.context.tool_output_visible_bytes as usize,
                    session.config.context.tool_output_head_lines as usize,
                    session.config.context.tool_output_tail_lines as usize,
                )
            });
            (Some(cox_protocol::ArchiveRef { id, bytes }), visible)
        }
        Err(_) => (None, "tool output could not be archived".into()),
    };
    let result = ToolResult {
        ok: !output.is_error,
        visible,
        archive,
        bytes,
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
