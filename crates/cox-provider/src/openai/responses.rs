//! Pure translation of a provider-neutral [`Request`] into an OpenAI
//! Responses body ([`build_body`]), and the SSE state machine that turns a
//! `POST /v1/responses` stream back into [`ProviderEvent`]s
//! ([`OpenAiResponsesStream`]). Same split as `anthropic::request`/
//! `anthropic::stream` (pure, no I/O) but combined into one file here since
//! T1.3's file list is just this module plus `super` (the client).
//!
//! **`input` shape.** Unlike Anthropic's per-message `content` array,
//! Responses' `input` is one flat list of items: a `Content::Text` becomes a
//! `message` item, a `Content::ToolUse` a `function_call` item, a
//! `Content::ToolResult` a `function_call_output` item — `message_items`
//! walks one `Message`'s content blocks into zero or more of these,
//! `build_body` concatenates every message's items into one flat `input`.
//!
//! **Ids.** Same reasoning as `anthropic::stream`: `call_id` on the wire is
//! an opaque provider string, never a valid ULID, so [`OpenAiResponsesStream`]
//! mints a fresh `CallId::new()` per `function_call` output item instead of
//! parsing it, and `message_items` round-trips that same cox-minted id back
//! out as both `function_call.call_id` and `function_call_output.call_id`.
//!
//! **Tool-call correlation.** `ProviderEvent::ToolUseInputDelta`/`ToolUseEnd`
//! carry no id (plan.md §1.2, same as Anthropic), and the Responses API opens
//! one `function_call` item fully (`output_item.added` … `arguments.done`)
//! before the next even for a parallel batch, so no per-item-id tracking is
//! needed here — unlike `anthropic::stream`, this state machine carries no
//! "current block" at all, because each event's own `type` (not a shared
//! `delta.type` field) already says what it is.
//!
//! **Thinking replay.** OpenAI reasoning items are opaque and provider-
//! specific like Anthropic's signed thinking blocks, but nothing in this
//! task wires up replaying them. A `Content::Thinking` with `signature: None`
//! is dropped silently (nothing to replay); one with `Some(_)` is a real
//! signed block from another provider that this translator cannot honour, so
//! `build_body` returns `ProviderError::Unsupported` rather than silently
//! dropping model-produced state — the one deliverable-mandated difference
//! from Anthropic's request builder, which stays infallible.
//! ponytail: reasoning-item replay unimplemented; add an `OpenAi`-specific
//! `Content` field (or a lookaside) when a task needs it end to end.

use async_trait::async_trait;
use cox_protocol::config::ProviderModel;
use cox_protocol::errors::ProviderError;
use cox_protocol::ids::CallId;
use cox_protocol::traits::Provider;
use cox_protocol::types::{
    Caps, Content, Effort, Message, ProviderEvent, ProviderId, Request, Role, StopReason, Usage,
};
use futures::StreamExt;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Translates a `Request` into the JSON body for `POST /v1/responses`.
/// Errors only when history carries a signed thinking block (see module
/// header) — every other `Request` shape translates unconditionally.
pub fn build_body(req: &Request) -> Result<Value, ProviderError> {
    let mut input = Vec::new();
    for m in &req.messages {
        input.extend(message_items(m)?);
    }

    let mut body = json!({
        "model": req.model.0,
        "input": input,
        "stream": true,
        "store": false,
        "max_output_tokens": req.max_tokens,
        "reasoning": {"effort": effort(req.effort)},
    });
    let obj = body.as_object_mut().expect("json! built an object");

    if !req.system.is_empty() {
        let instructions: Vec<&str> = req.system.iter().map(|b| b.text.as_str()).collect();
        obj.insert("instructions".into(), json!(instructions.join("\n\n")));
    }
    if !req.tools.is_empty() {
        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                })
            })
            .collect();
        obj.insert("tools".into(), Value::Array(tools));
    }
    Ok(body)
}

