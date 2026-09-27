//! Subagent definitions from `.claude/agents/*.md` and `.cox/agents/*.md`
//! (T7.3): `name`, `description`, `tools`, `model`. A definition narrows
//! what the `agent` tool may hand a child.
//!
//! T34.1: `AgentDef` and `tier_for` moved to `cox_protocol::agent` (they
//! cross into `cox-core`, which may not depend on `cox-ext`'s filesystem
//! I/O) and are re-exported here at their old path, so this module keeps
//! owning only `discover` and its parsing — the actual filesystem read.

use std::fs;
use std::path::{Path, PathBuf};

pub use cox_protocol::agent::{AgentDef, tier_for};
use serde::Deserialize;

use crate::frontmatter;

/// The `explore` and `shell` presets shipped in the binary (T9.3): the same
/// names, tools and models as the core's `agent` presets, so `cox ext list`
/// shows them with no config files present.
const EXPLORE_MD: &str = include_str!("../agents/explore.md");
const SHELL_MD: &str = include_str!("../agents/shell.md");

#[derive(Debug, Default, PartialEq)]
pub struct Discovered {
    pub agents: Vec<AgentDef>,
    pub notices: Vec<String>,
}

#[derive(Deserialize)]
struct Header {
    name: Option<String>,
    description: Option<String>,
    tools: Option<serde_yaml::Value>,
    model: Option<String>,
    /// T34.10: `disabled: true` hides the def from the model without
    /// removing it from disk or from `cox ext list`.
    disabled: Option<bool>,
}

/// `~/.cox/agents`, `~/.claude/agents`, `.cox/agents`, `.claude/agents`.
pub fn agent_dirs(
    cox_home: Option<&Path>,
    claude_home: Option<&Path>,
    project: Option<&Path>,
) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(h) = cox_home {
        dirs.push(h.join("agents"));
    }
    if let Some(h) = claude_home {
        dirs.push(h.join("agents"));
    }
    if let Some(p) = project {
        dirs.push(p.join(".cox").join("agents"));
        dirs.push(p.join(".claude").join("agents"));
    }
    dirs
}

pub fn discover(dirs: &[PathBuf]) -> Discovered {
    // Embedded presets first (T9.3): a same-named file in any dir overrides
    // them through the retain+push below, so users can replace either.
    let mut found = Discovered::default();
    for (name, text) in [("explore", EXPLORE_MD), ("shell", SHELL_MD)] {
        match parse_agent_text(&PathBuf::from(format!("<embedded>/{name}.md")), text) {
            Ok(def) => found.agents.push(def),
            Err(reason) => found
                .notices
                .push(format!("embedded agent {name} skipped: {reason}")),
        }
    }
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "md") && p.is_file())
            .collect();
        paths.sort();
        for path in paths {
            match parse_agent(&path) {
                Ok(def) => {
                    found.agents.retain(|a| a.name != def.name);
                    found.agents.push(def);
                }
                Err(reason) => found
                    .notices
                    .push(format!("agent {} skipped: {reason}", path.display())),
            }
        }
    }
    found
}

fn parse_agent(path: &Path) -> Result<AgentDef, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse_agent_text(path, &text)
}

fn parse_agent_text(path: &Path, text: &str) -> Result<AgentDef, String> {
    let (header, body): (Header, &str) = frontmatter::parse(text).map_err(|e| e.to_string())?;
    let name = header.name.ok_or("missing `name`")?;
    let description = header.description.ok_or("missing `description`")?;
    Ok(AgentDef {
        name,
        description: description.trim().to_string(),
        tools: frontmatter::names(header.tools.as_ref()),
        model: header.model,
        path: path.to_path_buf(),
        body: body.trim().to_string(),
        disabled: header.disabled.unwrap_or(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_embedded_defaults_include_explore_and_shell() {
        // No dirs at all: only the two shipped presets come back, and a
        // same-named file would override them (retain+push order).
        let found = discover(&[]);
        assert!(found.notices.is_empty(), "{:?}", found.notices);
        let names: Vec<&str> = found.agents.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["explore", "shell"]);
        let explore = &found.agents[0];
        assert_eq!(explore.tools, ["read", "grep", "glob", "outline", "expand"]);
        assert_eq!(explore.model.as_deref(), Some("haiku"));
        assert!(!explore.body.is_empty());
        let shell = &found.agents[1];
        assert_eq!(shell.tools, ["bash", "web_fetch"]);
        assert_eq!(shell.model.as_deref(), Some("haiku"));
    }

    #[test]
    fn agents_disabled_frontmatter_field_is_parsed() {
        let def = parse_agent_text(
            &PathBuf::from("<test>/blocked.md"),
            "---\nname: blocked\ndescription: not for the model\ndisabled: true\n---\nbody",
        )
        .unwrap();
        assert!(def.disabled);
        let enabled = parse_agent_text(
            &PathBuf::from("<test>/scout.md"),
            "---\nname: scout\ndescription: looks around\n---\nbody",
        )
        .unwrap();
        assert!(!enabled.disabled);
    }
}
