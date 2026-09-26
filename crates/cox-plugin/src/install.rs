//! The on-disk layout of a user plugin (PL§1, §1b) and the only code that
//! changes it: staging a package into `versions/<digest12>/`, and moving
//! the `current`/`previous` pointers. `cox plugin install`, `update` and
//! `update --rollback` all go through here, so there is one staging path and
//! one swap. Separate from `discover`, which only reads this layout, and
//! free of prompts and printing, which stay with the CLI in `crates/cox`.
//!
//! ```text
//! <cox_home>/plugins/<id>/
//!   current            digest12 of the active version
//!   previous           digest12 of the one kept for `--rollback`
//!   versions/<digest12>/
//! ```

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use crate::discover::package_digest;

/// The pointer file naming the active version.
pub const CURRENT: &str = "current";
/// The pointer file naming the one version kept for `--rollback`.
pub const PREVIOUS: &str = "previous";

/// `<cox_home>/plugins/<id>`. `id` has passed `PluginManifest::validate`
/// (`^[a-z][a-z0-9-]{1,23}$`), so it cannot name a path outside that root.
pub fn plugin_dir(cox_home: &Path, id: &str) -> PathBuf {
    cox_home.join("plugins").join(id)
}

/// The directory name of a version: the first 12 hex digits of its digest.
pub fn short(digest: &str) -> &str {
    digest.get(..12).unwrap_or(digest)
}

/// Copies `src` into `<plugin_dir>/versions/<digest12>/` through a
/// `<digest12>.tmp` sibling that is fsynced and renamed into place (PL§1b
/// step 6), so a crash never leaves a half-written version under its final
/// name. The copy is digested again before the rename: the grant is decided
/// against `digest`, so bytes that changed after validation must never land
/// under it. An existing version directory is reused only if it still
/// digests to `digest`.
pub fn stage(src: &Path, plugin_dir: &Path, digest: &str) -> io::Result<PathBuf> {
    let versions = plugin_dir.join("versions");
    fs::create_dir_all(&versions)?;
    let digest12 = short(digest);
    let dest = versions.join(digest12);
    if dest.is_dir() {
        if package_digest(&dest)? == digest {
            return Ok(dest);
        }
        fs::remove_dir_all(&dest)?;
    }
    let tmp = versions.join(format!("{digest12}.tmp"));
    if tmp.exists() {
        fs::remove_dir_all(&tmp)?;
    }
    copy_synced(src, &tmp)?;
    if package_digest(&tmp)? != digest {
        fs::remove_dir_all(&tmp)?;
        return Err(io::Error::other(
            "the package changed while it was being copied; run the command again",
        ));
    }
    fs::rename(&tmp, &dest)?;
    sync_dir(&versions);
    Ok(dest)
}