fn role_str(r: Role) -> &'static str {
    match r {
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

/// One message's content blocks as flat `input` items (see module header).
fn message_items(m: &Message) -> Result<Vec<Value>, ProviderError> {
    let mut items = Vec::new();
    for c in &m.content {
        match c {
            Content::Text { text } => items.push(json!({
                "type": "message",
                "role": role_str(m.role),
                "content": text,
            })),
            Content::ToolUse { id, name, input } => items.push(json!({
                "type": "function_call",
                "call_id": id.to_string(),
                "name": name,
                "arguments": input.to_string(),
            })),
            Content::ToolResult {
                call_id,
                content,
                is_error,
            } => {
                // `function_call_output` has no dedicated error flag on the
                // wire (unlike Anthropic's `tool_result.is_error`); folding
                // it into the text is the only way to carry it through.
                let output = if *is_error {
                    format!("Error: {content}")
                } else {
                    content.clone()
                };
                items.push(json!({
                    "type": "function_call_output",
                    "call_id": call_id.to_string(),
                    "output": output,
                }));
            }
            Content::Image {
                media_type,
                data_b64,
            } => items.push(json!({
                "type": "message",
                "role": role_str(m.role),
                "content": [{
                    "type": "input_image",
                    "image_url": format!("data:{media_type};base64,{data_b64}"),
                }],
            })),
            // Microcompaction: same treatment as `anthropic::request`.
            Content::Pointer { archive, summary } => items.push(json!({
                "type": "message",
                "role": role_str(m.role),
                "content": format!("[archived: {summary}; expand {}]", archive.id),
            })),
            Content::Thinking { signature, .. } => {
                if signature.is_some() {
                    return Err(ProviderError::Unsupported {
                        feature: "thinking replay".into(),
                    });
                }
                // No signature: nothing to replay, drop silently.
            }
        }
    }
    Ok(items)
}

fn effort(e: Effort) -> &'static str {
    e.name()
}

/// The state carried across one `POST /v1/responses` SSE body: just the
/// usage counters (see module header on why no "current block" is needed).
#[derive(Debug)]
pub struct OpenAiResponsesStream {
    usage: Usage,
    /// SSE frame ordinal, for `ProviderError::Parse { line }`.
    frame_no: u64,
}

impl Default for OpenAiResponsesStream {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenAiResponsesStream {
    /// Starts a fresh state machine for one streamed call.
    pub fn new() -> Self {
        Self {
            usage: Usage {
                input_tokens: 0,
                output_tokens: 0,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                estimated: false,
                cost_usd: 0.0,
                latency_ms: 0,
            },
            frame_no: 0,
        }
    }

    /// The usage accumulated so far (cost/latency filled in by the caller).
    pub fn usage(&self) -> Usage {
        self.usage
    }

