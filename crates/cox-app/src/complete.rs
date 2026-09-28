//! Composer completion (DT§5.3): `/` offers the shared built-in table plus
//! the project's command files, `@` offers the files the `glob` tool would
//! find. The sources are the TUI's — `cox_protocol::commands::COMMANDS`,
//! `cox_ext::commands::discover`, `cox_search::glob::workspace_files` —
//! ranked by the same nucleo scorer, so both surfaces offer the same rows.

use std::collections::HashMap;
use std::path::Path;
use std::time::SystemTime;

use cox_protocol::commands::COMMANDS;
use cox_search::glob::{Candidate, rank_by_query, workspace_files};
use serde::{Deserialize, Serialize};

/// One completion row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Completion {
    /// What replaces the typed token (`/compact`, `@src/lib.rs`).
    pub insert: String,
    /// Usage or path, for the row's second line.
    pub detail: String,
}

/// A session's completion sources, loaded once when it opens.
#[derive(Debug, Clone, Default)]
pub struct Completer {
    commands: Vec<Completion>,
    files: Vec<String>,
}

impl Completer {
    /// Blocking (a directory walk and file reads): the caller runs it on
    /// the blocking pool. `home` is `COX_HOME`, `claude_home` `~/.claude`.
    /// A broken command file is skipped, never fatal.
    pub fn load(cwd: &Path, home: &Path, claude_home: &Path) -> Self {
        let root = cox_config::load::find_git_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
        let dirs = cox_ext::commands::command_dirs(Some(home), Some(claude_home), Some(&root));
        let mut commands: Vec<Completion> = COMMANDS
            .iter()
            .map(|(name, usage, _)| Completion {
                insert: format!("/{name}"),
                detail: (*usage).to_string(),
            })
            .collect();
        for c in cox_ext::commands::discover(&dirs).commands {
            if COMMANDS.iter().any(|(n, ..)| *n == c.name) {
                continue;
            }
            commands.push(Completion {
                insert: format!("/{}", c.name),
                detail: c.description.unwrap_or_default(),
            });
        }
        Self {
            commands,
            files: workspace_files(cwd),
        }
    }

    /// Appends `rows` after the built-ins and command files, skipping any
    /// already offered, so a plugin's command never shadows a row (T52.14).
    pub fn extend(&mut self, rows: impl IntoIterator<Item = Completion>) {
        for row in rows {
            if !self.commands.iter().any(|c| c.insert == row.insert) {
                self.commands.push(row);
            }
        }
    }

    /// Rows for the token being typed: `/que…` or `@que…`, best first,
    /// at most `limit`. Anything else completes nothing.
    pub fn complete(&self, token: &str, limit: usize) -> Vec<Completion> {
        let (sigil, query, rows) = if let Some(q) = token.strip_prefix('/') {
            ('/', q, self.commands.clone())
        } else if let Some(q) = token.strip_prefix('@') {
            let files = self.files.iter().map(|f| Completion {
                insert: format!("@{f}"),
                detail: f.clone(),
            });
            ('@', q, files.collect())
        } else {
            return Vec::new();
        };
        if query.is_empty() {
            return rows.into_iter().take(limit).collect();
        }
        let mut by_name: HashMap<String, Completion> = rows
            .into_iter()
            .map(|c| {
                (
                    c.insert
                        .strip_prefix(sigil)
                        .unwrap_or(&c.insert)
                        .to_string(),
                    c,
                )
            })
            .collect();
        let mut found: Vec<Candidate> = by_name
            .keys()
            .map(|name| Candidate {
                display: name.clone(),
                mtime: SystemTime::UNIX_EPOCH,
            })
            .collect();
        rank_by_query(&mut found, query);
        found
            .into_iter()
            .take(limit)
            .filter_map(|f| by_name.remove(&f.display))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_offers_builtins_and_command_files_and_at_offers_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cwd = dir.path().join("project");
        std::fs::create_dir_all(cwd.join("src")).expect("src");
        std::fs::write(cwd.join("src/lib.rs"), "").expect("lib");
        std::fs::create_dir_all(cwd.join(".cox/commands")).expect("commands");
        let deploy = "---\ndescription: ship it\n---\nDeploy $ARGUMENTS\n";
        std::fs::write(cwd.join(".cox/commands/deploy.md"), deploy).expect("deploy");
        let home = dir.path().join("cox-home");
        let c = Completer::load(&cwd, &home, &dir.path().join("claude"));

        let all = c.complete("/", 500);
        assert_eq!(
            all[0].insert,
            format!("/{}", COMMANDS[0].0),
            "built-ins first"
        );
        assert_eq!(all.len(), COMMANDS.len() + 1);
        let deploy = &c.complete("/depl", 3)[0];
        assert_eq!(
            (deploy.insert.as_str(), deploy.detail.as_str()),
            ("/deploy", "ship it")
        );
        assert_eq!(c.complete("/compact", 1)[0].insert, "/compact");
        assert_eq!(c.complete("@lib", 3)[0].insert, "@src/lib.rs");
        assert!(c.complete("plain", 3).is_empty());
    }
}
