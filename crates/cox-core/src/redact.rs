//! Unconditional redaction of what leaves a session (T28.4): one pattern
//! table and one `scrub` helper behind every output boundary — rollout
//! lines, logs, headless output, exports. Separate module so the patterns
//! exist exactly once and what the model needs for the task never passes
//! through it (redacting model input is out of scope).

use std::borrow::Cow;

use cox_protocol::types::Event;

/// What a secret-shaped run becomes. The same marker `cox record --redact`
/// (T1.5) leaves in cassettes, so redacted bytes look identical everywhere.
pub const REDACTED: &str = "«redacted»";

/// Replaces secret-shaped runs in `text` — `sk-…` keys, `Bearer …` tokens,
/// AWS `AKIA…` key ids, GitHub `ghp_…` tokens and PEM blocks — and returns
/// the original borrow when nothing matched, so clean output is copied by
/// neither this function nor its callers.
pub fn scrub(text: &str) -> Cow<'_, str> {
    // Every shape starts with one of these; without one the scanner below
    // cannot change anything.
    if !["sk-", "Bearer ", "AKIA", "ghp_", "-----BEGIN "]
        .iter()
        .any(|marker| text.contains(marker))
    {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut changed = false;
    while !rest.is_empty() {
        match secret_span(rest) {
            Some(len) => {
                out.push_str(REDACTED);
                rest = &rest[len..];
                changed = true;
            }
            None => {
                let Some(ch) = rest.chars().next() else {
                    break;
                };
                out.push(ch);
                rest = &rest[ch.len_utf8()..];
            }
        }
    }
    if changed {
        Cow::Owned(out)
    } else {
        Cow::Borrowed(text)
    }
}

/// The copy of `ev` for output boundaries: its `TextDelta`,
/// `ToolCallOutput` and `ToolCallDone` text scrubbed. Everything the model
/// needs — user text, tool-call input, thinking — is left verbatim (out of
/// scope), and an unchanged event is returned by borrow.
pub fn scrub_event(ev: &Event) -> Cow<'_, Event> {
    match ev {
        Event::TextDelta { item, text } => match changed(text) {
            Some(text) => Cow::Owned(Event::TextDelta { item: *item, text }),
            None => Cow::Borrowed(ev),
        },
        Event::ToolCallOutput { call_id, delta } => match changed(delta) {
            Some(delta) => Cow::Owned(Event::ToolCallOutput {
                call_id: *call_id,
                delta,
            }),
            None => Cow::Borrowed(ev),
        },
        Event::ToolCallDone { call_id, result } => {
            let visible = changed(&result.visible);
            let unified = result.diff.as_ref().and_then(|d| changed(&d.unified));
            if visible.is_none() && unified.is_none() {
                return Cow::Borrowed(ev);
            }
            let mut result = result.clone();
            if let Some(visible) = visible {
                result.visible = visible;
            }
            if let (Some(unified), Some(diff)) = (unified, result.diff.as_mut()) {
                diff.unified = unified;
            }
            Cow::Owned(Event::ToolCallDone {
                call_id: *call_id,
                result,
            })
        }
        _ => Cow::Borrowed(ev),
    }
}

/// `Some(scrubbed)` only when `scrub` changed something.
fn changed(text: &str) -> Option<String> {
    let scrubbed = scrub(text);
    (scrubbed.as_ref() != text).then(|| scrubbed.into_owned())
}

/// The byte length of the secret-shaped run at the start of `s`, if any.
fn secret_span(s: &str) -> Option<usize> {
    if let Some(after) = s.strip_prefix("Bearer ") {
        // Any run to the next whitespace is a bearer token.
        let end = after.find(char::is_whitespace).unwrap_or(after.len());
        return Some("Bearer ".len() + end);
    }
    if let Some(after) = s.strip_prefix("sk-") {
        return prefixed(3, after, 8);
    }
    if let Some(after) = s.strip_prefix("AKIA") {
        return prefixed(4, after, 16);
    }
    if let Some(after) = s.strip_prefix("ghp_") {
        return prefixed(4, after, 8);
    }
    s.starts_with("-----BEGIN ").then(|| {
        // A PEM block runs to the end of its `-----END …-----` line; an
        // unterminated one means the key material was pasted without a tail.
        s.find("\n-----END ").map_or(s.len(), |at| {
            s[at + 1..].find('\n').map_or(s.len(), |nl| at + 1 + nl)
        })
    })
}