    /// Feeds one SSE frame and returns the `ProviderEvent`s it produces.
    pub fn feed(
        &mut self,
        event: Option<&str>,
        data: &str,
    ) -> Result<Vec<ProviderEvent>, ProviderError> {
        self.frame_no += 1;
        let value: Value = serde_json::from_str(data).map_err(|_| ProviderError::Parse {
            line: self.frame_no,
        })?;
        // The JSON body's own "type" mirrors the SSE `event:` name (same
        // fallback `anthropic::stream` uses), so a frame missing the SSE
        // field is still routed correctly.
        let kind = event
            .map(str::to_string)
            .or_else(|| {
                value
                    .get("type")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_default();
        match kind.as_str() {
            "response.output_text.delta" => Ok(vec![ProviderEvent::TextDelta {
                text: str_field(&value, "delta"),
            }]),
            "response.output_item.added" => self.on_output_item_added(&value),
            "response.function_call_arguments.delta" => {
                Ok(vec![ProviderEvent::ToolUseInputDelta {
                    text: str_field(&value, "delta"),
                }])
            }
            "response.function_call_arguments.done" => Ok(vec![ProviderEvent::ToolUseEnd]),
            "response.completed" => self.on_completed(&value),
            "error" => self.on_error(&value).map(|e| vec![e]),
            // response.created/in_progress, content_part.*, output_text.done,
            // output_item.done, ping and anything unrecognised: no event, no
            // error — a stray frame must never fail the whole call.
            _ => Ok(vec![]),
        }
    }

    fn on_output_item_added(&mut self, v: &Value) -> Result<Vec<ProviderEvent>, ProviderError> {
        let item = v.get("item").ok_or(ProviderError::Parse {
            line: self.frame_no,
        })?;
        if item.get("type").and_then(Value::as_str) != Some("function_call") {
            // A `message` output item: its text arrives via
            // `response.output_text.delta`, nothing to emit here.
            return Ok(vec![]);
        }
        Ok(vec![ProviderEvent::ToolUseStart {
            id: CallId::new(),
            name: str_field(item, "name"),
        }])
    }

    fn on_completed(&mut self, v: &Value) -> Result<Vec<ProviderEvent>, ProviderError> {
        let response = v.get("response").ok_or(ProviderError::Parse {
            line: self.frame_no,
        })?;
        if let Some(usage) = response.get("usage") {
            self.apply_usage(usage);
        }
        Ok(vec![
            // §1.2 StopReason: a provider only ever emits EndTurn/Refusal/
            // Error; the Responses API surfaces refusal as a `refusal`
            // content part rather than a terminal status this task covers.
            ProviderEvent::Stop {
                stop: StopReason::EndTurn,
            },
            ProviderEvent::Usage { usage: self.usage },
        ])
    }

    fn on_error(&mut self, v: &Value) -> Result<ProviderEvent, ProviderError> {
        let code = v.get("code").and_then(Value::as_str).unwrap_or_default();
        let message = str_field(v, "message");
        let mapped = if code.contains("rate_limit") {
            ProviderError::RateLimited { retry_after: None }
        } else if code.contains("auth") || code.contains("api_key") {
            ProviderError::Auth
        } else {
            ProviderError::BadRequest { message }
        };
        Ok(ProviderEvent::Error { error: mapped })
    }

    /// Only overwrites what `response.usage` actually carries, same
    /// precedent as `anthropic::stream::apply_usage`. `cache_write_tokens`
    /// stays 0: unlike Anthropic, OpenAI does not bill a separate cache-write
    /// cost, and the task only asks for `input_tokens_details.cached_tokens`.
    fn apply_usage(&mut self, usage: &Value) {
        if let Some(n) = usage.get("input_tokens").and_then(Value::as_u64) {
            self.usage.input_tokens = n as u32;
        }
        if let Some(n) = usage.get("output_tokens").and_then(Value::as_u64) {
            self.usage.output_tokens = n as u32;
        }
        if let Some(n) = usage
            .get("input_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(Value::as_u64)
        {
            self.usage.cache_read_tokens = n as u32;
        }
    }
}

fn str_field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// A configured Responses-API client (`POST /v1/responses`): what OpenAI's
/// own models use, and what any gateway passing `api = "responses"` speaks.
/// Same thin-client shape as `chat::OpenAiChatProvider`: [`build_body`]
/// translates, [`OpenAiResponsesStream`] parses, this sends one and drives
/// the other over the shared `sse` framing.
pub struct OpenAiResponsesProvider {
    /// `providers.openai.base_url`, without a trailing slash.
    pub base_url: String,
    /// `None` means no `Authorization` header at all; `Some` is a bearer token.
    pub api_key: Option<String>,
    /// Known models with their context windows; the roomiest bounds
    /// [`Caps::max_context`] (which drives the compaction trigger).
    pub models: Vec<ProviderModel>,
    /// Fallback context window when `models` names nothing.
    pub context_window: u32,
    /// The shared connection pool.
    pub http: reqwest::Client,
}

impl OpenAiResponsesProvider {
    /// Builds a provider from the `[providers.openai]` section (or any
    /// compatible section passing `api = "responses"`).
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        models: Vec<ProviderModel>,
        context_window: u32,
    ) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key,
            models,
            context_window,
            http: reqwest::Client::new(),
        }
    }

    /// The roomiest known context window: a listed model if the request
    /// names one, else the section fallback.
    pub fn context_for(&self, model: &str) -> u32 {
        self.models
            .iter()
            .find(|m| m.id == model)
            .map(|m| m.context_window)
            .filter(|c| *c > 0)
            .unwrap_or(self.context_window)
    }
}

#[async_trait]
impl Provider for OpenAiResponsesProvider {
    fn id(&self) -> ProviderId {
        ProviderId::OpenAi
    }

