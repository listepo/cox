//! Pure translation of a provider-neutral [`Request`] into an Anthropic
//! Messages body. No I/O and no client state, so every rule that decides
//! cost — where `cache_control` lands, whether a thinking block is replayed,
//! which effort is asked for — is a snapshot test instead of a live call.
//!
//! **Breakpoint indexing.** `Request.cache_breakpoints` are indices into the
//! concatenation `system ++ messages`: `i < system.len()` names
//! `system[i]`, anything above names `messages[i - system.len()]`. A system
//! breakpoint marks that text block; a message breakpoint marks that
//! message's *last* content block, because Anthropic caches the prefix up
//! to and including the marked block. An index that is out of range, or
//! that names a `SystemBlock` with `cache == false`, is skipped rather than
//! being an error: context assembly (plan.md §1.9) owns the layout, and a
//! stale index must never fail a turn.

use cox_protocol::types::{Content, Effort, Message, ModelId, Request, Role, Thinking};
use serde_json::{Value, json};

use super::CacheTtl;

/// Anthropic accepts at most four `cache_control` breakpoints per request;
/// a fifth is a 400. cox plans for three (plan.md §1.9) and clamps here so
/// a caller's mistake costs a cache miss, not the turn.
pub const MAX_BREAKPOINTS: usize = 4;

/// Model families that take `thinking: {"type": "adaptive"}`. Older models
/// want `{"type": "enabled", "budget_tokens": N}`, which is a 400 on these —
/// cox never sends `budget_tokens`, so an unlisted model simply gets no
/// `thinking` field.
const ADAPTIVE_THINKING_PREFIXES: &[&str] = &[
    "claude-opus-5",
    "claude-sonnet-5",
    "claude-haiku-5",
    "claude-fable-5",
    "claude-mythos-5",
    "claude-opus-4-6",
    "claude-opus-4-7",
    "claude-opus-4-8",
    "claude-sonnet-4-6",
];

/// The provider-level knobs [`build_body`] needs that are not part of the
/// `Request` itself.
#[derive(Debug, Clone, Copy)]
pub struct BuildCfg<'a> {
    /// TTL written into every `cache_control` block.
    pub ttl: CacheTtl,
    /// Whether to send `fallbacks: "default"` (needs the matching beta header).
    pub fallbacks: bool,
    /// The model that produced the `Content::Thinking` blocks currently in
    /// history. `Content` carries no `produced_by` of its own, and adding
    /// one would change the rollout schema every crate shares, so the
    /// caller — which saw the `ModelSwitched` event — passes it here.
    /// `None`, or a value different from `Request.model`, drops the blocks.
    pub thinking_model: Option<&'a ModelId>,
}

/// Translates a `Request` into the JSON body for `POST /v1/messages`.
pub fn build_body(req: &Request, cfg: BuildCfg<'_>) -> Value {
    let mut system: Vec<Value> = req
        .system
        .iter()
        .map(|b| json!({"type": "text", "text": b.text}))
        .collect();

    let mut messages: Vec<Value> = req
        .messages
        .iter()
        .map(|m| {
            json!({
                "role": match m.role { Role::User => "user", Role::Assistant => "assistant" },
                "content": content_blocks(m, req, &cfg),
            })
        })
        .collect();

    place_breakpoints(req, cfg.ttl, &mut system, &mut messages);

    let mut body = json!({
        "model": req.model.0,
        "max_tokens": req.max_tokens,
        "messages": messages,
        "stream": true,
        "output_config": {"effort": effort(req.effort)},
    });
    let obj = body.as_object_mut().expect("json! built an object");

    if !system.is_empty() {
        obj.insert("system".into(), Value::Array(system));
    }
    if !req.tools.is_empty() {
        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                })
            })
            .collect();
        obj.insert("tools".into(), Value::Array(tools));
        // Never `any`/`tool`: forced tool use is a 400 on the Fable/Mythos
        // tier, and cox routes the same request shape to every model.
        obj.insert("tool_choice".into(), json!({"type": "auto"}));
    }
    if req.thinking == Thinking::Adaptive && supports_adaptive_thinking(&req.model) {
        obj.insert("thinking".into(), json!({"type": "adaptive"}));
    }
    if !req.stop_sequences.is_empty() {
        obj.insert("stop_sequences".into(), json!(req.stop_sequences));
    }
    if cfg.fallbacks {
        // The scalar form: Anthropic picks the substitute by refusal
        // category, so cox owes no migration when one is deprecated.
        obj.insert("fallbacks".into(), json!("default"));
    }
    body
}

