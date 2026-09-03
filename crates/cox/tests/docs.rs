//! T12.3: `docs/config.md` documents every key in `config/default.toml`.

fn collect(item: &toml_edit::Item, prefix: String, out: &mut Vec<String>) {
    match item {
        toml_edit::Item::Table(table) => {
            for (key, child) in table.iter() {
                let dotted = if prefix.is_empty() {
                    key.into()
                } else {
                    format!("{prefix}.{key}")
                };
                collect(child, dotted, out);
            }
        }
        toml_edit::Item::Value(_) => out.push(prefix),
        _ => {}
    }
}

#[test]
fn docs_config_covers_every_key() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../");
    let toml = std::fs::read_to_string(root.join("config/default.toml")).expect("default.toml");
    let md = std::fs::read_to_string(root.join("docs/config.md")).expect("config.md");
    let doc: toml_edit::DocumentMut = toml.parse().expect("valid toml");
    let mut keys = Vec::new();
    collect(doc.as_item(), String::new(), &mut keys);
    assert!(!keys.is_empty(), "no keys parsed");
    // The reference groups by `## [section]` with short `` `key` `` bullets.
    let missing: Vec<_> = keys
        .iter()
        .filter(|k| {
            let (section, short) = k.rsplit_once('.').unwrap_or(("", k.as_str()));
            !md.contains(&format!("## `[{section}]`")) || !md.contains(&format!("`{short}`"))
        })
        .collect();
    assert!(missing.is_empty(), "undocumented keys: {missing:?}");
}
