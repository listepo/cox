//! Shared HTTP plumbing for the network backends: credential lookup, auth
//! headers, and non-2xx → [`ProviderError`] mapping (plan.md §1.14).
//!
//! Anthropic and OpenAI-shaped servers phrase errors differently but map to
//! the same taxonomy, so the union lives here once instead of once per
//! backend. Every status a contract test asserts on maps exactly as before;
//! the only deliberate widening is that a 5xx is `Overloaded` (retryable) on
//! every backend — a 500 from Anthropic is transient per their own docs, and
//! treating it as a fatal `BadRequest` retried nothing.

use cox_protocol::errors::ProviderError;
use reqwest::header::HeaderValue;

/// `env_var` first, else the platform keyring entry `service/account` (the
/// native store per platform, not a mock). A missing or unreadable
/// credential is [`ProviderError::Auth`]; called on every provider
/// construction, including in `cox doctor`, so it never panics.
pub fn resolve_key_env_or_keyring(
    env_var: &str,
    service: &str,
    account: &str,
) -> Result<String, ProviderError> {
    match std::env::var(env_var) {
        Ok(k) if !k.trim().is_empty() => return Ok(k),
        _ => {}
    }
    keyring::Entry::new(service, account)
        .and_then(|e| e.get_password())
        .map_err(|_| ProviderError::Auth)
}

/// `Authorization: Bearer <key>`, marked sensitive. A key with non-ASCII
/// bytes is a misconfigured credential (`Auth`), not a transport failure.
pub fn bearer(key: &str) -> Result<HeaderValue, ProviderError> {
    let mut v = HeaderValue::from_str(&format!("Bearer {key}")).map_err(|_| ProviderError::Auth)?;
    v.set_sensitive(true);
    Ok(v)
}

/// `x-api-key: <key>`, marked sensitive. Same Auth-on-garbage rule as
/// [`bearer`].
pub fn api_key(key: &str) -> Result<HeaderValue, ProviderError> {
    let mut v = HeaderValue::from_str(key).map_err(|_| ProviderError::Auth)?;
    v.set_sensitive(true);
    Ok(v)
}

/// The `{"error": {"message": ...}}` envelope both API families use (Ollama
/// included); falls back to the raw body so a proxy's plain-text error is
/// not lost.
pub fn error_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error")?.get("message")?.as_str().map(str::to_string))
        .unwrap_or_else(|| body.to_string())
}

/// Best-effort `(got, max)` extraction from a too-long/context-exceeded
/// message. Union of both families' phrasings ("too long", "exceed" —
/// Ollama says "exceeds context length", Anthropic says "too long"); no
/// documented fixed schema exists, so anything else falls back to
/// `BadRequest` at the call site rather than guessing.
pub fn parse_context_too_long(message: &str) -> Option<(u32, u32)> {
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

/// Maps a non-2xx response to a [`ProviderError`]. `retry_after` comes from
/// the `retry-after` header, read before the body is consumed.
pub fn map_http_error(
    status: reqwest::StatusCode,
    body: &str,
    retry_after: Option<u64>,
) -> ProviderError {
    let message = error_message(body);
    match status.as_u16() {
        401 | 403 => ProviderError::Auth,
        429 => ProviderError::RateLimited { retry_after },
        500 | 502 | 503 | 504 | 529 => ProviderError::Overloaded,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_union_mapping_preserves_both_backends_cases() {
        // Anthropic's asserted cases.
        assert!(matches!(
            map_http_error(reqwest::StatusCode::UNAUTHORIZED, "{}", None),
            ProviderError::Auth
        ));
        assert!(matches!(
            map_http_error(reqwest::StatusCode::TOO_MANY_REQUESTS, "{}", Some(30)),
            ProviderError::RateLimited {
                retry_after: Some(30)
            }
        ));
        assert!(matches!(
            map_http_error(reqwest::StatusCode::SERVICE_UNAVAILABLE, "{}", None),
            ProviderError::Overloaded
        ));
        let body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"prompt is too long: 250000 tokens > 200000 maximum"}}"#;
        assert_eq!(
            map_http_error(reqwest::StatusCode::BAD_REQUEST, body, None),
            ProviderError::ContextTooLong {
                max: 200_000,
                got: 250_000,
            }
        );
        // Chat's asserted cases: 403 is auth, Ollama's "exceeds" phrasing parses.
        assert!(matches!(
            map_http_error(reqwest::StatusCode::FORBIDDEN, "{}", None),
            ProviderError::Auth
        ));
        let body = r#"{"error":{"message":"input length exceeds context length: 40000 tokens > 32768 maximum","type":"invalid_request_error"}}"#;
        assert_eq!(
            map_http_error(reqwest::StatusCode::BAD_REQUEST, body, None),
            ProviderError::ContextTooLong {
                max: 32_768,
                got: 40_000,
            }
        );
        // Plain-text proxy body survives.
        assert_eq!(
            map_http_error(reqwest::StatusCode::BAD_REQUEST, "nope", None),
            ProviderError::BadRequest {
                message: "nope".into()
            }
        );
    }

    #[test]
    fn http_bearer_and_api_key_reject_garbage_as_auth() {
        assert!(bearer("sk-test").is_ok());
        assert!(api_key("sk-test").is_ok());
        // A control byte can never be a credential: auth problem, not transport.
        assert!(matches!(bearer("a\nb"), Err(ProviderError::Auth)));
        assert!(matches!(api_key("a\nb"), Err(ProviderError::Auth)));
    }
}