/// Sets `cache_control` on the blocks named by `Request.cache_breakpoints`.
fn place_breakpoints(req: &Request, ttl: CacheTtl, system: &mut [Value], messages: &mut [Value]) {
    let cache_control = json!({"type": "ephemeral", "ttl": ttl.as_str()});
    let mut placed = 0;
    for &i in &req.cache_breakpoints {
        if placed == MAX_BREAKPOINTS {
            break;
        }
        let target = if i < req.system.len() {
            if !req.system[i].cache {
                continue;
            }
            system.get_mut(i)
        } else {
            messages
                .get_mut(i - req.system.len())
                .and_then(|m| m.get_mut("content"))
                .and_then(Value::as_array_mut)
                .and_then(|blocks| blocks.last_mut())
        };
        if let Some(block) = target.and_then(Value::as_object_mut) {
            block.insert("cache_control".into(), cache_control.clone());
            placed += 1;
        }
    }
}

/// One message's content blocks. Several `Content::ToolResult`s in the same
/// user message become several `tool_result` blocks in that one message,
/// which is how Anthropic wants a parallel tool batch answered.
fn content_blocks(m: &Message, req: &Request, cfg: &BuildCfg<'_>) -> Vec<Value> {
    m.content
        .iter()
        .filter_map(|c| match c {
            Content::Text { text } => Some(json!({"type": "text", "text": text})),
            Content::ToolUse { id, name, input } => Some(json!({
                "type": "tool_use",
                "id": id.to_string(),
                "name": name,
                "input": input,
            })),
            Content::ToolResult {
                call_id,
                content,
                is_error,
            } => Some(json!({
                "type": "tool_result",
                "tool_use_id": call_id.to_string(),
                "content": content,
                "is_error": is_error,
            })),
            Content::Image {
                media_type,
                data_b64,
            } => Some(json!({
                "type": "image",
                "source": {"type": "base64", "media_type": media_type, "data": data_b64},
            })),
            // Microcompaction: the model sees the summary and the id it can
            // pass to `expand`, never the archived bytes.
            Content::Pointer { archive, summary } => Some(json!({
                "type": "text",
                "text": format!("[archived: {summary}; expand {}]", archive.id),
            })),
            // A signature is bound to the model that produced it: replaying
            // one to a different model is at best ignored and at worst a
            // 400, so a block only survives a model switch by being dropped.
            Content::Thinking { text, signature } => match (signature, cfg.thinking_model) {
                (Some(sig), Some(produced_by)) if *produced_by == req.model => Some(json!({
                    "type": "thinking",
                    "thinking": text,
                    "signature": sig,
                })),
                _ => None,
            },
        })
        .collect()
}

fn effort(e: Effort) -> &'static str {
    match e {
        Effort::Low => "low",
        Effort::High => "high",
        Effort::Xhigh => "xhigh",
    }
}

