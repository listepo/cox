//! The Anthropic Messages API backend: the client handle, its headers and
//! its credential lookup. The body translation lives next door in
//! [`request`] so it can be snapshot-tested with no key and no socket.
//!
//! Kept apart from the OpenAI backends because the fields that decide cost
//! here — `cache_control` placement, `output_config.effort`, adaptive
//! `thinking`, `fallbacks` — have no counterpart there (D3).

pub mod request;

use async_trait::async_trait;
use cox_protocol::errors::ProviderError;
use cox_protocol::traits::Provider;
use cox_protocol::types::{Caps, ProviderEvent, ProviderId, Request, Usage};
use reqwest::header::{HeaderMap, HeaderValue};
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
        _req: Request,
        _sink: mpsc::Sender<ProviderEvent>,
        _cancel: CancellationToken,
    ) -> Result<Usage, ProviderError> {
        Err(ProviderError::Unsupported {
            feature: "streaming lands in T1.2".into(),
        })
    }

    async fn count_tokens(&self, _req: &Request) -> Result<u32, ProviderError> {
        // `POST {base_url}/v1/messages/count_tokens` with the same body;
        // wired up alongside the streaming client in T1.2.
        Err(ProviderError::Unsupported {
            feature: "count_tokens lands in T1.2".into(),
        })
    }
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
}
