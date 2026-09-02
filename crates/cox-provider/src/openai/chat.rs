//! The Chat Completions subset of the [OI] shape: what Ollama, vLLM, LM
//! Studio, llama.cpp and OpenRouter all speak (`POST /v1/chat/completions`).
//! Same split as every other backend: a *pure* request translator
//! ([`build_body`]) and a pure SSE → [`ProviderEvent`] state machine
//! ([`OpenAiChatStream`]), fixture-tested with no key and no socket (D12);
//! [`OpenAiChatProvider`] is the thin client that sends the body and drives
//! the machine over the shared `sse` framing.
//!
//! **Tool-call correlation.** Unlike Responses' one-item-at-a-time stream,
//! Chat interleaves parallel tool calls *by index*: each
//! `delta.tool_calls[i]` chunk carries the index of the call it belongs
//! to, arguments arrive split across many chunks, and the whole batch
//! finishes together on `finish_reason: "tool_calls"`. So this state
//! machine — unlike `responses` — keeps per-call state: a Vec of
//! accumulators keyed by wire index. Wire ids (`tool_call_id`) are opaque
//! provider strings (Ollama mints `call_xxx`, never a ULID), so cox mints
//! its own `CallId` per call and sends it back out as
//! `tool.role: "tool"`, `tool_call_id` — the same "cox owns the id space"
//! move `responses.rs` makes with `function_call.call_id`, and it works
//! for the same reason: cox owns the history, the server never has to
//! correlate our results with its own ids.
//!
//! **No auth by default.** Local servers (Ollama, vLLM, LM Studio,
//! llama.cpp) ignore or warn on `Authorization` headers, so the client
//! sends one only when a key was configured. OpenRouter is why a key can
//! be: same wire shape, real auth.
//!
//! **Thinking.** Chat has no reasoning-item replay (that is a Responses
//! feature), so `Content::Thinking` is treated exactly as in
//! `responses.rs`: unsigned dropped, signed rejected with `Unsupported`.

use async_trait::async_trait;
use cox_protocol::errors::ProviderError;
use cox_protocol::ids::CallId;
use cox_protocol::traits::Provider;
use cox_protocol::types::{
    Caps, Content, Message, ProviderEvent, ProviderId, Request, Role, StopReason, Usage,
};
use futures::StreamExt;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Translates a `Request` into the JSON body for `POST /v1/chat/completions`.
/// Errors only when history carries a signed thinking block (see module
/// header) — every other shape translates unconditionally.
pub fn build_body(req: &Request) -> Result<Value, ProviderError> {
    let mut messages = Vec::new();
    // Chat takes one `system` message; the blocks are joined in order.
    if !req.system.is_empty() {
        let text: Vec<&str> = req.system.iter().map(|b| b.text.as_str()).collect();
        messages.push(json!({"role": "system", "content": text.join("\n\n")}));
    }
    for m in &req.messages {
        messages.extend(message_items(m)?);
    }

    let mut body = json!({
        "model": req.model.0,
        "messages": messages,
        "stream": true,
        "stream_options": {"include_usage": true},
        "max_tokens": req.max_tokens,
    });
    let obj = body.as_object_mut().expect("json! built an object");

    if !req.tools.is_empty() {
        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    },
                })
            })
            .collect();
        obj.insert("tools".into(), Value::Array(tools));
    }
    if !req.stop_sequences.is_empty() {
        obj.insert("stop".into(), json!(req.stop_sequences));
    }
    Ok(body)
}

