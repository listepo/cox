//! Subagent definitions from `.claude/agents/*.md` and `.cox/agents/*.md`
//! (T7.3): `name`, `description`, `tools`, `model`. A definition narrows
//! what the `agent` tool may hand a child; the tier mapping from Claude's
//! aliases lives here so every surface agrees on it.

use std::fs;
use std::path::{Path, PathBuf};

use cox_protocol::types::Tier;
use serde::Deserialize;

use crate::frontmatter;

#[derive(Debug, Clone, PartialEq)]
pub struct AgentDef {
    pub name: String,
    pub description: String,
    /// Tool names the child may use; empty means everything the parent has.
    pub tools: Vec<String>,
    pub model: Option<String>,
    pub path: PathBuf,
    /// The system prompt for the child.
    pub body: String,
}

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
    let mut found = Discovered::default();
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
    let (header, body): (Header, &str) = frontmatter::parse(&text).map_err(|e| e.to_string())?;
    let name = header.name.ok_or("missing `name`")?;
    let description = header.description.ok_or("missing `description`")?;
    Ok(AgentDef {
        name,
        description: description.trim().to_string(),
        tools: frontmatter::names(header.tools.as_ref()),
        model: header.model,
        path: path.to_path_buf(),
        body: body.trim().to_string(),
    })
}

/// How a `model:` value picks a tier: tier names, Claude's aliases
/// (`haiku` → cheap, `sonnet` → code, `opus` → think), a model id by its
/// family, `inherit`/absent → the parent's tier.
pub fn tier_for(model: Option<&str>) -> Option<Tier> {
    let m = model?.trim().to_ascii_lowercase();
    if m == "inherit" || m.is_empty() {
        return None;
    }
    let pick = |t| Some(t);
    match m.as_str() {
        "cheap" | "haiku" => pick(Tier::Cheap),
        "code" | "sonnet" => pick(Tier::Code),
        "think" | "opus" | "fable" => pick(Tier::Think),
        id if id.contains("haiku") => pick(Tier::Cheap),
        id if id.contains("opus") || id.contains("fable") => pick(Tier::Think),
        _ => pick(Tier::Code),
    }
}

impl AgentDef {
    /// The parent's tools the child may keep, in the parent's order. A
    /// listed tool the parent lacks is silently absent — a child can never
    /// gain a tool by naming it.
    pub fn restrict<T: Clone>(&self, tools: &[(String, T)]) -> Vec<T> {
        tools
            .iter()
            .filter(|(name, _)| self.tools.is_empty() || self.tools.contains(name))
            .map(|(_, t)| t.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_model_aliases_map_to_tiers() {
        assert_eq!(tier_for(Some("haiku")), Some(Tier::Cheap));
        assert_eq!(tier_for(Some("sonnet")), Some(Tier::Code));
        assert_eq!(tier_for(Some("opus")), Some(Tier::Think));
        assert_eq!(tier_for(Some("think")), Some(Tier::Think));
        assert_eq!(tier_for(Some("claude-haiku-4-5")), Some(Tier::Cheap));
        assert_eq!(tier_for(Some("gpt-5")), Some(Tier::Code));
        assert_eq!(tier_for(Some("inherit")), None);
        assert_eq!(tier_for(None), None);
    }
}
