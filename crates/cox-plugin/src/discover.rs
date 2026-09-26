//! Plugin discovery and the package digest (PL§1). Finds user plugins under
//! `~/.cox/plugins/<id>/versions/<digest12>/` (following each `current`
//! file) and project plugins under `<git root>/.cox/plugins/<id>/`, parses
//! and validates each `plugin.toml` the way cox's own config loader parses
//! TOML (figment's `Toml` provider), and computes the digest. A user plugin
//! wins an id clash with a notice: a repository must not shadow code the
//! user installed (§1). The grant state itself is T33.6; this module only
//! answers `Loaded` or `Skipped`, never fatal (invariant 17). `load_manifest`
//! is `pub` so `cox plugin install` (T33.7) parses, validates and digests a
//! not-yet-placed package through the same path `discover` uses, rather
//! than computing a second digest of its own.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use cox_plugin_api::PluginManifest;
use figment::Figment;
use figment::providers::{Format, Toml};
use sha2::{Digest, Sha256};

/// Where a plugin's directory was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `~/.cox/plugins/<id>/versions/<digest12>/`, following `current`.
    User,
    /// `<git root>/.cox/plugins/<id>/`, read in place.
    Project,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Source::User => "user",
            Source::Project => "project",
        })
    }
}

/// One discovered plugin id, loaded or skipped with why.
#[derive(Debug, Clone)]
pub struct Plugin {
    /// The directory name it was found under.
    pub id: String,
    /// User or project.
    pub source: Source,
    /// The package directory (the version directory for a user plugin).
    pub dir: PathBuf,
    /// The outcome of parsing, validating and digesting it.
    pub state: State,
}

/// A discovered plugin's manifest state.
#[derive(Debug, Clone)]
pub enum State {
    /// The manifest parsed and validated; ready for a grant check (T33.6).
    /// Boxed: `PluginManifest` carries several `Vec`s, which would otherwise
    /// make every `State` as large as `Loaded` even when `Skipped`.
    Loaded {
        /// The parsed manifest.
        manifest: Box<PluginManifest>,
        /// SHA-256 over the package tree (`package_digest`).
        digest: String,
    },
    /// Missing, unreadable or invalid. Shown, never fatal.
    Skipped {
        /// Why, for `cox plugin list`.
        reason: String,
    },
}

/// One discovery pass: every plugin found, plus shadowing notices.
#[derive(Debug, Default)]
pub struct Discovered {
    /// Every plugin id found, sorted by id.
    pub plugins: Vec<Plugin>,
    /// Notices such as a project plugin shadowed by a user one.
    pub notices: Vec<String>,
}