fn role_str(r: Role) -> &'static str {
    match r {
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

/// One message's content blocks as one or more Chat messages: tool use
/// joins its own assistant `tool_calls` message, each tool result becomes
/// its own `role: "tool"` message (the wire format has no batching for
/// them), everything else folds into one plain content message.
fn message_items(m: &Message) -> Result<Vec<Value>, ProviderError> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    let mut images = Vec::new();

    for c in &m.content {
        match c {
            Content::Text { text: t } => {
                if !text.is_empty() {
                    text.push_str("\n\n");
                }
                text.push_str(t);
            }
            Content::ToolUse { id, name, input } => tool_calls.push(json!({
                "id": id.to_string(),
                "type": "function",
                "function": {"name": name, "arguments": input.to_string()},
            })),
            Content::ToolResult {
                call_id,
                content,
                is_error,
            } => {
                // `role: "tool"` has no error flag on the wire; folding it
                // into the text is the only way to carry it through, same
                // move `responses.rs` makes on `function_call_output`.
                let out_text = if *is_error {
                    format!("Error: {content}")
                } else {
                    content.clone()
                };
                out.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id.to_string(),
                    "content": out_text,
                }));
            }
            Content::Image {
                media_type,
                data_b64,
            } => images.push(json!({
                "type": "image_url",
                "image_url": {"url": format!("data:{media_type};base64,{data_b64}")},
            })),
            // Microcompaction: same treatment as the other translators.
            Content::Pointer { archive, summary } => {
                if !text.is_empty() {
                    text.push_str("\n\n");
                }
                text.push_str(&format!("[archived: {summary}; expand {}]", archive.id));
            }
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

    if !images.is_empty() {
        // Multimodal content forces the array form of `content`; text, if
        // any, rides along as the first part.
        let mut parts = Vec::new();
        if !text.is_empty() {
            parts.push(json!({"type": "text", "text": text}));
        }
        parts.extend(images);
        out.push(json!({"role": role_str(m.role), "content": parts}));
    } else if !tool_calls.is_empty() {
        let mut msg = json!({"role": "assistant", "tool_calls": tool_calls});
        if !text.is_empty() {
            msg["content"] = json!(text);
        }
        out.push(msg);
    } else if !text.is_empty() || m.content.is_empty() {
        out.push(json!({"role": role_str(m.role), "content": text}));
    }
    Ok(out)
}

/// One tool call being accumulated across streamed deltas (see module
/// header: Chat interleaves parallel calls by wire index).
#[derive(Debug)]
pub struct AccruedCall {
    /// The cox id minted at `ToolUseStart`; sent back out as `tool_call_id`.
    pub id: CallId,
    /// The tool's name.
    pub name: String,
    /// The JSON input, accumulated one string chunk at a time.
    pub arguments: String,
    /// Whether `ToolUseStart` was emitted for this call yet.
    started: bool,
}

/// The state carried across one `POST /v1/chat/completions` SSE body:
/// per-index tool-call accumulators and the usage counters.
#[derive(Debug)]
pub struct OpenAiChatStream {
    calls: Vec<AccruedCall>,
    usage: Usage,
    /// SSE frame ordinal, for `ProviderError::Parse { line }`.
    frame_no: u64,
}

