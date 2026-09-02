//! The Anthropic Messages API backend: the client handle, its headers and
//! its credential lookup. The body translation lives next door in
//! [`request`] so it can be snapshot-tested with no key and no socket; the
//! SSE → `ProviderEvent` state machine lives in [`stream`] for the same
//! reason (fixtures, no socket).
//!
//! Kept apart from the OpenAI backends because the fields that decide cost
//! here — `cache_control` placement, `output_config.effort`, adaptive
//! `thinking`, `fallbacks` — have no counterpart there (D3).

pub mod request;
pub mod stream;

use async_trait::async_trait;
use cox_protocol::errors::ProviderError;
use cox_protocol::traits::Provider;
use cox_protocol::types::{Caps, ProviderEvent, ProviderId, Request, Usage};
use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// The API version cox pins; Anthropic requires it on every request.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

/// The beta that gates the scalar `fallbacks: "default"` form. The array
/// form uses `server-side-fallback-2026-06-01` instead and pairing either
/// header with the other form is a 400, so the two are never both sent.
pub const FALLBACKS_BETA: &str = "server-side-fallback-2026-07-01";

/// How long a cache entry written by a `cache_control` breakpoint lives
/// (`providers.anthropic.cache_ttl`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CacheTtl {
    /// The API default.
    #[default]
    FiveMinutes,
    /// Longer TTL for bursty sessions; writes cost more.
    OneHour,
}

impl CacheTtl {
    /// The wire value for `cache_control.ttl`.
    pub fn as_str(self) -> &'static str {
        match self {
            CacheTtl::FiveMinutes => "5m",
            CacheTtl::OneHour => "1h",
        }
    }
}

/// A configured Anthropic Messages client.
///
/// Fields are public because `cox-core` builds one straight from
/// `[providers.anthropic]`; there is no behaviour worth hiding behind
/// accessors.
pub struct AnthropicProvider {
    /// `providers.anthropic.base_url`, without a trailing slash.
    pub base_url: String,
    /// The resolved credential (see [`resolve_api_key`]).
    pub api_key: String,
    /// TTL written into every `cache_control` block.
    pub ttl: CacheTtl,
    /// Whether to send `fallbacks: "default"` and its beta header.
    pub fallbacks: bool,
    /// The shared connection pool.
    pub http: reqwest::Client,
}

impl AnthropicProvider {
    /// Builds a provider, resolving the credential from the environment or
    /// the keyring. Fails with [`ProviderError::Auth`] rather than panicking
    /// when neither has one.
    pub fn new(
        base_url: impl Into<String>,
        ttl: CacheTtl,
        fallbacks: bool,
    ) -> Result<Self, ProviderError> {
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: resolve_api_key()?,
            ttl,
            fallbacks,
            http: reqwest::Client::new(),
        })
    }

    /// The headers every call carries. `anthropic-beta` is assembled from
    /// the features actually enabled, so a request never claims a beta it
    /// does not use (some betas 400 when paired with the wrong body shape).
    pub fn headers(&self) -> Result<HeaderMap, ProviderError> {
        let mut h = HeaderMap::new();
        h.insert("content-type", HeaderValue::from_static("application/json"));
        h.insert(
            "anthropic-version",
            HeaderValue::from_static(ANTHROPIC_VERSION),
        );
        // An api key with non-ASCII bytes is a misconfigured credential, not
        // a transport failure: report it as an auth problem.
        let mut key = HeaderValue::from_str(&self.api_key).map_err(|_| ProviderError::Auth)?;
        key.set_sensitive(true);
        h.insert("x-api-key", key);

        let betas = self.betas();
        if !betas.is_empty() {
            let value = HeaderValue::from_str(&betas.join(",")).map_err(|_| {
                ProviderError::Unsupported {
                    feature: "anthropic-beta".into(),
                }
            })?;
            h.insert("anthropic-beta", value);
        }
        Ok(h)
    }

    /// The `anthropic-beta` values implied by the enabled features.
    pub fn betas(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.fallbacks {
            v.push(FALLBACKS_BETA);
        }
        v
    }

    /// The config [`request::build_body`] needs from this provider.
    pub fn build_cfg<'a>(
        &self,
        thinking_model: Option<&'a cox_protocol::types::ModelId>,
    ) -> request::BuildCfg<'a> {
        request::BuildCfg {
            ttl: self.ttl,
            fallbacks: self.fallbacks,
            thinking_model,
        }
    }
}

