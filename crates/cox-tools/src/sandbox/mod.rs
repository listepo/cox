//! The sandbox front door (plan.md D7): turns a shell command plus the
//! session's `SandboxPolicy` into the `Command` that confines it on this
//! host. Separate from `bash` so the tool only knows it runs *a* command,
//! and so the backends (`seatbelt` on macOS, `bwrap` or `landlock` on
//! Linux) share one policy-to-paths translation and `doctor` has one place
//! to ask. Seatbelt and bwrap wrap the argv; Landlock cannot, so it hooks
//! the child between fork and exec instead.

pub mod bwrap;
#[cfg(target_os = "linux")]
pub mod landlock;
pub mod seatbelt;

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use cox_protocol::{LinuxBackend, SandboxMode, SandboxPolicy};

/// Part of macOS since 10.5; not a PATH lookup because the sandbox must
/// not depend on what the user's shell resolves.
const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// What confines shell commands on this host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// macOS `sandbox-exec` profiles.
    Seatbelt,
    /// bubblewrap namespaces and bind mounts.
    Bwrap,
    /// Landlock rules plus a seccomp network filter, applied in the child.
    Landlock,
}

impl Backend {
    /// The name `doctor` and the status line print.
    pub fn name(self) -> &'static str {
        match self {
            Backend::Seatbelt => "seatbelt",
            Backend::Bwrap => "bwrap",
            Backend::Landlock => "landlock",
        }
    }
}

/// The backend that will confine commands here, or `None` when nothing
/// will (Windows, `linux_backend = none`, or a Linux host with neither
/// namespaces nor Landlock). The surface that builds the session turns
/// `None` into a security notice and forces `on-request`.
pub fn backend(linux: LinuxBackend) -> Option<Backend> {
    if cfg!(target_os = "macos") {
        return Path::new(SANDBOX_EXEC)
            .is_file()
            .then_some(Backend::Seatbelt);
    }
    if !cfg!(target_os = "linux") {
        return None;
    }
    match linux {
        LinuxBackend::None => None,
        LinuxBackend::Bwrap => bwrap_works().then_some(Backend::Bwrap),
        LinuxBackend::Landlock => landlock_works().then_some(Backend::Landlock),
        LinuxBackend::Auto => {
            if bwrap_works() {
                Some(Backend::Bwrap)
            } else if landlock_works() {
                Some(Backend::Landlock)
            } else {
                None
            }
        }
    }
}

/// The command that runs `command` under `policy`: `/bin/sh -c` wrapped by
/// the backend, or bare for `danger-full-access` and hosts without one.
pub fn command(policy: &SandboxPolicy, roots: &[PathBuf], command: &str) -> io::Result<Command> {
    let shell = ["/bin/sh".to_string(), "-c".to_string(), command.to_string()];
    let backend = (policy.mode != SandboxMode::DangerFullAccess)
        .then(|| backend(policy.linux_backend))
        .flatten();
    let scratch = scratch(policy.mode);
    let argv: Vec<String> = match backend {
        Some(Backend::Seatbelt) => {
            let profile = seatbelt::profile(policy, roots, &scratch);
            let mut argv = vec![
                SANDBOX_EXEC.to_string(),
                "-p".to_string(),
                profile,
                "--".to_string(),
            ];
            argv.extend(shell);
            argv
        }
        Some(Backend::Bwrap) => bwrap::argv(policy, roots, &scratch, &shell),
        Some(Backend::Landlock) | None => shell.to_vec(),
    };
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    #[cfg(target_os = "linux")]
    if backend == Some(Backend::Landlock) {
        use std::os::unix::process::CommandExt;
        let guard = landlock::prepare(policy, &writable(policy, roots, &scratch))?;
        // SAFETY: `apply` only issues syscalls on state prepared before the
        // fork; nothing in it allocates or takes a lock.
        unsafe { cmd.pre_exec(move || guard.apply()) };
    }
    Ok(cmd)
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

/// What the command may write: `scratch` in every mode, the workspace
/// roots and `[sandbox].writable` only in `workspace-write`.
fn writable(policy: &SandboxPolicy, roots: &[PathBuf], scratch: &[PathBuf]) -> Vec<PathBuf> {
    let mut paths = scratch.to_vec();
    if policy.mode == SandboxMode::WorkspaceWrite {
        paths.extend(roots.iter().chain(&policy.writable).cloned());
    }
    paths
}

/// Every root × `readonly_in_workspace`, the subpaths that stay read-only
/// inside a writable root.
fn readonly(policy: &SandboxPolicy, roots: &[PathBuf]) -> Vec<PathBuf> {
    roots
        .iter()
        .flat_map(|root| {
            policy
                .readonly_in_workspace
                .iter()
                .map(|sub| root.join(sub))
        })
        .collect()
}

/// Runs `bwrap` once with the namespaces the real argv uses and remembers
/// the answer for the process; see `bwrap::PROBE` for why PATH is not enough.
fn bwrap_works() -> bool {
    static PROBE: OnceLock<bool> = OnceLock::new();
    *PROBE.get_or_init(|| {
        Command::new("bwrap")
            .args(bwrap::PROBE)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
}

fn landlock_works() -> bool {
    #[cfg(target_os = "linux")]
    {
        landlock::supported()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
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
            linux_backend: Default::default(),
        }
    }

    fn argv(cmd: &Command) -> Vec<String> {
        std::iter::once(cmd.get_program())
            .chain(cmd.get_args())
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn sandbox_danger_full_access_runs_the_shell_bare() {
        let cmd = command(&policy(SandboxMode::DangerFullAccess), &[], "echo hi").expect("command");
        assert_eq!(argv(&cmd), ["/bin/sh", "-c", "echo hi"]);
    }

    #[test]
    fn sandbox_writable_is_scratch_plus_roots_only_in_workspace_write() {
        let roots = vec![PathBuf::from("/ws")];
        let scratch = vec![PathBuf::from("/scratch")];
        let rw = writable(&policy(SandboxMode::WorkspaceWrite), &roots, &scratch);
        assert_eq!(rw, [PathBuf::from("/scratch"), PathBuf::from("/ws")]);
        let ro = writable(&policy(SandboxMode::ReadOnly), &roots, &scratch);
        assert_eq!(ro, [PathBuf::from("/scratch")]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn sandbox_macos_backend_is_seatbelt_and_wraps_the_shell() {
        assert_eq!(backend(LinuxBackend::Auto), Some(Backend::Seatbelt));
        let cmd = command(&policy(SandboxMode::WorkspaceWrite), &[], "echo hi").expect("command");
        let argv = argv(&cmd);
        assert_eq!(argv[0], SANDBOX_EXEC);
        assert_eq!(argv[1], "-p");
        assert!(argv[2].starts_with("(version 1)"));
        assert_eq!(&argv[argv.len() - 3..], ["/bin/sh", "-c", "echo hi"]);
    }
}