/// `prefix` bytes plus at least `min` alphanumeric bytes, or no match.
fn prefixed(prefix: usize, after: &str, min: usize) -> Option<usize> {
    let n = after.bytes().take_while(u8::is_ascii_alphanumeric).count();
    (n >= min).then_some(prefix + n)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use cox_protocol::errors::ToolError;
    use cox_protocol::ids::SessionId;
    use cox_protocol::traits::{Store as _, Tool, ToolCx};
    use cox_protocol::types::{Concurrency, Level, Risk, Submission, ToolOutput, ToolSpec};
    use cox_provider::scripted::Scripted;
    use serde_json::Value;

    use crate::{MemoryStore, Session};

    use super::*;

    #[test]
    fn redact_table() {
        // The block's trailing newline stays, like every other pattern's tail.
        let pem = "-----BEGIN RSA PRIVATE KEY-----\nMIIEow\n-----END RSA PRIVATE KEY-----";
        let cases: &[(&str, &str)] = &[
            ("sk-abc12345678", REDACTED),
            ("Bearer tokensecret", REDACTED),
            ("AKIA0123456789ABCDEF", REDACTED),
            ("ghp_0123456789abcdef0123", REDACTED),
            (pem, REDACTED),
            // T1.5 parity: both shapes in one line, non-ASCII preserved.
            (
                "key=sk-abcdefghijk Authorization: Bearer tokensecret",
                "key=«redacted» Authorization: «redacted»",
            ),
            ("café sk-abcdefghijk 日本語", "café «redacted» 日本語"),
            // Below the length floors, and ordinary text, stay verbatim.
            ("sk-shrt", "sk-shrt"),
            ("AKIA0123", "AKIA0123"),
            ("plain text 1234", "plain text 1234"),
        ];
        for (input, want) in cases {
            assert_eq!(scrub(input).as_ref(), *want, "{input:?}");
        }
        assert!(matches!(scrub("plain text 1234"), Cow::Borrowed(_)));
    }

    /// A tool whose *output* is secret-shaped: the scenario scripts the
    /// model, this scripts the tool side (a secret in `input` would be
    /// model input — out of scope — and legitimately stay verbatim).
    struct Leaky;

    #[async_trait::async_trait]
    impl Tool for Leaky {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "leak".into(),
                description: "returns a secret-shaped string".into(),
                input_schema: serde_json::json!({"type": "object"}),
                deferred: false,
                risk: Risk::ReadOnly,
                concurrency: Concurrency::Parallel,
            }
        }
        fn subject(&self, _input: &Value) -> String {
            String::new()
        }
        async fn call(&self, _input: Value, _cx: &ToolCx) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput {
                text: "key sk-abc12345678 end".into(),
                is_error: false,
                diff: None,
                structured: None,
            })
        }
    }

    #[tokio::test]
    async fn rollout_never_contains_key_patterns() {
        let mut config = cox_protocol::Config::default();
        config.core.workspace_roots = vec![PathBuf::from("/tmp/cox-turn")];
        let provider = Arc::new(
            Scripted::from_toml(
                include_str!("../tests/scenarios/secret_tool_output.toml"),
                "",
            )
            .expect("scenario"),
        );
        let store = Arc::new(MemoryStore::new());
        let session = Session::new(
            config,
            provider,
            vec![Arc::new(Leaky)],
            store.clone(),
            store.clone(),
            PathBuf::from("/tmp/cox-turn"),
        )
        .expect("session");
        session
            .submit(Submission::UserTurn {
                text: "run the tool".into(),
                attachments: vec![],
                confirm_think: false,
            })
            .await
            .expect("turn");
        let events = store.rollout_read(&SessionId::new()).expect("rollout");
        let rollout = serde_json::to_string(&events).expect("rollout json");
        assert!(
            !rollout.contains("sk-abc12345678"),
            "the rollout kept the secret-shaped tool output"
        );
        assert!(rollout.contains(REDACTED), "nothing was redacted at all");
        assert_eq!(
            scrub(&rollout).as_ref(),
            rollout.as_str(),
            "the rollout still contains a key pattern"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::Notice {
                    level: Level::Security,
                    text
                } if text.contains("redacted in the rollout")
            )),
            "no Notice(Security) after the redaction"
        );
        // The model's own input is out of scope: it still sees the original.
        let history = serde_json::to_string(&session.history().await).expect("history json");
        assert!(
            history.contains("sk-abc12345678"),
            "the model lost the tool output"
        );
    }
}