impl Default for OpenAiChatStream {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenAiChatStream {
    /// Starts a fresh state machine for one streamed call.
    pub fn new() -> Self {
        Self {
            calls: Vec::new(),
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
    /// Chat's wire has no named SSE events: every frame is `data: {...}`
    /// and is typed by shape — an `error` envelope is an error, a frame
    /// with `choices` is a delta, and the choice-less frame carrying only
    /// `usage` (sent last when `stream_options.include_usage` is set, as
    /// Ollama and vLLM do) just updates the counters.
    pub fn feed(&mut self, data: &str) -> Result<Vec<ProviderEvent>, ProviderError> {
        self.frame_no += 1;
        let value: Value = serde_json::from_str(data).map_err(|_| ProviderError::Parse {
            line: self.frame_no,
        })?;

        if let Some(error) = value.get("error").filter(|e| !e.is_null()) {
            return self.on_error(error);
        }

        let mut events = Vec::new();
        if let Some(choices) = value.get("choices").and_then(Value::as_array)
            && let Some(choice) = choices.first()
        {
            self.on_choice(choice, &mut events);
        }
        // Usage can ride on any frame (final frame when include_usage is
        // honoured, or inline on servers that ignore stream_options);
        // whatever arrives last wins, and only the fields it carries.
        if let Some(usage) = value.get("usage").filter(|u| !u.is_null()) {
            self.apply_usage(usage);
            // The event is emitted only for a *terminal* usage frame —
            // `include_usage`'s choice-less last frame, or usage riding on
            // the `finish_reason` frame — so a mid-stream usage field
            // updates the counters without duplicating the event.
            let terminal = value.get("choices").is_none_or(|c| {
                c.as_array().is_none_or(|a| {
                    a.is_empty()
                        || a.first().is_some_and(|ch| {
                            ch.get("finish_reason")
                                .and_then(Value::as_str)
                                .is_some_and(|f| !f.is_empty())
                        })
                })
            });
            if terminal {
                events.push(ProviderEvent::Usage { usage: self.usage });
            }
        }
        Ok(events)
    }

    fn on_choice(&mut self, choice: &Value, events: &mut Vec<ProviderEvent>) {
        if let Some(delta) = choice.get("delta") {
            // DeepSeek/Qwen-style local reasoning field; vLLM and OpenRouter
            // surface it too. Absent on plain Ollama — the filter skips it.
            if let Some(reasoning) = delta.get("reasoning_content").and_then(Value::as_str)
                && !reasoning.is_empty()
            {
                events.push(ProviderEvent::ThinkingDelta {
                    text: reasoning.to_string(),
                });
            }
            if let Some(content) = delta.get("content").and_then(Value::as_str)
                && !content.is_empty()
            {
                events.push(ProviderEvent::TextDelta {
                    text: content.to_string(),
                });
            }
            if let Some(tool_chunks) = delta.get("tool_calls").and_then(Value::as_array) {
                for chunk in tool_chunks {
                    self.on_tool_call_chunk(chunk, events);
                }
            }
        }
        if let Some(finish) = choice.get("finish_reason").and_then(Value::as_str) {
            match finish {
                // §1.2 StopReason: a provider only ever emits EndTurn/
                // Refusal/Error. `tool_calls`, `stop`, `length` and any
                // unknown reason all collapse to `EndTurn` here —
                // `cox-core` infers tool use from the `ToolUseStart`s it
                // saw (same collapse `anthropic::stream` performs for
                // `tool_use`/`max_tokens`/`stop_sequence`).
                "" | "tool_calls" | "stop" | "length" => events.push(ProviderEvent::Stop {
                    stop: StopReason::EndTurn,
                }),
                "content_filter" => events.push(ProviderEvent::Stop {
                    stop: StopReason::Refusal {
                        detail: "content_filter".into(),
                    },
                }),
                _ => events.push(ProviderEvent::Stop {
                    stop: StopReason::EndTurn,
                }),
            }
        }
    }

    /// One `delta.tool_calls[i]` chunk: index-keyed accumulation (module
    /// header). The first chunk for an index carries `id` + `function.name`
    /// and emits `ToolUseStart`; later chunks append to `arguments`.
    fn on_tool_call_chunk(&mut self, chunk: &Value, events: &mut Vec<ProviderEvent>) {
        let idx = chunk.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
        while self.calls.len() <= idx {
            self.calls.push(AccruedCall {
                id: CallId::new(),
                name: String::new(),
                arguments: String::new(),
                started: false,
            });
        }
        let call = &mut self.calls[idx];

        if let Some(name) = chunk
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(Value::as_str)
            && !name.is_empty()
        {
            call.name = name.to_string();
        }

        // Emit `ToolUseStart` once, on the chunk that first names the tool.
        // The name is set above and the id was minted when the accumulator
        // was created, so a second name chunk (some servers resend it) is
        // idempotent: `started` is a separate flag.
        if !call.name.is_empty() && !call.started {
            call.started = true;
            events.push(ProviderEvent::ToolUseStart {
                id: call.id,
                name: call.name.clone(),
            });
        }
        if let Some(args) = chunk
            .get("function")
            .and_then(|f| f.get("arguments"))
            .and_then(Value::as_str)
            && !args.is_empty()
        {
            call.arguments.push_str(args);
            events.push(ProviderEvent::ToolUseInputDelta {
                text: args.to_string(),
            });
        }
    }

    fn on_error(&mut self, error: &Value) -> Result<Vec<ProviderEvent>, ProviderError> {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let kind = error
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let mapped = if message.contains("rate limit") || kind.contains("rate_limit") {
            ProviderError::RateLimited { retry_after: None }
        } else if kind.contains("auth")
            || message.to_lowercase().contains("api key")
            || message.to_lowercase().contains("unauthorized")
        {
            ProviderError::Auth
        } else {
            ProviderError::BadRequest { message }
        };
        Ok(vec![ProviderEvent::Error { error: mapped }])
    }

    /// Only overwrites what `usage` actually carries, same precedent as
    /// `anthropic::stream::apply_usage`. Some servers report
    /// `prompt_tokens_details.cached_tokens` (Ollama does not, OpenRouter
    /// does); `cache_write_tokens` stays 0 — Chat has no cache-write bill.
    fn apply_usage(&mut self, usage: &Value) {
        if let Some(n) = usage.get("prompt_tokens").and_then(Value::as_u64) {
            self.usage.input_tokens = n as u32;
        }
        if let Some(n) = usage.get("completion_tokens").and_then(Value::as_u64) {
            self.usage.output_tokens = n as u32;
        }
        if let Some(n) = usage
            .get("prompt_tokens_details")
            .and_then(|d| d.get("cached_tokens"))
            .and_then(Value::as_u64)
        {
            self.usage.cache_read_tokens = n as u32;
        }
    }
}

/// A configured Chat-Completions client for a local (or OpenRouter-shaped)
/// server.
pub struct OpenAiChatProvider {
    /// `providers.local.base_url`, without a trailing slash.
    pub base_url: String,
    /// `None` means no `Authorization` header at all (module header: local
    /// servers ignore or warn on it); `Some` is sent as a bearer token.
    pub api_key: Option<String>,
    /// `providers.local.context_window` (local servers don't report it).
    pub context_window: u32,
    /// The shared connection pool.
    pub http: reqwest::Client,
}

impl OpenAiChatProvider {
    /// Builds a provider from the `[providers.local]` config section.
    pub fn new(cfg: &cox_protocol::config::LocalProviderConfig) -> Self {
        Self {
            base_url: cfg.base_url.trim_end_matches('/').to_string(),
            api_key: None,
            context_window: cfg.context_window,
            http: reqwest::Client::new(),
        }
    }

    /// Builds one with an API key (OpenRouter-shaped servers).
    pub fn with_key(cfg: &cox_protocol::config::LocalProviderConfig, api_key: String) -> Self {
        Self {
            api_key: Some(api_key),
            ..Self::new(cfg)
        }
    }
}

#[async_trait]
impl Provider for OpenAiChatProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Local
    }

