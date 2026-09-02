//! The `Replay` provider: serves recorded HTTP cassettes so contract tests
//! and evals run with no network and no key (D12). Separate from `Scripted`
//! because a cassette is a hashed wire capture, not a TOML scenario.
//!
//! Layout: `<dir>/<sha256>.request.json` + `<dir>/<sha256>.sse`. The hash is
//! sha256 of the canonical `Request` JSON with volatile keys (`date`, `cwd`,
//! `created_at`) masked. A miss names the hash and the nearest cassette.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use cox_protocol::errors::ProviderError;
use cox_protocol::traits::Provider;
use cox_protocol::types::{Caps, ProviderEvent, ProviderId, Request, Usage};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::anthropic::stream::AnthropicStream;
use crate::sse::parse_sse_str;

/// Replaces `sk-` keys and `Bearer ` prefixes so a cassette can be committed.
pub fn redact_secrets(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(stripped) = rest.strip_prefix("Bearer ") {
            out.push_str("«redacted»");
            let end = stripped
                .find(|c: char| c.is_ascii_whitespace())
                .unwrap_or(stripped.len());
            rest = &stripped[end..];
            continue;
        }
        if let Some(stripped) = rest.strip_prefix("sk-") {
            let n = stripped
                .bytes()
                .take_while(u8::is_ascii_alphanumeric)
                .count();
            if n >= 8 {
                out.push_str("«redacted»");
                rest = &stripped[n..];
                continue;
            }
        }
        let Some(ch) = rest.chars().next() else {
            break;
        };
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    out
}