/// `ANTHROPIC_API_KEY` first, else the keyring entry `cox/anthropic`.
///
/// A missing or unreadable credential is [`ProviderError::Auth`]; this is
/// called on every provider construction, including in `cox doctor`, so it
/// must never panic.
pub fn resolve_api_key() -> Result<String, ProviderError> {
    match std::env::var("ANTHROPIC_API_KEY") {
        Ok(k) if !k.trim().is_empty() => return Ok(k),
        _ => {}
    }
    // `keyring`'s default features carry the native store for each platform
    // (Keychain, Credential Manager, Secret Service over zbus), so this is
    // the real system store, not a mock.
    keyring::Entry::new("cox", "anthropic")
        .and_then(|e| e.get_password())
        .map_err(|_| ProviderError::Auth)
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Anthropic
    }

    fn capabilities(&self) -> Caps {
        Caps {
            cache: true,
            thinking: true,
            server_tools: true,
            count_tokens: true,
            // The floor across the first-party lineup; per-model windows are
            // a routing-table concern (plan.md §1.4), not a provider one.
            max_context: 200_000,
        }
    }

    async fn stream(
        &self,
        req: Request,
        sink: mpsc::Sender<ProviderEvent>,
        cancel: CancellationToken,
    ) -> Result<Usage, ProviderError> {
        let started = std::time::Instant::now();
        // No `ModelSwitched` signal reaches a `Provider`: `stream`'s
        // signature (cox-protocol T0.2) carries only the `Request`, so
        // thinking-block replay stays off here — see request.rs's
        // `BuildCfg::thinking_model` doc and this module's `stream` doc.
        let body = request::build_body(&req, self.build_cfg(None));

        let response = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
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
        let mut machine = stream::AnthropicStream::new();
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
                // The receiving end (`cox-core`) hung up: nothing left to
                // stream to, so unwind as a cancellation rather than
                // silently dropping the rest of the call.
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
        // `POST {base_url}/v1/messages/count_tokens` with the same body;
        // out of scope for T1.2 (streaming only) — lands in T1.8.
        Err(ProviderError::Unsupported {
            feature: "count_tokens lands in T1.8".into(),
        })
    }
}

/// Maps a non-2xx `/v1/messages` response to a `ProviderError` (plan.md
/// §1.14). `retry_after` comes from the `retry-after` header, read before
/// the body is consumed.
fn http_error(status: reqwest::StatusCode, body: &str, retry_after: Option<u64>) -> ProviderError {
    let message = error_message(body);
    match status.as_u16() {
        401 => ProviderError::Auth,
        429 => ProviderError::RateLimited { retry_after },
        503 | 529 => ProviderError::Overloaded,
        // A too-long prompt is a 400 `invalid_request_error` in practice
        // (413 is reserved for raw request-body size); handled the same way
        // regardless of which status carried it.
        400 | 413 => match parse_context_too_long(&message) {
            Some((got, max)) => ProviderError::ContextTooLong { max, got },
            None => ProviderError::BadRequest { message },
        },
        _ => ProviderError::BadRequest { message },
    }
}

/// Anthropic's error envelope is `{"error": {"type": ..., "message": ...}}`
/// (claude-api skill, `shared/error-codes.md`); falls back to the raw body
/// when it is not that shape, so a proxy's plain-text error is not lost.
fn error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("error")?.get("message")?.as_str().map(str::to_string))
        .unwrap_or_else(|| body.to_string())
}