    fn capabilities(&self) -> Caps {
        Caps {
            cache: false,
            thinking: true,
            server_tools: false,
            count_tokens: false,
            max_context: self.context_window,
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
            .post(format!("{}/chat/completions", self.base_url))
            .header("content-type", "application/json")
            .json(&body);
        if let Some(key) = &self.api_key {
            // OpenAI-compatible servers expect `Authorization: Bearer <key>`.
            let mut v = reqwest::header::HeaderValue::from_str(&format!("Bearer {key}"))
                .map_err(|_| ProviderError::Auth)?;
            v.set_sensitive(true);
            request = request.header("authorization", v);
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
            return Err(http_error(status, &body_text, retry_after));
        }

        let mut frames = std::pin::pin!(crate::sse::sse_stream(response.bytes_stream()));
        let mut machine = OpenAiChatStream::new();
        loop {
            let next = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(ProviderError::Cancelled),
                frame = frames.next() => frame,
            };
            let Some(frame) = next else {
                break;
            };
            let (_event, data) = frame.map_err(|_| ProviderError::Network)?;
            for provider_event in machine.feed(&data)? {
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
        // No dedicated endpoint on local servers; T1.8's estimate covers it.
        Err(ProviderError::Unsupported {
            feature: "count_tokens".into(),
        })
    }
}

/// Maps a non-2xx `/chat/completions` response to a `ProviderError` (plan.md
/// §1.14), same shape as `anthropic::http_error`.
fn http_error(status: reqwest::StatusCode, body: &str, retry_after: Option<u64>) -> ProviderError {
    let message = error_message(body);
    match status.as_u16() {
        401 | 403 => ProviderError::Auth,
        429 => ProviderError::RateLimited { retry_after },
        500 | 502 | 503 | 504 => ProviderError::Overloaded,
        400 | 413 => match parse_context_too_long(&message) {
            Some((got, max)) => ProviderError::ContextTooLong { max, got },
            None => ProviderError::BadRequest { message },
        },
        _ => ProviderError::BadRequest { message },
    }
}

/// The Chat error envelope is `{"error": {"message": ..., "type": ...}}`
/// (Ollama's is the same shape); falls back to the raw body so a proxy's
/// plain-text error is not lost.
fn error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("error")?.get("message")?.as_str().map(str::to_string))
        .unwrap_or_else(|| body.to_string())
}

/// Best-effort `(got, max)` extraction from a "context length exceeded …
/// N tokens … M maximum"-shaped message; local servers phrase it wildly
/// differently (Ollama: "input length exceeds context length"), so this is
/// read-only best effort: no "exceed"/"too long"/"context" mention or
/// fewer than two numbers falls back to `BadRequest`.
fn parse_context_too_long(message: &str) -> Option<(u32, u32)> {
    let lower = message.to_ascii_lowercase();
    if !(lower.contains("too long") || lower.contains("exceed")) {
        return None;
    }
    let mut numbers = message
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<u32>().ok());
    let got = numbers.next()?;
    let max = numbers.next()?;
    Some((got, max))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::str::FromStr;