    fn capabilities(&self) -> Caps {
        Caps {
            cache: false,
            // `reasoning.effort` is sent, but reasoning summaries are not
            // surfaced as `ThinkingDelta` (see the module header), so this
            // stays false until replay/surface lands.
            thinking: false,
            server_tools: false,
            count_tokens: false,
            max_context: self
                .models
                .iter()
                .map(|m| m.context_window)
                .max()
                .filter(|c| *c > 0)
                .unwrap_or(self.context_window),
        }
    }

    async fn stream(
        &self,
        req: Request,
        sink: mpsc::Sender<ProviderEvent>,
        cancel: CancellationToken,
    ) -> Result<Usage, ProviderError> {
        let started = std::time::Instant::now();
        let body = build_body(&req)?;

        let mut request = self
            .http
            .post(format!("{}/responses", self.base_url))
            .header("content-type", "application/json")
            .json(&body);
        if let Some(key) = &self.api_key {
            request = request.header("authorization", crate::http::bearer(key)?);
        }

        let response = request.send().await.map_err(|e| {
            if e.is_timeout() {
                ProviderError::Timeout
            } else {
                ProviderError::Network
            }
        })?;

        let status = response.status();
        if !status.is_success() {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let body_text = response.text().await.unwrap_or_default();
            return Err(crate::http::map_http_error(status, &body_text, retry_after));
        }

        let mut frames = std::pin::pin!(crate::sse::sse_stream(response.bytes_stream()));
        let mut machine = OpenAiResponsesStream::new();
        loop {
            let next = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(ProviderError::Cancelled),
                frame = frames.next() => frame,
            };
            let Some(frame) = next else {
                break;
            };
            let (event, data) = frame.map_err(|_| ProviderError::Network)?;
            for provider_event in machine.feed(event.as_deref(), &data)? {
                // The receiving end hung up: unwind as a cancellation
                // rather than silently dropping the rest of the call.
                if sink.send(provider_event).await.is_err() {
                    return Err(ProviderError::Cancelled);
                }
            }
        }

        let mut usage = machine.usage();
        usage.latency_ms = started.elapsed().as_millis() as u64;
        Ok(usage)
    }

    async fn count_tokens(&self, _req: &Request) -> Result<u32, ProviderError> {
        Err(ProviderError::Unsupported {
            feature: "count_tokens".into(),
        })
    }
}

