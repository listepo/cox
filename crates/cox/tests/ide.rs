//! T11.2: the Zed `agent_servers` snippet in `docs/ide.md` is valid JSON
//! and points at `cox acp`.

/// First ```json fenced block in the doc.
fn snippet() -> String {
    let doc = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/ide.md"),
    )
    .expect("docs/ide.md");
    let start = doc.find("```json").expect("json fence") + "```json".len();
    let end = doc[start..].find("```").expect("fence end") + start;
    doc[start..end].to_string()
}

#[test]
fn ide_zed_snippet_is_valid_json_for_cox_acp() {
    let value: serde_json::Value = serde_json::from_str(&snippet()).expect("valid json");
    let cox = &value["agent_servers"]["cox"];
    assert_eq!(cox["type"], "custom");
    assert_eq!(cox["command"], "cox");
    assert_eq!(cox["args"], serde_json::json!(["acp"]));
}