/// Best-effort extraction of `(got, max)` from a "prompt is too long: N
/// tokens > M maximum"-shaped message. No fixed wire schema is documented
/// for this case, so this is read-only best effort: a message that does not
/// mention "too long", or that carries fewer than two numbers, falls back
/// to a plain `BadRequest` in [`http_error`] rather than guessing.
fn parse_context_too_long(message: &str) -> Option<(u32, u32)> {
    if !message.to_ascii_lowercase().contains("too long") {
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
    use super::*;

    fn provider(fallbacks: bool) -> AnthropicProvider {
        AnthropicProvider {
            base_url: "https://api.anthropic.com".into(),
            api_key: "sk-test".into(),
            ttl: CacheTtl::FiveMinutes,
            fallbacks,
            http: reqwest::Client::new(),
        }
    }

    #[test]
    fn beta_header_only_lists_enabled_features() {
        assert!(provider(false).betas().is_empty());
        assert_eq!(provider(true).betas(), vec![FALLBACKS_BETA]);

        let h = provider(false).headers().expect("headers build");
        assert!(h.get("anthropic-beta").is_none());
        assert_eq!(
            h.get("anthropic-version").and_then(|v| v.to_str().ok()),
            Some(ANTHROPIC_VERSION)
        );

        let h = provider(true).headers().expect("headers build");
        assert_eq!(
            h.get("anthropic-beta").and_then(|v| v.to_str().ok()),
            Some(FALLBACKS_BETA)
        );
    }

    #[test]
    fn missing_credential_is_auth_error_not_a_panic() {
        // Safety: single-threaded test process section; no other thread reads
        // the environment while this runs.
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
        // Either the keyring holds a real entry (developer machine) or it
        // does not; both outcomes are a `Result`, never a panic.
        let _ = resolve_api_key();

        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-from-env") };
        assert_eq!(resolve_api_key().ok().as_deref(), Some("sk-from-env"));
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
    }

    #[test]
    fn http_error_maps_known_statuses() {
        assert!(matches!(
            http_error(reqwest::StatusCode::UNAUTHORIZED, "{}", None),
            ProviderError::Auth
        ));
        assert!(matches!(
            http_error(reqwest::StatusCode::TOO_MANY_REQUESTS, "{}", Some(30)),
            ProviderError::RateLimited {
                retry_after: Some(30)
            }
        ));
        assert!(matches!(
            http_error(reqwest::StatusCode::SERVICE_UNAVAILABLE, "{}", None),
            ProviderError::Overloaded
        ));
        let body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"prompt is too long: 250000 tokens > 200000 maximum"}}"#;
        assert_eq!(
            http_error(reqwest::StatusCode::BAD_REQUEST, body, None),
            ProviderError::ContextTooLong {
                max: 200_000,
                got: 250_000,
            }
        );
        let body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"messages: roles must alternate"}}"#;
        assert_eq!(
            http_error(reqwest::StatusCode::BAD_REQUEST, body, None),
            ProviderError::BadRequest {
                message: "messages: roles must alternate".into(),
            }
        );
    }

    fn minimal_request() -> Request {
        use cox_protocol::types::{Content, Effort, Job, Message, Role, Thinking, Tier};

        Request {
            tier: Tier::Code,
            job: Job::Main,
            model: cox_protocol::types::ModelId("claude-sonnet-5".into()),
            system: vec![],
            tools: vec![],
            messages: vec![Message {
                role: Role::User,
                content: vec![Content::Text {
                    text: "read a.rs".into(),
                }],
            }],
            effort: Effort::High,
            max_tokens: 1024,
            thinking: Thinking::Off,
            cache_breakpoints: vec![],
            stop_sequences: vec![],
        }
    }

    /// Contract test (plan.md §1.5): the wired-up `Provider::stream` client,
    /// driven against a `wiremock` server serving a real fixture byte-for-
    /// byte, must produce exactly what the pure state machine produces from
    /// the same fixture — and a `Usage` that carries the cache fields.
    #[tokio::test]
    async fn anthropic_stream_over_http() {
        let fixture = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/anthropic/one_tool_call.sse"),
        )
        .expect("fixture reads");

        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/messages"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_raw(fixture.clone(), "text/event-stream"),
            )
            .mount(&server)
            .await;

        let client = AnthropicProvider {
            base_url: server.uri(),
            api_key: "sk-test".into(),
            ttl: CacheTtl::FiveMinutes,
            fallbacks: false,
            http: reqwest::Client::new(),
        };

        let (tx, mut rx) = mpsc::channel(64);
        let got_usage = client
            .stream(minimal_request(), tx, CancellationToken::new())
            .await
            .expect("stream succeeds");

        let mut got_events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            got_events.push(event);
        }

        // Golden: the same fixture replayed straight through the pure state
        // machine (no network) must match exactly.
        let mut machine = stream::AnthropicStream::new();
        let mut want_events = Vec::new();
        for (event, data) in crate::sse::parse_sse_str(&fixture) {
            want_events.extend(
                machine
                    .feed(event.as_deref(), &data)
                    .expect("fixture parses"),
            );
        }

        // `ToolUseStart.id` is a freshly minted random `CallId` on every
        // `feed` call (see `stream.rs`'s module header), so the live and
        // golden runs mint different ones even for the same fixture;
        // normalize both before comparing structure.
        assert_eq!(
            stream::normalize_tool_ids(got_events),
            stream::normalize_tool_ids(want_events)
        );
        assert_eq!(got_usage.cache_read_tokens, 50);
        assert_eq!(got_usage.cache_write_tokens, 100);
        assert_eq!(got_usage.output_tokens, 24);
    }
}