/// Test-only: normalizes freshly minted `ToolUseStart` ids into stable,
/// counter-derived ones, same purpose (and same reasoning) as
/// `anthropic::stream::normalize_tool_ids` — kept as its own copy rather than
/// reused across modules since it is `#[cfg(test)]`-only, one caller each.
#[cfg(test)]
fn normalize_tool_ids(events: Vec<ProviderEvent>) -> Vec<ProviderEvent> {
    let mut next = 0u32;
    events
        .into_iter()
        .map(|event| match event {
            ProviderEvent::ToolUseStart { name, .. } => {
                let id = format!("{next:026}")
                    .parse()
                    .expect("26 decimal digits is a valid ULID shape");
                next += 1;
                ProviderEvent::ToolUseStart { id, name }
            }
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::str::FromStr;

    use cox_protocol::ids::{ArchiveId, CallId};
    use cox_protocol::types::{
        ArchiveRef, Concurrency, Job, ModelId, Risk, SystemBlock, Thinking, Tier, ToolSpec,
    };

    use super::*;
    use crate::sse::parse_sse_str;

    fn fixture(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/openai-responses")
            .join(format!("{name}.sse"));
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading fixture {path:?}: {e}"))
    }

    fn run_fixture(name: &str) -> Vec<ProviderEvent> {
        let mut stream = OpenAiResponsesStream::new();
        let mut events = Vec::new();
        for (event, data) in parse_sse_str(&fixture(name)) {
            events.extend(
                stream
                    .feed(event.as_deref(), &data)
                    .expect("fixture is well-formed"),
            );
        }
        normalize_tool_ids(events)
    }

    #[test]
    fn responses_stream_text_only() {
        insta::assert_json_snapshot!("responses_stream_text_only", run_fixture("text_only"));
    }

    #[test]
    fn responses_stream_one_tool_call() {
        insta::assert_json_snapshot!(
            "responses_stream_one_tool_call",
            run_fixture("one_tool_call")
        );
    }

    #[test]
    fn responses_stream_parallel_tool_calls() {
        let events = run_fixture("parallel_tool_calls");
        let starts = events
            .iter()
            .filter(|e| matches!(e, ProviderEvent::ToolUseStart { .. }))
            .count();
        assert_eq!(starts, 2, "expected two parallel function_call items");
        insta::assert_json_snapshot!("responses_stream_parallel_tool_calls", events);
    }

    #[test]
    fn responses_stream_usage_reads_cached_tokens() {
        let events = run_fixture("one_tool_call");
        let usage = events.iter().find_map(|e| match e {
            ProviderEvent::Usage { usage } => Some(*usage),
            _ => None,
        });
        assert_eq!(usage.map(|u| u.cache_read_tokens), Some(50));
    }

    #[test]
    fn responses_stream_malformed_json_is_a_parse_error_not_a_panic() {
        let mut stream = OpenAiResponsesStream::new();
        let err = stream
            .feed(Some("response.output_text.delta"), "{not json")
            .unwrap_err();
        assert!(matches!(err, ProviderError::Parse { line: 1 }));
    }

    #[test]
    fn responses_stream_unknown_event_is_ignored_not_fatal() {
        let mut stream = OpenAiResponsesStream::new();
        let events = stream
            .feed(Some("response.some_future_event"), "{}")
            .expect("ignored");
        assert!(events.is_empty());
    }

    fn call(n: u8) -> CallId {
        CallId::from_str(&format!("01ARZ3NDEKTSV4RRFFQ69G5FA{n}")).expect("valid ulid")
    }

    fn base(model: &str) -> Request {
        Request {
            tier: Tier::Code,
            job: Job::Main,
            model: ModelId(model.into()),
            system: vec![
                SystemBlock {
                    text: "<tool specs>".into(),
                    cache: true,
                },
                SystemBlock {
                    text: "You are cox.".into(),
                    cache: true,
                },
            ],
            tools: vec![ToolSpec {
                name: "read".into(),
                description: "Read a file".into(),
                input_schema: json!({"type": "object", "properties": {"path": {"type": "string"}}}),
                deferred: false,
                risk: Risk::ReadOnly,
                concurrency: Concurrency::Parallel,
            }],
            messages: vec![],
            effort: Effort::High,
            max_tokens: 16384,
            thinking: Thinking::Off,
            cache_breakpoints: vec![],
            stop_sequences: vec![],
        }
    }

    fn user_text(text: &str) -> Message {
        Message {
            role: Role::User,
            content: vec![Content::Text { text: text.into() }],
        }
    }

    #[test]
    fn responses_request_plain_text() {
        let mut req = base("gpt-5.1");
        req.messages = vec![user_text("what does cox-provider own?")];

        let body = build_body(&req).expect("no thinking blocks, never fails");
        insta::assert_json_snapshot!(body);
    }

    #[test]
    fn responses_request_parallel_tool_calls() {
        let mut req = base("gpt-5.1");
        req.messages = vec![
            user_text("read both files"),
            Message {
                role: Role::Assistant,
                content: vec![
                    Content::ToolUse {
                        id: call(1),
                        name: "read".into(),
                        input: json!({"path": "a.rs"}),
                    },
                    Content::ToolUse {
                        id: call(2),
                        name: "read".into(),
                        input: json!({"path": "b.rs"}),
                    },
                ],
            },
            Message {
                role: Role::User,
                content: vec![
                    Content::ToolResult {
                        call_id: call(1),
                        content: "fn a() {}".into(),
                        is_error: false,
                    },
                    Content::ToolResult {
                        call_id: call(2),
                        content: "no such file".into(),
                        is_error: true,
                    },
                ],
            },
        ];

        let body = build_body(&req).expect("no thinking blocks, never fails");
        let dumped = serde_json::to_string(&body).expect("serializes");
        assert!(dumped.contains("\"function_call\""));
        assert!(dumped.contains("\"function_call_output\""));
        assert!(dumped.contains("Error: no such file"));
        insta::assert_json_snapshot!(body);
    }

    #[test]
    fn responses_request_pointer_and_compaction_summary() {
        let mut req = base("gpt-5.1");
        req.messages = vec![Message {
            role: Role::User,
            content: vec![
                Content::Pointer {
                    archive: ArchiveRef {
                        id: ArchiveId::from_str("01ARZ3NDEKTSV4RRFFQ69G5FB0").expect("valid ulid"),
                        bytes: 91_000,
                    },
                    summary: "bash cargo test: 91000 bytes, exit 0".into(),
                },
                Content::Text {
                    text: "now fix the failing test".into(),
                },
            ],
        }];

        let body = build_body(&req).expect("no thinking blocks, never fails");
        insta::assert_json_snapshot!(body);
    }

    #[test]
    fn responses_request_signed_thinking_is_unsupported() {
        let mut req = base("gpt-5.1");
        req.messages = vec![Message {
            role: Role::Assistant,
            content: vec![Content::Thinking {
                text: "the parser is recursive descent".into(),
                signature: Some("sig-from-another-provider".into()),
            }],
        }];

        let err =
            build_body(&req).expect_err("a signed thinking block must not be dropped silently");
        assert!(matches!(err, ProviderError::Unsupported { .. }));
    }

    #[test]
    fn responses_request_unsigned_thinking_is_dropped_silently() {
        let mut req = base("gpt-5.1");
        req.messages = vec![Message {
            role: Role::Assistant,
            content: vec![Content::Thinking {
                text: "thought, never signed".into(),
                signature: None,
            }],
        }];

        let body = build_body(&req).expect("no signature: nothing to replay, not an error");
        assert_eq!(body["input"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn responses_request_effort_maps_to_reasoning() {
        let mut req = base("gpt-5.1");
        req.effort = Effort::Xhigh;
        let body = build_body(&req).expect("no thinking blocks, never fails");
        assert_eq!(body["reasoning"]["effort"], "xhigh");
    }

    /// "Done when": a wiremock shaped like `POST /responses` completes a
    /// tool-call turn end to end through the live client, with the bearer
    /// header the key implies.
    #[tokio::test]
    async fn responses_over_http() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/responses"))
            .and(wiremock::matchers::header(
                "authorization",
                "Bearer sk-test",
            ))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_raw(fixture("one_tool_call"), "text/event-stream"),
            )
            .mount(&server)
            .await;

        let client =
            OpenAiResponsesProvider::new(server.uri(), Some("sk-test".into()), vec![], 400_000);
        let mut req = base("gpt-5.1");
        req.messages = vec![user_text("read a.rs")];

        let (tx, mut rx) = mpsc::channel(64);
        let usage = client
            .stream(req, tx, CancellationToken::new())
            .await
            .expect("mock matched, so the header was sent");
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ProviderEvent::ToolUseStart { .. }))
        );
        assert!(events.iter().any(|e| matches!(
            e,
            ProviderEvent::Stop {
                stop: StopReason::EndTurn
            }
        )));
        assert_eq!(usage.input_tokens, 512);
        assert_eq!(usage.output_tokens, 24);
        assert_eq!(usage.cache_read_tokens, 50);
    }

    #[test]
    fn responses_context_for_prefers_listed_model() {
        use cox_protocol::config::ProviderModel;
        let client = OpenAiResponsesProvider::new(
            "https://api.openai.com/v1",
            None,
            vec![ProviderModel {
                id: "gpt-5.5".into(),
                context_window: 1_050_000,
                efforts: vec![],
            }],
            400_000,
        );
        assert_eq!(client.context_for("gpt-5.5"), 1_050_000);
        assert_eq!(client.context_for("gpt-unknown"), 400_000);
        assert_eq!(client.capabilities().max_context, 1_050_000);
        let bare = OpenAiResponsesProvider::new("https://x", None, vec![], 400_000);
        assert_eq!(bare.capabilities().max_context, 400_000);
    }
}