/// Scans `<cox_home>/plugins` and, if `project_root` is given,
/// `<project_root>/.cox/plugins`. An id present in both keeps the user
/// plugin and drops the project one with a notice (§1).
pub fn discover(cox_home: &Path, project_root: Option<&Path>) -> Discovered {
    let mut out = Discovered::default();
    let user_root = cox_home.join("plugins");
    for id in scan_ids(&user_root) {
        let plugin_dir = user_root.join(&id);
        let dir = match current_version_dir(&plugin_dir) {
            Ok(dir) => dir,
            Err(reason) => {
                out.plugins.push(Plugin {
                    id,
                    source: Source::User,
                    dir: plugin_dir,
                    state: State::Skipped { reason },
                });
                continue;
            }
        };
        out.plugins
            .push(load_one(id, Source::User, &dir, &dir.join("plugin.toml")));
    }
    if let Some(root) = project_root {
        let project_root_dir = root.join(".cox").join("plugins");
        for id in scan_ids(&project_root_dir) {
            if out.plugins.iter().any(|p| p.id == id) {
                out.notices.push(format!(
                    "project plugin {id:?} is shadowed by the user plugin of the same id"
                ));
                continue;
            }
            let dir = project_root_dir.join(&id);
            let manifest_path = dir.join("plugin.toml");
            out.plugins
                .push(load_one(id, Source::Project, &dir, &manifest_path));
        }
    }
    out.plugins.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Directory names directly under `dir`; empty (never an error) when `dir`
/// itself does not exist, so an unused plugin root is not a notice.
fn scan_ids(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    ids.sort();
    ids
}

/// Follows a user plugin's `current` file to its version directory.
fn current_version_dir(plugin_dir: &Path) -> Result<PathBuf, String> {
    let current = plugin_dir.join("current");
    let digest12 = fs::read_to_string(&current)
        .map_err(|e| format!("cannot read {}: {e}", current.display()))?;
    let digest12 = digest12.trim();
    if digest12.is_empty() {
        return Err(format!("{} is empty", current.display()));
    }
    Ok(plugin_dir.join("versions").join(digest12))
}

fn load_one(id: String, source: Source, dir: &Path, manifest_path: &Path) -> Plugin {
    let state = load_manifest(dir, manifest_path, Some(&id))
        .map(|(manifest, digest)| State::Loaded {
            manifest: Box::new(manifest),
            digest,
        })
        .unwrap_or_else(|reason| State::Skipped { reason });
    Plugin {
        id,
        source,
        dir: dir.to_path_buf(),
        state,
    }
}

/// Parses and validates one `plugin.toml`, the way `config_load` parses
/// cox's own config (figment's `Toml` provider); this is the one module in
/// `cox-plugin` that owns loading and validation (PL§2, AGENTS "Config
/// files"). `expect_id` checks the manifest against a directory name for a
/// discovered plugin (`Some`); `cox plugin install <dir>` has no directory
/// name to check against yet — the id comes from the manifest itself — so
/// it passes `None` and reuses this same parse, validate and digest path
/// (T33.7: one digest computation, never a second one).
pub fn load_manifest(
    dir: &Path,
    manifest_path: &Path,
    expect_id: Option<&str>,
) -> Result<(PluginManifest, String), String> {
    let manifest: PluginManifest = Figment::from(Toml::file(manifest_path))
        .extract()
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    if let Some(id) = expect_id
        && manifest.id != id
    {
        return Err(format!(
            "plugin.toml id {:?} does not match directory {id:?}",
            manifest.id
        ));
    }
    manifest.validate().map_err(|e| e.to_string())?;
    // `wasm` is untrusted for a project plugin (D14): reject an absolute
    // path or a `..` component before anything reads it (T33.4's own loader
    // owns this, since `cox-plugin` cannot depend on `cox-sandbox::confine`,
    // per `crates/cox/tests/deps.rs`'s allowed-deps list for this crate).
    if !wasm_path_is_safe(&manifest.wasm) {
        return Err(format!(
            "wasm {:?} must be a path inside the package directory",
            manifest.wasm
        ));
    }
    let digest = package_digest(dir).map_err(|e| format!("cannot digest package: {e}"))?;
    Ok((manifest, digest))
}

fn wasm_path_is_safe(wasm: &str) -> bool {
    let path = Path::new(wasm);
    !path.is_absolute()
        && path
            .components()
            .all(|c| !matches!(c, std::path::Component::ParentDir))
}

/// SHA-256 over the whole package tree (PL§1): `(relative path, length,
/// bytes)` for every file under `dir`, sorted by path, `/`-joined so the
/// digest does not depend on the host's path separator.
pub fn package_digest(dir: &Path) -> std::io::Result<String> {
    let mut files = Vec::new();
    walk(dir, dir, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for rel in &files {
        let bytes = fs::read(dir.join(rel))?;
        hasher.update(rel.as_bytes());
        hasher.update([0]);
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(&bytes);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let ty = entry.file_type()?;
        if ty.is_dir() {
            walk(root, &path, out)?;
        } else if ty.is_file() {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str =
        "api = 1\nid = \"demo\"\nversion = \"0.1.0\"\nname = \"Demo\"\nwasm = \"plugin.wasm\"\n";

    fn write(dir: &Path, rel: &str, contents: &str) {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn digest_changes_when_any_file_changes() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "plugin.toml", MANIFEST);
        write(dir.path(), "nested/plugin.wasm", "one");
        let first = package_digest(dir.path()).unwrap();
        assert_eq!(
            first,
            package_digest(dir.path()).unwrap(),
            "stable for the same tree"
        );

        write(dir.path(), "nested/plugin.wasm", "two");
        let second = package_digest(dir.path()).unwrap();
        assert_ne!(first, second, "a changed byte must change the digest");
    }

    #[test]
    fn user_plugin_shadows_project_plugin_with_notice() {
        let home = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        let user_dir = home.path().join("plugins/demo");
        write(&user_dir, "current", "abc123456789");
        write(&user_dir, "versions/abc123456789/plugin.toml", MANIFEST);
        write(&user_dir, "versions/abc123456789/plugin.wasm", "user");
        write(
            &repo.path().join(".cox/plugins/demo"),
            "plugin.toml",
            MANIFEST,
        );

        let found = discover(home.path(), Some(repo.path()));

        assert_eq!(found.plugins.len(), 1, "{found:?}");
        assert_eq!(found.plugins[0].source, Source::User);
        assert!(
            found.notices.iter().any(|n| n.contains("demo")),
            "{:?}",
            found.notices
        );
    }

    #[test]
    fn malformed_manifest_is_listed_as_skipped() {
        let repo = tempfile::tempdir().unwrap();
        write(
            &repo.path().join(".cox/plugins/bad"),
            "plugin.toml",
            "id = \"bad\"\nnot_a_field = true\n",
        );

        let found = discover(repo.path(), Some(repo.path()));

        assert_eq!(found.plugins.len(), 1);
        assert!(matches!(&found.plugins[0].state, State::Skipped { reason } if !reason.is_empty()));
    }

    #[test]
    fn wasm_path_escaping_package_dir_is_skipped() {
        let repo = tempfile::tempdir().unwrap();
        let escaping = MANIFEST.replace("plugin.wasm", "../../etc/passwd");
        write(
            &repo.path().join(".cox/plugins/demo"),
            "plugin.toml",
            &escaping,
        );

        let found = discover(repo.path(), Some(repo.path()));

        assert_eq!(found.plugins.len(), 1);
        assert!(
            matches!(&found.plugins[0].state, State::Skipped { reason } if reason.contains("wasm"))
        );
    }
}