fn supports_adaptive_thinking(model: &ModelId) -> bool {
    ADAPTIVE_THINKING_PREFIXES
        .iter()
        .any(|p| model.0.starts_with(p))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use cox_protocol::ids::{ArchiveId, CallId};
    use cox_protocol::types::{ArchiveRef, Concurrency, Job, Risk, SystemBlock, Tier, ToolSpec};

    use super::*;

    /// Fixed ids so the snapshots are byte-stable across runs.
    fn call(n: u8) -> CallId {
        CallId::from_str(&format!("01ARZ3NDEKTSV4RRFFQ69G5FA{n}")).expect("valid ulid")
    }

    fn cfg(thinking_model: Option<&ModelId>) -> BuildCfg<'_> {
        BuildCfg {
            ttl: CacheTtl::FiveMinutes,
            fallbacks: true,
            thinking_model,
        }
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
                SystemBlock {
                    text: "date: 2026-09-02".into(),
                    cache: false,
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
            thinking: Thinking::Adaptive,
            cache_breakpoints: vec![1],
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
    fn anthropic_request_plain_text() {
        let mut req = base("claude-sonnet-5");
        req.messages = vec![user_text("what does cox-provider own?")];

        insta::assert_json_snapshot!(build_body(&req, cfg(None)));
    }

    #[test]
    fn anthropic_request_parallel_tool_results() {
        let mut req = base("claude-sonnet-5");
        req.messages = vec![
            user_text("read both files"),
            Message {
                role: Role::Assistant,
                content: vec![
                    Content::Text {
                        text: "Reading both.".into(),
                    },
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
            // Both results ride in one user message, as Anthropic requires
            // for a parallel batch.
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
        // system[1] (end of the stable prefix), the assistant turn, and the
        // tool-result turn: three of the four slots.
        req.cache_breakpoints = vec![1, 4, 5];

        let body = build_body(&req, cfg(None));
        assert_eq!(count_cache_control(&body), 3);
        insta::assert_json_snapshot!(body);
    }

    #[test]
    fn anthropic_request_after_compaction() {
        // The turn is on opus after a `/model opus` switch; the thinking
        // block in history was produced by sonnet, so it is dropped.
        let mut req = base("claude-opus-5");
        req.messages = vec![
            Message {
                role: Role::User,
                content: vec![Content::Text {
                    text: "Summary of earlier work: refactored the parser.".into(),
                }],
            },
            Message {
                role: Role::Assistant,
                content: vec![
                    Content::Thinking {
                        text: "the parser is recursive descent".into(),
                        signature: Some("sig-from-sonnet".into()),
                    },
                    Content::Text {
                        text: "Continuing.".into(),
                    },
                ],
            },
            Message {
                role: Role::User,
                content: vec![
                    Content::Pointer {
                        archive: ArchiveRef {
                            id: ArchiveId::from_str("01ARZ3NDEKTSV4RRFFQ69G5FB0")
                                .expect("valid ulid"),
                            bytes: 91_000,
                        },
                        summary: "bash cargo test: 91000 bytes, exit 0".into(),
                    },
                    Content::Text {
                        text: "now fix the failing test".into(),
                    },
                ],
            },
        ];
        req.cache_breakpoints = vec![1, 4];

        let body = build_body(&req, cfg(Some(&ModelId("claude-sonnet-5".into()))));
        let dumped = serde_json::to_string(&body).expect("serializes");
        assert!(
            !dumped.contains("sig-from-sonnet"),
            "stale thinking replayed"
        );
        insta::assert_json_snapshot!(body);
    }

    #[test]
    fn thinking_replayed_only_on_same_model() {
        let mut req = base("claude-sonnet-5");
        req.messages = vec![Message {
            role: Role::Assistant,
            content: vec![Content::Thinking {
                text: "thought".into(),
                signature: Some("sig".into()),
            }],
        }];

        let same = ModelId("claude-sonnet-5".into());
        let body = build_body(&req, cfg(Some(&same)));
        assert_eq!(
            body["messages"][0]["content"][0]["type"], "thinking",
            "same model must replay the block verbatim"
        );
        assert_eq!(body["messages"][0]["content"][0]["signature"], "sig");

        let other = ModelId("claude-opus-5".into());
        let body = build_body(&req, cfg(Some(&other)));
        assert_eq!(
            body["messages"][0]["content"].as_array().map(Vec::len),
            Some(0),
            "a block from another model must be dropped"
        );

        // Unknown provenance is treated like a switch: never guess.
        let body = build_body(&req, cfg(None));
        assert_eq!(
            body["messages"][0]["content"].as_array().map(Vec::len),
            Some(0)
        );
    }

    #[test]
    fn breakpoints_never_exceed_four() {
        let mut req = base("claude-sonnet-5");
        req.messages = (0..8).map(|i| user_text(&format!("turn {i}"))).collect();
        // Six valid indices, one pointing at a non-cacheable system block
        // and one past the end: the body must still carry at most four.
        req.cache_breakpoints = vec![0, 1, 2, 3, 4, 5, 6, 7, 99];

        let body = build_body(&req, cfg(None));
        assert_eq!(count_cache_control(&body), MAX_BREAKPOINTS);
        // The volatile system block never gets one, whatever the caller asks.
        assert!(body["system"][2].get("cache_control").is_none());
    }

    fn count_cache_control(v: &Value) -> usize {
        match v {
            Value::Object(map) => {
                let here = usize::from(map.contains_key("cache_control"));
                here + map.values().map(count_cache_control).sum::<usize>()
            }
            Value::Array(items) => items.iter().map(count_cache_control).sum(),
            _ => 0,
        }
    }
}