/// Reads a pointer file (`CURRENT` or `PREVIOUS`); `None` when it is
/// missing or empty.
pub fn read_pointer(plugin_dir: &Path, name: &str) -> io::Result<Option<String>> {
    match fs::read_to_string(plugin_dir.join(name)) {
        Ok(text) => {
            let text = text.trim();
            Ok((!text.is_empty()).then(|| text.to_string()))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Makes the staged `digest12` current (PL§1b step 8). The version it
/// replaces becomes `previous`, and every other version is deleted, so at
/// most two stay on disk. Rolling back is this same call with the
/// `previous` digest: the two pointers trade places. The caller decides
/// whether the grant allows it; this function never asks.
pub fn activate(plugin_dir: &Path, digest12: &str) -> io::Result<()> {
    let old = read_pointer(plugin_dir, CURRENT)?;
    if old.as_deref() != Some(digest12) {
        if let Some(old) = &old {
            write_pointer(plugin_dir, PREVIOUS, old)?;
        }
        write_pointer(plugin_dir, CURRENT, digest12)?;
    }
    let previous = read_pointer(plugin_dir, PREVIOUS)?;
    prune(plugin_dir, digest12, previous.as_deref())
}

/// Writes a pointer via a temp file plus rename, atomic on one filesystem,
/// so a reader sees the old name or the new one, never a torn write.
fn write_pointer(plugin_dir: &Path, name: &str, digest12: &str) -> io::Result<()> {
    fs::create_dir_all(plugin_dir)?;
    let tmp = plugin_dir.join(format!("{name}.tmp"));
    fs::write(&tmp, digest12)?;
    File::open(&tmp)?.sync_all()?;
    fs::rename(&tmp, plugin_dir.join(name))?;
    sync_dir(plugin_dir);
    Ok(())
}

/// Deletes every entry under `versions/` except `current` and `previous`,
/// including an unapproved staged version or a leftover `.tmp`.
fn prune(plugin_dir: &Path, current: &str, previous: Option<&str>) -> io::Result<()> {
    let Ok(entries) = fs::read_dir(plugin_dir.join("versions")) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        if name == current || previous.is_some_and(|p| name == p) {
            continue;
        }
        // `file_type` does not follow symlinks, and neither does
        // `remove_dir_all`, so a link out of `versions/` loses only itself.
        if entry.file_type()?.is_dir() {
            fs::remove_dir_all(entry.path())?;
        } else {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

/// Copies regular files and directories, fsyncing each file. Symlinks are
/// skipped, the same files `package_digest` skips, so the copy digests the
/// same as the source.
fn copy_synced(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_synced(&entry.path(), &to)?;
        } else if ty.is_file() {
            fs::copy(entry.path(), &to)?;
            File::open(&to)?.sync_all()?;
        }
    }
    sync_dir(dst);
    Ok(())
}

/// Best effort: a directory fsync makes a rename durable on Unix; other
/// platforms cannot open a directory as a file, and there the rename alone
/// is what the OS offers.
fn sync_dir(dir: &Path) {
    if let Ok(f) = File::open(dir) {
        let _ = f.sync_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(dir: &Path, wasm: &str) -> String {
        fs::create_dir_all(dir.join("bin")).unwrap();
        fs::write(dir.join("plugin.toml"), "id = \"demo\"\n").unwrap();
        fs::write(dir.join("bin/plugin.wasm"), wasm).unwrap();
        package_digest(dir).unwrap()
    }

    #[test]
    fn staged_version_digests_the_same_and_leaves_no_tmp() {
        let src = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let digest = package(src.path(), "one");
        let plugin = plugin_dir(home.path(), "demo");

        let dest = stage(src.path(), &plugin, &digest).unwrap();

        assert_eq!(dest, plugin.join("versions").join(short(&digest)));
        assert_eq!(package_digest(&dest).unwrap(), digest);
        assert!(
            !plugin
                .join(format!("versions/{}.tmp", short(&digest)))
                .exists()
        );
    }

    #[test]
    fn stage_refuses_bytes_that_differ_from_the_validated_digest() {
        let src = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let validated = package(src.path(), "one");
        package(src.path(), "two");
        let plugin = plugin_dir(home.path(), "demo");

        assert!(stage(src.path(), &plugin, &validated).is_err());
        let versions: Vec<_> = fs::read_dir(plugin.join("versions")).unwrap().collect();
        assert!(
            versions.is_empty(),
            "nothing lands under the granted digest"
        );
    }

    #[test]
    fn activate_keeps_one_previous_and_rollback_swaps_the_pointers() {
        let home = tempfile::tempdir().unwrap();
        let plugin = plugin_dir(home.path(), "demo");
        let mut digests = Vec::new();
        for wasm in ["one", "two", "three"] {
            let src = tempfile::tempdir().unwrap();
            let digest = package(src.path(), wasm);
            stage(src.path(), &plugin, &digest).unwrap();
            activate(&plugin, short(&digest)).unwrap();
            digests.push(short(&digest).to_string());
        }
        let current = read_pointer(&plugin, CURRENT).unwrap();
        let previous = read_pointer(&plugin, PREVIOUS).unwrap();
        assert_eq!(current.as_deref(), Some(digests[2].as_str()));
        assert_eq!(previous.as_deref(), Some(digests[1].as_str()));
        assert!(!plugin.join("versions").join(&digests[0]).exists());

        activate(&plugin, &digests[1]).unwrap();

        assert_eq!(
            read_pointer(&plugin, CURRENT).unwrap().as_deref(),
            Some(digests[1].as_str())
        );
        assert_eq!(
            read_pointer(&plugin, PREVIOUS).unwrap().as_deref(),
            Some(digests[2].as_str())
        );
        assert!(plugin.join("versions").join(&digests[2]).is_dir());
    }
}
