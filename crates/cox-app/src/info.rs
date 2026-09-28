//! The inspector's Info tab (DT§5.1, T37.29.5): what a session is and where
//! it lives — its id, cwd, linked worktree, the config layers it runs with
//! and its rollout file. Built on request from what already answers each
//! part (`cox_tools::git::linked`, the Settings view's `cox-config`
//! provenance, `cox_store::Store::rollout_path`); separate so the tab's
//! shape is tested apart from the live session that gathers it.

use std::path::{Path, PathBuf};

use cox_protocol::ids::SessionId;
use cox_tools::git::Linked;
use serde::Serialize;

use crate::settings::{Layer, SettingsView};

/// What the Info tab lists.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Info {
    pub session: SessionId,
    pub cwd: PathBuf,
    /// `None` when the session runs outside a linked worktree.
    pub worktree: Option<Linked>,
    /// Each layer that set at least one key, in load order (`default`
    /// first, `flag` last).
    pub config: Vec<ConfigSource>,
    /// The session's JSONL rollout, where its events are appended.
    pub rollout: PathBuf,
}

/// One config layer the session's config came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigSource {
    pub layer: Layer,
    /// The file it was read from; `None` for a layer that is not a file.
    pub file: Option<PathBuf>,
    /// How many effective leaves it set.
    pub keys: u32,
}

/// The order `cox-config` layers them in, later over earlier.
const ORDER: [Layer; 6] = [
    Layer::Default,
    Layer::User,
    Layer::Project,
    Layer::ClaudeSettings,
    Layer::Env,
    Layer::Flag,
];

pub fn build(
    session: SessionId,
    cwd: &Path,
    worktree: Option<Linked>,
    settings: &SettingsView,
    rollout: PathBuf,
) -> Info {
    let config = ORDER
        .into_iter()
        .filter_map(|layer| {
            let n = settings
                .settings
                .iter()
                .filter(|s| s.layer == layer)
                .count();
            let file = match layer {
                Layer::User => Some(settings.user_file.clone()),
                Layer::Project => settings.project_file.clone(),
                _ => None,
            };
            (n > 0).then(|| ConfigSource {
                layer,
                file,
                keys: u32::try_from(n).unwrap_or(u32::MAX),
            })
        })
        .collect();
    Info {
        session,
        cwd: cwd.to_path_buf(),
        worktree,
        config,
        rollout,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn info_counts_each_layer_that_set_a_key_with_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let (home, repo) = (dir.path().join("home"), dir.path().join("repo"));
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(repo.join(".cox")).unwrap();
        fs::create_dir_all(&home).unwrap();
        let user = home.join("config.toml");
        fs::write(&user, "[budget]\nwarn_at = 0.5\n").unwrap();
        let project = repo.join(".cox").join("config.toml");
        fs::write(&project, "[tui]\nvim = true\n").unwrap();
        let settings = crate::settings::view(&user, &repo).unwrap();
        let id = SessionId::new();
        let rollout = home.join("sessions").join(format!("{id}.jsonl"));

        let info = build(id, &repo, None, &settings, rollout.clone());

        let layers: Vec<_> = info
            .config
            .iter()
            // A `COX_*` variable in the developer's shell adds an env layer.
            .filter(|c| c.layer != Layer::Env)
            .map(|c| (c.layer, c.file.clone()))
            .collect();
        let project = fs::canonicalize(&repo).unwrap().join(".cox/config.toml");
        assert_eq!(
            layers,
            [
                (Layer::Default, None),
                (Layer::User, Some(user)),
                (Layer::Project, Some(project)),
            ]
        );
        assert_eq!(info.config[1].keys, 1);
        assert_eq!(info.config[2].keys, 1);
        assert!(info.config[0].keys > 1);
        assert_eq!((info.session, info.cwd, info.rollout), (id, repo, rollout));
    }
}
