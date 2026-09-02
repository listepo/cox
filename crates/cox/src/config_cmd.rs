//! `cox config show|get|set|path` (plan.md §1.6/§1.12): renders the layered
//! config `config_load::load` produces, and edits the user config file with
//! `toml_edit` so hand-written comments survive a `set`.

use std::fs;
use std::path::PathBuf;

use serde_json::Value as JsonValue;
use toml_edit::{DocumentMut, Item, Table, Value as TomlValue};

use crate::config_load::{self, LoadedConfig};

/// Flattens a serialized `Config` into `(dotted.key, leaf value)` pairs.
fn json_leaves(root: &JsonValue, prefix: &str, out: &mut Vec<(String, JsonValue)>) {
    match root {
        JsonValue::Object(map) => {
            for (k, v) in map {
                let dotted = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                json_leaves(v, &dotted, out);
            }
        }
        other => out.push((prefix.to_string(), other.clone())),
    }
}

/// Renders a leaf value the way TOML would write it (quoted strings), for
/// `cox config show`.
fn fmt_toml_value(v: &JsonValue) -> String {
    match v {
        JsonValue::String(s) => format!("{s:?}"),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Null => "\"\"".to_string(),
        JsonValue::Array(items) => {
            let rendered: Vec<String> = items.iter().map(fmt_toml_value).collect();
            format!("[{}]", rendered.join(", "))
        }
        // Only reachable for a hook/mcp-server entry with nested tables
        // (e.g. `[[hooks.PreToolUse]]`), which `default.toml` never has.
        JsonValue::Object(_) => serde_json::to_string(v).unwrap_or_default(),
    }
}

/// Renders a leaf value bare (unquoted strings), for `cox config get`.
fn fmt_plain_value(v: &JsonValue) -> String {
    match v {
        JsonValue::String(s) => s.clone(),
        other => fmt_toml_value(other),
    }
}

/// `cox config show [--sources]`.
pub fn show(loaded: &LoadedConfig, with_sources: bool) {
    let json = serde_json::to_value(&loaded.config).expect("Config always serializes");
    let mut leaves = Vec::new();
    json_leaves(&json, "", &mut leaves);
    leaves.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, value) in leaves {
        let rendered = fmt_toml_value(&value);
        if with_sources {
            println!("{key} = {rendered}  # {}", loaded.source_of(&key));
        } else {
            println!("{key} = {rendered}");
        }
    }
}

/// `cox config get <key>`. `None` if the key doesn't exist in the schema.
pub fn get(loaded: &LoadedConfig, key: &str) -> Option<String> {
    let json = serde_json::to_value(&loaded.config).ok()?;
    let mut cur = &json;
    for part in key.split('.') {
        cur = cur.get(part)?;
    }
    Some(fmt_plain_value(cur))
}

/// `cox config path`.
pub fn path() -> PathBuf {
    config_load::user_config_path()
}

/// Parses a `cox config set` value as TOML (`5`, `true`, `"text"`,
/// `[1, 2]`, ...), falling back to a bare string for input that isn't
/// valid TOML on its own (e.g. `cox config set tiers.code.model sonnet`
/// without quotes).
fn parse_value(raw: &str) -> TomlValue {
    raw.parse::<TomlValue>()
        .unwrap_or_else(|_| TomlValue::from(raw.to_string()))
}

/// `cox config set <key> <value>`: edits the user config file in place with
/// `toml_edit`, which preserves comments and formatting for everything it
/// doesn't touch; creates the file (and `~/.cox`) if absent. Returns the
/// path written.
pub fn set(key: &str, raw_value: &str) -> anyhow::Result<PathBuf> {
    let path = config_load::user_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let mut doc: DocumentMut = existing
        .parse()
        .map_err(|e| anyhow::anyhow!("{} is not valid TOML: {e}", path.display()))?;

    let parts: Vec<&str> = key.split('.').collect();
    let (last, ancestors) = parts
        .split_last()
        .ok_or_else(|| anyhow::anyhow!("empty key"))?;

    let mut table: &mut Table = doc.as_table_mut();
    for part in ancestors {
        table = table
            .entry(part)
            .or_insert(Item::Table(Table::new()))
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("`{part}` in `{key}` is not a table"))?;
    }
    // `Table::insert` resets the key's own formatting on overwrite, which
    // strips a comment sitting directly above an existing key. `entry(..)`
    // (via `IndexMut`) leaves the key's decor alone and only replaces the
    // value, so a leading `# comment` above `key = old` survives a `set`.
    *table.entry(last).or_insert(Item::None) = Item::Value(parse_value(raw_value));

    fs::write(&path, doc.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn config_set_preserves_comments() {
        let _guard = crate::config_load::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let home = tempdir().expect("tempdir");
        // Point `COX_HOME` at a scratch dir so `set` writes there, not the
        // developer's real `~/.cox`.
        unsafe { std::env::set_var("COX_HOME", home.path()) };

        let path = home.path().join("config.toml");
        fs::write(
            &path,
            "# my notes on this file\n[tiers.code]\n# pinned deliberately\nmodel = \"claude-sonnet-5\"\n",
        )
        .expect("seed user config");

        set("tiers.code.model", "claude-opus-5").expect("set succeeds");
        let after = fs::read_to_string(&path).expect("read back");

        unsafe { std::env::remove_var("COX_HOME") };

        assert!(after.contains("# my notes on this file"));
        assert!(after.contains("# pinned deliberately"));
        assert!(after.contains("model = \"claude-opus-5\""));
        assert!(!after.contains("claude-sonnet-5"));
    }

    #[test]
    fn config_set_creates_missing_file_and_parents() {
        let _guard = crate::config_load::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let home = tempdir().expect("tempdir");
        let nested = home.path().join("nested-home");
        unsafe { std::env::set_var("COX_HOME", &nested) };

        let path = set("budget.session_usd", "10").expect("set succeeds");

        unsafe { std::env::remove_var("COX_HOME") };

        assert_eq!(path, nested.join("config.toml"));
        let contents = fs::read_to_string(&path).expect("file created");
        assert!(contents.contains("session_usd = 10"));
    }
}