/// Canonical cassette key for `req`: volatile fields masked, then sha256.
pub fn cassette_hash(req: &Request) -> Result<String, ProviderError> {
    let mut value = serde_json::to_value(req).map_err(|e| ProviderError::BadRequest {
        message: format!("canonical request: {e}"),
    })?;
    mask_volatile(&mut value);
    let bytes = serde_json::to_vec(&value).map_err(|e| ProviderError::BadRequest {
        message: format!("canonical request: {e}"),
    })?;
    Ok(hex_sha256(&bytes))
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn mask_volatile(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if matches!(k.as_str(), "date" | "cwd" | "created_at") {
                    *v = Value::String(String::new());
                } else {
                    mask_volatile(v);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(mask_volatile),
        _ => {}
    }
}

/// Writes `<hash>.request.json` and `<hash>.sse` under `dir`.
pub fn write_cassette(
    dir: &Path,
    req: &Request,
    sse: &str,
    redact: bool,
) -> Result<String, ProviderError> {
    std::fs::create_dir_all(dir).map_err(|_| ProviderError::Network)?;
    let hash = cassette_hash(req)?;
    let req_json = serde_json::to_string_pretty(req).map_err(|e| ProviderError::BadRequest {
        message: e.to_string(),
    })?;
    let sse = if redact {
        redact_secrets(sse)
    } else {
        sse.to_string()
    };
    let req_json = if redact {
        redact_secrets(&req_json)
    } else {
        req_json
    };
    std::fs::write(dir.join(format!("{hash}.request.json")), req_json)
        .map_err(|_| ProviderError::Network)?;
    std::fs::write(dir.join(format!("{hash}.sse")), sse).map_err(|_| ProviderError::Network)?;
    Ok(hash)
}

/// Replays cassettes from one directory.
pub struct Replay {
    dir: PathBuf,
}

impl Replay {
    /// `dir` is the cassette folder (`cassettes/<name>/` or `COX_CASSETTES`).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// `COX_PROVIDER=replay` plus `COX_CASSETTES=<dir>`.
    pub fn from_env() -> Result<Self, ProviderError> {
        let dir = std::env::var("COX_CASSETTES").map_err(|_| ProviderError::BadRequest {
            message: "COX_CASSETTES is required when COX_PROVIDER=replay".into(),
        })?;
        Ok(Self::new(dir))
    }
}

#[async_trait]
impl Provider for Replay {
    fn id(&self) -> ProviderId {
        ProviderId::Local
    }

    fn capabilities(&self) -> Caps {
        Caps {
            cache: false,
            thinking: true,
            server_tools: false,
            count_tokens: false,
            max_context: u32::MAX,
        }
    }

    async fn stream(
        &self,
        req: Request,
        sink: mpsc::Sender<ProviderEvent>,
        cancel: CancellationToken,
    ) -> Result<Usage, ProviderError> {
        let hash = cassette_hash(&req)?;
        let sse_path = self.dir.join(format!("{hash}.sse"));
        let sse = std::fs::read_to_string(&sse_path).map_err(|_| ProviderError::Unsupported {
            feature: format!("cassette miss: {hash}{}", nearest_hint(&self.dir, &hash)),
        })?;
        let mut machine = AnthropicStream::new();
        for (event, data) in parse_sse_str(&sse) {
            if cancel.is_cancelled() {
                return Err(ProviderError::Cancelled);
            }
            for ev in machine.feed(event.as_deref(), &data)? {
                if sink.send(ev).await.is_err() {
                    return Err(ProviderError::Cancelled);
                }
            }
        }
        Ok(machine.usage())
    }

    async fn count_tokens(&self, _req: &Request) -> Result<u32, ProviderError> {
        Err(ProviderError::Unsupported {
            feature: "replay count_tokens".into(),
        })
    }
}

fn nearest_hint(dir: &Path, hash: &str) -> String {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return String::new();
    };
    let mut best: Option<(usize, String)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(stem) = name.strip_suffix(".sse") else {
            continue;
        };
        let shared = stem
            .chars()
            .zip(hash.chars())
            .take_while(|(a, b)| a == b)
            .count();
        if best.as_ref().is_none_or(|(n, _)| shared > *n) {
            best = Some((shared, stem.to_string()));
        }
    }
    match best {
        Some((_, stem)) => format!("\nnearest: {stem}"),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cox_protocol::types::{Effort, Job, ModelId, Thinking, Tier};

    fn req() -> Request {
        Request {
            tier: Tier::Code,
            job: Job::Main,
            model: ModelId("claude-sonnet-5".into()),
            system: vec![],
            tools: vec![],
            messages: vec![],
            effort: Effort::High,
            max_tokens: 1024,
            thinking: Thinking::Off,
            cache_breakpoints: vec![],
            stop_sequences: vec![],
        }
    }

    #[test]
    fn redact_strips_sk_and_bearer() {
        let raw = "key=sk-abcdefghijk Authorization: Bearer tokensecret\n";
        let redacted = redact_secrets(raw);
        assert!(!redacted.contains("sk-abcdefghijk"));
        assert!(!redacted.contains("Bearer "));
        assert!(redacted.contains("«redacted»"));
    }

    #[test]
    fn redact_preserves_non_ascii() {
        let raw = "café sk-abcdefghijk 日本語";
        let redacted = redact_secrets(raw);
        assert!(redacted.contains("café"));
        assert!(redacted.contains("日本語"));
        assert!(!redacted.contains("sk-abcdefghijk"));
    }

    #[test]
    fn cassette_hash_is_stable_for_same_request() {
        assert_eq!(
            cassette_hash(&req()).expect("hash"),
            cassette_hash(&req()).expect("hash")
        );
    }

    #[tokio::test]
    async fn cassette_miss_names_hash() {
        let dir = std::env::temp_dir().join(format!("cox-replay-miss-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let provider = Replay::new(&dir);
        let (tx, _rx) = mpsc::channel(8);
        let err = provider
            .stream(req(), tx, CancellationToken::new())
            .await
            .expect_err("miss");
        let ProviderError::Unsupported { feature } = err else {
            panic!("expected miss, got {err:?}");
        };
        assert!(feature.contains("cassette miss:"), "{feature}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn replay_streams_anthropic_fixture() {
        let dir = std::env::temp_dir().join(format!("cox-replay-hit-{}", std::process::id()));
        let sse = include_str!("../../../fixtures/anthropic/text_only.sse");
        let hash = write_cassette(&dir, &req(), sse, false).expect("write");
        let provider = Replay::new(&dir);
        let (tx, mut rx) = mpsc::channel(64);
        let usage = provider
            .stream(req(), tx, CancellationToken::new())
            .await
            .expect("hit");
        assert!(
            std::fs::read_to_string(dir.join(format!("{hash}.sse")))
                .expect("sse")
                .contains("Hello")
        );
        let mut saw_hello = false;
        while let Ok(ev) = rx.try_recv() {
            if let ProviderEvent::TextDelta { text } = ev
                && text.contains("Hello")
            {
                saw_hello = true;
            }
        }
        assert!(saw_hello, "expected Hello delta");
        assert!(usage.output_tokens > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