    use cox_protocol::ids::ArchiveId;
    use cox_protocol::types::{
        ArchiveRef, Concurrency, Effort, Job, ModelId, Risk, SystemBlock, Thinking, Tier, ToolSpec,
    };

    use super::*;
    use crate::sse::parse_sse_str;

    fn fixture(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/openai-chat")
            .join(format!("{name}.sse"));
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading fixture {path:?}: {e}"))
    }

    /// Test-only: normalizes freshly minted `ToolUseStart` ids into stable,
    /// counter-derived ones — without it every run snapshots a different
    /// random ULID (see `stream.rs`/`responses.rs` for the same move).
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

    fn run_fixture(name: &str) -> Vec<ProviderEvent> {
        let mut stream = OpenAiChatStream::new();
        let mut events = Vec::new();
        for (_event, data) in parse_sse_str(&fixture(name)) {
            events.extend(stream.feed(&data).expect("fixture is well-formed"));
        }
        normalize_tool_ids(events)
    }

    #[test]
    fn chat_stream_text_only() {
        insta::assert_json_snapshot!("chat_stream_text_only", run_fixture("text_only"));
    }

    #[test]
    fn chat_stream_one_tool_call() {
        insta::assert_json_snapshot!("chat_stream_one_tool_call", run_fixture("one_tool_call"));
    }

    #[test]
    fn chat_stream_parallel_tool_calls_by_index() {
        let events = run_fixture("parallel_tool_calls");
        let starts = events
            .iter()
            .filter(|e| matches!(e, ProviderEvent::ToolUseStart { .. }))
            .count();
        assert_eq!(starts, 2, "two interleaved-by-index calls: {events:?}");
        let stops = events
            .iter()
            .filter(|e| matches!(e, ProviderEvent::Stop { .. }))
            .count();
        assert_eq!(stops, 1, "one terminal finish_reason for the batch");
        insta::assert_json_snapshot!("chat_stream_parallel_tool_calls", events);
    }

