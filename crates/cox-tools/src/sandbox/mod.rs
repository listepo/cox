//! The sandbox front door (plan.md D7): turns a shell command plus the
//! session's `SandboxPolicy` into the argv that confines it on this host.
//! Separate from `bash` so the tool only knows it runs *an* argv, and so the
//! per-platform backends (`seatbelt` now, `bwrap`/`landlock` with T4.2) share
//! one policy-to-rules translation and `doctor` has one place to ask.

pub mod seatbelt;

use std::path::{Path, PathBuf};

use cox_protocol::{SandboxMode, SandboxPolicy};

/// Part of macOS since 10.5; not on PATH lookups because the sandbox must
/// not depend on what the user's shell resolves.
const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// The backend that will confine commands on this host, or `None` when
/// nothing will (T4.2 adds the Linux backends; Windows stays `None`).
pub fn backend() -> Option<&'static str> {
    if cfg!(target_os = "macos") && Path::new(SANDBOX_EXEC).is_file() {
        Some("seatbelt")
    } else if cfg!(target_os = "linux") && on_path("bwrap") {
        Some("bwrap")
    } else {
        None
    }
}

/// The argv that runs `command` under `policy`. `danger-full-access` and a
/// host without a backend run the shell bare; the caller decides whether
/// the latter is acceptable (T4.2 forces `on-request` then).
pub fn argv(policy: &SandboxPolicy, roots: &[PathBuf], command: &str) -> Vec<String> {
    let shell = || vec!["/bin/sh".to_string(), "-c".to_string(), command.to_string()];
    if policy.mode == SandboxMode::DangerFullAccess {
        return shell();
    }
    match backend() {
        Some("seatbelt") => {
            let profile = seatbelt::profile(policy, roots, &scratch(policy.mode));
            let mut argv = vec![
                SANDBOX_EXEC.to_string(),
                "-p".to_string(),
                profile,
                "--".to_string(),
            ];
            argv.extend(shell());
            argv
        }
        // bwrap/landlock arrive with T4.2; until then Linux runs bare.
        _ => shell(),
    }
}

/// Directories every command may write regardless of the workspace: the
/// temp dir always, plus the shared temp root and the user's cache in
/// `workspace-write` (cargo, pip and friends keep their caches there).
fn scratch(mode: SandboxMode) -> Vec<PathBuf> {
    let mut dirs = vec![std::env::temp_dir()];
    if mode == SandboxMode::WorkspaceWrite {
        dirs.push(PathBuf::from("/tmp"));
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(PathBuf::from(home).join(".cache"));
        }
    }
    dirs
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(mode: SandboxMode) -> SandboxPolicy {
        SandboxPolicy {
            mode,
            network: false,
            writable: vec![],
            readonly_in_workspace: vec![],
        }
    }

    #[test]
    fn sandbox_danger_full_access_runs_the_shell_bare() {
        let argv = argv(&policy(SandboxMode::DangerFullAccess), &[], "echo hi");
        assert_eq!(argv, ["/bin/sh", "-c", "echo hi"]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn sandbox_macos_backend_is_seatbelt_and_wraps_the_shell() {
        assert_eq!(backend(), Some("seatbelt"));
        let argv = argv(&policy(SandboxMode::WorkspaceWrite), &[], "echo hi");
        assert_eq!(argv[0], SANDBOX_EXEC);
        assert_eq!(&argv[argv.len() - 3..], ["/bin/sh", "-c", "echo hi"]);
        assert!(argv[2].starts_with("(version 1)"));
    }
}
