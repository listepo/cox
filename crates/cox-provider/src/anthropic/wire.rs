//! Generated Rust types for the Anthropic Messages streaming SSE event
//! bodies (T30.10). `typify::import_types!` reads
//! `schema/anthropic-stream.json` (a curated JSON Schema subset, not a full
//! Anthropic SDK — see the schema's own `$comment` for the source and why
//! it stays partial) and emits one struct per definition. Nothing here is
//! hand-written but this header: the SSE → `ProviderEvent` state machine
//! that uses these types lives in [`super::stream`], unchanged in shape by
//! this task.

typify::import_types!(schema = "schema/anthropic-stream.json");

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    /// The schema's own `$comment` says extra fields must never break
    /// parsing; the corresponding assertion is
    /// `unknown_fields_on_event_block_and_usage_are_ignored` below.

    #[test]
    fn message_start_reads_model_and_usage() {
        let raw = r#"{"type":"message_start","message":{"id":"msg_1","model":"claude-sonnet-5",
            "usage":{"input_tokens":25,"output_tokens":1,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#;
        let event: MessageStartEvent =
            serde_json::from_str(raw).expect("well-formed message_start");
        assert_eq!(event.message.model.as_deref(), Some("claude-sonnet-5"));
        let usage = event.message.usage.expect("usage present");
        assert_eq!(usage.input_tokens, Some(25));
        assert_eq!(usage.output_tokens, Some(1));
        assert_eq!(usage.cache_creation_input_tokens, Some(0));
        assert_eq!(usage.cache_read_input_tokens, Some(0));
    }

    #[test]
    fn content_block_start_reads_text_block_type() {
        let raw =
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#;
        let event: ContentBlockStartEvent =
            serde_json::from_str(raw).expect("well-formed text block");
        assert_eq!(event.content_block.type_.as_deref(), Some("text"));
    }

    #[test]
    fn content_block_start_reads_thinking_block_type() {
        let raw = r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#;
        let event: ContentBlockStartEvent =
            serde_json::from_str(raw).expect("well-formed thinking block");
        assert_eq!(event.content_block.type_.as_deref(), Some("thinking"));
    }

    #[test]
    fn content_block_start_reads_tool_use_name() {
        let raw = r#"{"type":"content_block_start","index":0,
            "content_block":{"type":"tool_use","id":"toolu_1","name":"write","input":{}}}"#;
        let event: ContentBlockStartEvent =
            serde_json::from_str(raw).expect("well-formed tool_use block");
        assert_eq!(event.content_block.type_.as_deref(), Some("tool_use"));
        assert_eq!(event.content_block.name.as_deref(), Some("write"));
    }

    #[test]
    fn content_block_delta_reads_text_delta() {
        let raw =
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}"#;
        let event: ContentBlockDeltaEvent =
            serde_json::from_str(raw).expect("well-formed text_delta");
        assert_eq!(event.delta.type_.as_deref(), Some("text_delta"));
        assert_eq!(event.delta.text.as_deref(), Some("hi"));
    }

    #[test]
    fn content_block_delta_reads_input_json_delta() {
        let raw = r#"{"type":"content_block_delta","index":0,
            "delta":{"type":"input_json_delta","partial_json":"{\"a\":1}"}}"#;
        let event: ContentBlockDeltaEvent =
            serde_json::from_str(raw).expect("well-formed input_json_delta");
        assert_eq!(event.delta.partial_json.as_deref(), Some(r#"{"a":1}"#));
    }

    #[test]
    fn content_block_delta_reads_thinking_delta() {
        let raw = r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"hm"}}"#;
        let event: ContentBlockDeltaEvent =
            serde_json::from_str(raw).expect("well-formed thinking_delta");
        assert_eq!(event.delta.thinking.as_deref(), Some("hm"));
    }

    #[test]
    fn content_block_delta_reads_signature_delta_type() {
        // stream.rs consumes and drops the signature itself (module header:
        // no ProviderEvent carries it yet); the wire layer still needs to
        // deserialize the frame without error so dispatch can reach that
        // no-op arm instead of failing the whole call.
        let raw = r#"{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig"}}"#;
        let event: ContentBlockDeltaEvent =
            serde_json::from_str(raw).expect("well-formed signature_delta");
        assert_eq!(event.delta.type_.as_deref(), Some("signature_delta"));
    }

    #[test]
    fn message_delta_reads_stop_reason_and_usage() {
        let raw = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},
            "usage":{"output_tokens":8}}"#;
        let event: MessageDeltaEvent =
            serde_json::from_str(raw).expect("well-formed message_delta");
        assert_eq!(event.delta.stop_reason.as_deref(), Some("end_turn"));
        assert_eq!(event.usage.expect("usage present").output_tokens, Some(8));
    }

    #[test]
    fn message_delta_reads_refusal_stop_details() {
        let raw = r#"{"type":"message_delta","delta":{"stop_reason":"refusal",
            "stop_details":{"category":"cyber","explanation":"matched a policy classifier"}},"usage":{}}"#;
        let event: MessageDeltaEvent =
            serde_json::from_str(raw).expect("well-formed refusal message_delta");
        let details = event.delta.stop_details.expect("stop_details present");
        assert_eq!(details.category.as_deref(), Some("cyber"));
        assert_eq!(
            details.explanation.as_deref(),
            Some("matched a policy classifier")
        );
    }

    #[test]
    fn error_event_reads_type_and_message() {
        let raw = r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        let event: ErrorEvent = serde_json::from_str(raw).expect("well-formed error frame");
        let error = event.error.expect("error present");
        assert_eq!(error.type_.as_deref(), Some("overloaded_error"));
        assert_eq!(error.message.as_deref(), Some("Overloaded"));
    }

    #[test]
    fn unknown_fields_on_event_block_and_usage_are_ignored() {
        // Every field here that is not `model` or `usage.*` is a real field
        // from fixtures/anthropic/live_tool_use.sse (container, diagnostics,
        // service_tier, the nested cache_creation object) that this schema
        // deliberately does not model. None of them may set
        // `additionalProperties: false`, so none may cause a deserialize
        // failure here.
        let raw = r#"{
            "type":"message_start",
            "message":{
                "id":"msg_1",
                "model":"claude-sonnet-5",
                "container": null,
                "diagnostics": null,
                "usage":{
                    "input_tokens":10,
                    "output_tokens":2,
                    "service_tier":"standard",
                    "cache_creation":{"ephemeral_5m_input_tokens":0,"ephemeral_1h_input_tokens":0}
                }
            },
            "some_future_top_level_field": 42
        }"#;
        let event: MessageStartEvent =
            serde_json::from_str(raw).expect("unrecognised fields must never break parsing");
        assert_eq!(event.message.model.as_deref(), Some("claude-sonnet-5"));
        assert_eq!(
            event.message.usage.expect("usage present").input_tokens,
            Some(10)
        );
    }

    #[test]
    fn usage_missing_fields_deserialize_to_none() {
        let raw = r#"{"output_tokens":8}"#;
        let usage: Usage = serde_json::from_str(raw).expect("partial usage object");
        assert_eq!(usage.output_tokens, Some(8));
        assert_eq!(usage.input_tokens, None);
        assert_eq!(usage.cache_creation_input_tokens, None);
        assert_eq!(usage.cache_read_input_tokens, None);
    }

    #[test]
    fn usage_null_fields_deserialize_to_none() {
        // `Option<T>`'s own `Deserialize` impl treats an explicit JSON
        // `null` the same as an absent key regardless of `T` -- this schema
        // does not need `"type": ["integer", "null"]` to get that for free.
        let raw = r#"{"output_tokens":8,"cache_creation_input_tokens":null,"cache_read_input_tokens":null}"#;
        let usage: Usage = serde_json::from_str(raw).expect("explicit nulls parse");
        assert_eq!(usage.cache_creation_input_tokens, None);
        assert_eq!(usage.cache_read_input_tokens, None);
    }

    #[test]
    fn message_start_without_message_fails_to_deserialize() {
        // `message` is the one field `AnthropicStream::on_message_start`
        // (stream.rs) treats as fatal-if-missing (`ProviderError::Parse`);
        // this is the schema-level half of that contract. `id` is
        // deliberately NOT required on `ContentBlock` even though every
        // real tool_use block carries one: `ContentBlock` is shared by
        // text/thinking/tool_use, cox never reads the wire id (stream.rs
        // module header: it mints its own `CallId`), and requiring it here
        // would make every text/thinking block -- which never carries an
        // `id` -- fail to parse.
        let raw = r#"{"type":"message_start"}"#;
        let result: Result<MessageStartEvent, _> = serde_json::from_str(raw);
        assert!(result.is_err(), "message is required");
    }

    #[test]
    fn content_block_start_without_content_block_fails_to_deserialize() {
        // Mutation-checked: dropping `"required": ["content_block"]` from
        // the schema does not just fail this assertion, it stops the crate
        // from compiling at all -- `content_block` becomes
        // `Option<ContentBlock>` and every `.type_`/`.name` access on it,
        // here and in `stream.rs::on_block_start`, no longer type-checks.
        // That is a stronger guarantee than this test alone, which is why
        // it is worth keeping as the schema-level contract even though the
        // field is also load-bearing at the call site.
        let raw = r#"{"type":"content_block_start","index":0}"#;
        let result: Result<ContentBlockStartEvent, _> = serde_json::from_str(raw);
        assert!(result.is_err(), "content_block is required");
    }

    #[test]
    fn message_delta_without_delta_fails_to_deserialize() {
        let raw = r#"{"type":"message_delta","usage":{"output_tokens":1}}"#;
        let result: Result<MessageDeltaEvent, _> = serde_json::from_str(raw);
        assert!(result.is_err(), "delta is required");
    }

    #[test]
    fn unknown_content_block_and_delta_type_still_deserialize_at_the_wire_layer() {
        // Design choice (plan.md T30.10 step 3): `ContentBlock.type_` and
        // `Delta.type_` are modelled as an open `String`, not a closed
        // enum/oneOf, specifically so a block or delta kind cox does not
        // know about yet still deserializes *here* without error.
        // Ignoring it is `AnthropicStream::on_block_start` /
        // `on_block_delta`'s job (stream.rs `_ =>` arms), one layer up --
        // this test documents that the wire layer never rejects an unknown
        // `type` value, it only reports it.
        let block = r#"{"type":"content_block_start","index":0,
            "content_block":{"type":"some_future_block"}}"#;
        let event: ContentBlockStartEvent =
            serde_json::from_str(block).expect("an unknown block type still deserializes");
        assert_eq!(
            event.content_block.type_.as_deref(),
            Some("some_future_block")
        );

        let delta =
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"some_future_delta"}}"#;
        let event: ContentBlockDeltaEvent =
            serde_json::from_str(delta).expect("an unknown delta type still deserializes");
        assert_eq!(event.delta.type_.as_deref(), Some("some_future_delta"));
    }

    /// Guards the schema file itself against a typo'd `required` entry that
    /// names a property absent from that same definition's `properties` --
    /// cheap to check here, and the kind of mistake that would otherwise
    /// only surface as a confusing typify macro-expansion error.
    #[test]
    fn schema_required_fields_exist_in_properties() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("schema/anthropic-stream.json");
        let raw = fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path:?}: {e}"));
        let schema: serde_json::Value = serde_json::from_str(&raw).expect("schema is valid JSON");
        let definitions = schema
            .get("definitions")
            .and_then(serde_json::Value::as_object)
            .expect("schema has a top-level \"definitions\" object");
        for (name, def) in definitions {
            let Some(required) = def.get("required").and_then(serde_json::Value::as_array) else {
                continue;
            };
            let properties = def
                .get("properties")
                .and_then(serde_json::Value::as_object)
                .unwrap_or_else(|| panic!("{name} has \"required\" but no \"properties\""));
            for field in required {
                let field = field.as_str().expect("required entries are strings");
                assert!(
                    properties.contains_key(field),
                    "{name}.required lists {field:?}, which is not in its own properties"
                );
            }
        }
    }
}