    #[test]
    fn chat_stream_reasoning_content_becomes_thinking() {
        let events = run_fixture("reasoning");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ProviderEvent::ThinkingDelta { .. })),
            "reasoning_content must map to ThinkingDelta: {events:?}"
        );
    }

    #[test]
    fn chat_stream_usage_frame_read() {
        let events = run_fixture("text_only");
        let usage = events
            .iter()
            .find_map(|e| match e {
                ProviderEvent::Usage { usage } => Some(*usage),
                _ => None,
            })
            .expect("usage frame present");
        assert_eq!(usage.input_tokens, 12);
        assert_eq!(usage.output_tokens, 34);
        assert_eq!(usage.cache_read_tokens, 5);
    }
    #[test]
    fn chat_stream_malformed_json_is_parse_error_not_panic() {
        let mut stream = OpenAiChatStream::new();
        let err = stream.feed("{not json").unwrap_err();
        assert!(matches!(err, ProviderError::Parse { line: 1 }));
    }

    #[test]
    fn chat_stream_unknown_frame_is_ignored_not_fatal() {
        let mut stream = OpenAiChatStream::new();
        let events = stream
            .feed(r#"{"id":"x","object":"chat.completion.chunk","some_future_field":1}"#)
            .expect("ignored");
        assert!(events.is_empty());
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

    fn call(n: u8) -> CallId {
        CallId::from_str(&format!("01ARZ3NDEKTSV4RRFFQ69G5FA{n}")).expect("valid ulid")
    }

    #[test]
    fn chat_request_plain_text() {
        let mut req = base("qwen3-coder");
        req.messages = vec![user_text("read a.rs")];
        let body = build_body(&req).expect("no thinking blocks");
        insta::assert_json_snapshot!(body);
    }

    #[test]
    fn chat_request_tool_roundtrip() {
        let mut req = base("qwen3-coder");
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
        let body = build_body(&req).expect("no thinking blocks");
        let dumped = serde_json::to_string(&body).expect("serializes");
        assert!(dumped.contains("\"tool_calls\""));
        assert!(dumped.contains("\"tool_call_id\""));
        assert!(dumped.contains("Error: no such file"));
        insta::assert_json_snapshot!(body);
    }

    #[test]
    fn chat_request_signed_thinking_unsupported() {
        let mut req = base("qwen3-coder");
        req.messages = vec![Message {
            role: Role::Assistant,
            content: vec![Content::Thinking {
                text: "signed elsewhere".into(),
                signature: Some("sig".into()),
            }],
        }];
        let err = build_body(&req).expect_err("signed thinking must not drop silently");
        assert!(matches!(err, ProviderError::Unsupported { .. }));
    }

    #[test]
    fn chat_request_unsigned_thinking_dropped() {
        let mut req = base("qwen3-coder");
        req.messages = vec![Message {
            role: Role::Assistant,
            content: vec![
                Content::Thinking {
                    text: "thought, never signed".into(),
                    signature: None,
                },
                Content::Text {
                    text: "here is the answer".into(),
                },
            ],
        }];
        let body = build_body(&req).expect("no signature: nothing to replay");
        let msgs = body["messages"].as_array().expect("messages");
        let last = msgs.last().expect("at least one message");
        assert_eq!(last["content"], "here is the answer");
    }

    #[test]
    fn chat_request_pointer_rendered_as_text() {
        let mut req = base("qwen3-coder");
        req.messages = vec![Message {
            role: Role::User,
            content: vec![
                Content::Pointer {
                    archive: ArchiveRef {
                        id: ArchiveId::from_str("01ARZ3NDEKTSV4RRFFQ69G5FB0").expect("ulid"),
                        bytes: 91_000,
                    },
                    summary: "bash cargo test: 91000 bytes, exit 0".into(),
                },
                Content::Text {
                    text: "now fix it".into(),
                },
            ],
        }];
        let body = build_body(&req).expect("no thinking blocks");
        let msgs = body["messages"].as_array().expect("messages");
        let last = msgs.last().expect("user message follows system");
        let content = last["content"].as_str().expect("text content");
        assert!(content.contains("[archived:"));
        assert!(content.contains("now fix it"));
    }

    #[test]
    fn chat_request_stop_sequences() {
        let mut req = base("qwen3-coder");
        req.stop_sequences = vec!["```".into()];
        let body = build_body(&req).expect("no thinking blocks");
        assert_eq!(body["stop"], json!(["```"]));
    }

    #[test]
    fn chat_http_error_maps_known_statuses() {
        assert!(matches!(
            http_error(reqwest::StatusCode::UNAUTHORIZED, "{}", None),
            ProviderError::Auth
        ));
        assert!(matches!(
            http_error(reqwest::StatusCode::TOO_MANY_REQUESTS, "{}", Some(7)),
            ProviderError::RateLimited {
                retry_after: Some(7)
            }
        ));
        let body = r#"{"error":{"message":"input length exceeds context length: 40000 tokens > 32768 maximum","type":"invalid_request_error"}}"#;
        assert_eq!(
            http_error(reqwest::StatusCode::BAD_REQUEST, body, None),
            ProviderError::ContextTooLong {
                max: 32_768,
                got: 40_000,
            }
        );
        let body = r#"{"error":{"message":"messages: role must alternate","type":"invalid_request_error"}}"#;
        assert_eq!(
            http_error(reqwest::StatusCode::BAD_REQUEST, body, None),
            ProviderError::BadRequest {
                message: "messages: role must alternate".into()
            }
        );
    }

    /// "Done when": a wiremock shaped like Ollama's
    /// `/v1/chat/completions` completes a tool-call turn end to end.
    /// The later-mounted mock only matches when an `Authorization` header
    /// *is* sent and answers 401 — wiremock prefers later mounts, so a
    /// local server wrongly getting the header fails the test (step 3).
    #[tokio::test]
    async fn chat_over_http_ollama_shaped() {
        let fixture = fixture("one_tool_call");
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/chat/completions"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_raw(fixture, "text/event-stream"),
            )
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/chat/completions"))
            .and(wiremock::matchers::header_exists("authorization"))
            .respond_with(wiremock::ResponseTemplate::new(401).set_body_string("no auth wanted"))
            .mount(&server)
            .await;

        // No api_key: a local server, so no Authorization header is built.
        let client = OpenAiChatProvider {
            base_url: server.uri(),
            api_key: None,
            context_window: 32_768,
            http: reqwest::Client::new(),
        };
        let mut req = base("qwen3-coder");
        req.messages = vec![user_text("read a.rs")];

        let (tx, mut rx) = mpsc::channel(64);
        let usage = client
            .stream(req, tx, CancellationToken::new())
            .await
            .expect("stream succeeds");

        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ProviderEvent::ToolUseStart { .. }))
        );
        // §1.2 StopReason convention: `finish_reason: "tool_calls"` is
        // `EndTurn` from a provider; the core infers tool use.
        assert!(events.iter().any(|e| matches!(
            e,
            ProviderEvent::Stop {
                stop: StopReason::EndTurn
            }
        )));
        assert_eq!(usage.input_tokens, 11);
        assert_eq!(usage.output_tokens, 29);
    }

    /// The OpenRouter shape: same wire, but the bearer header is sent.
    #[tokio::test]
    async fn chat_over_http_with_key_sends_bearer() {
        let fixture = fixture("text_only");
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/chat/completions"))
            .and(wiremock::matchers::header(
                "authorization",
                "Bearer sk-or-test",
            ))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_raw(fixture, "text/event-stream"),
            )
            .mount(&server)
            .await;

        let client = OpenAiChatProvider {
            base_url: server.uri(),
            api_key: Some("sk-or-test".into()),
            context_window: 128_000,
            http: reqwest::Client::new(),
        };
        let mut req = base("qwen3-coder");
        req.messages = vec![user_text("hello")];

        let (tx, mut rx) = mpsc::channel(64);
        let usage = client
            .stream(req, tx, CancellationToken::new())
            .await
            .expect("mock matched, so the header was sent");
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        assert!(events.iter().any(|e| matches!(
            e,
            ProviderEvent::Stop {
                stop: StopReason::EndTurn
            }
        )));
        assert_eq!(usage.output_tokens, 34);
    }
}
