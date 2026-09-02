//! bubblewrap argv (plan.md T4.2 step 1): user and pid namespaces, `/`
//! bound read-only, the writable set bound read-write on top, the read-only
//! subpaths re-bound read-only over that, a private `/tmp`, and no network
//! namespace unless the policy allows one. Pure text like `seatbelt`, so it
//! is unit-tested on every platform; only running it needs Linux.

use std::path::{Path, PathBuf};

use cox_protocol::SandboxPolicy;

/// The namespaces and mounts every invocation starts with. `bwrap` sets
/// `PR_SET_NO_NEW_PRIVS` itself.
const BASE: &[&str] = &[
    "--unshare-user",
    "--unshare-pid",
    "--die-with-parent",
    "--ro-bind",
    "/",
    "/",
    "--tmpfs",
    "/tmp",
    "--proc",
    "/proc",
    "--dev",
    "/dev",
];

/// A run that proves the host lets us use the namespaces `BASE` needs: a
/// `bwrap` binary on PATH is not enough (Docker, hardened kernels and
/// Ubuntu's AppArmor all refuse unprivileged user namespaces).
pub const PROBE: &[&str] = &[
    "--unshare-user",
    "--unshare-pid",
    "--die-with-parent",
    "--ro-bind",
    "/",
    "/",
    "--proc",
    "/proc",
    "--dev",
    "/dev",
    "/bin/true",
];

/// The full argv, `bwrap` first, `shell` last. Bind sources must exist, so
/// missing ones are skipped; anything under `/tmp` is already covered by
/// the private tmpfs unless it is a workspace root, which is bound so the
/// command's cwd exists inside the sandbox.
pub fn argv(
    policy: &SandboxPolicy,
    roots: &[PathBuf],
    scratch: &[PathBuf],
    shell: &[String],
) -> Vec<String> {
    let mut argv: Vec<String> = std::iter::once("bwrap")
        .chain(BASE.iter().copied())
        .map(str::to_string)
        .collect();
    let writable = super::writable(policy, roots, scratch);
    for path in writable.iter().filter(|p| p.exists()) {
        let under_tmp = path.starts_with("/tmp") && !roots.contains(path);
        if !under_tmp {
            bind(&mut argv, "--bind", path);
        }
    }
    for path in super::readonly(policy, roots).iter().filter(|p| p.exists()) {
        bind(&mut argv, "--ro-bind", path);
    }
    if !policy.network {
        argv.push("--unshare-net".to_string());
    }
    argv.push("--".to_string());
    argv.extend(shell.iter().cloned());
    argv
}

fn bind(argv: &mut Vec<String>, flag: &str, path: &Path) {
    let path = path.to_string_lossy().into_owned();
    argv.push(flag.to_string());
    argv.push(path.clone());
    argv.push(path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use cox_protocol::SandboxMode;

    fn policy(mode: SandboxMode, network: bool) -> SandboxPolicy {
        SandboxPolicy {
            mode,
            network,
            writable: vec![],
            readonly_in_workspace: vec![PathBuf::from(".git")],
            linux_backend: Default::default(),
        }
    }

    fn shell() -> Vec<String> {
        vec!["/bin/sh".into(), "-c".into(), "echo hi".into()]
    }

    fn triple(flag: &str, path: &Path) -> String {
        let p = path.to_string_lossy();
        format!("{flag} {p} {p}")
    }

    #[test]
    fn bwrap_workspace_write_binds_the_root_and_rebinds_git_read_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(dir.path().join(".git")).expect("mkdir");
        let root = dir.path().to_path_buf();
        let argv = argv(
            &policy(SandboxMode::WorkspaceWrite, false),
            std::slice::from_ref(&root),
            &[],
            &shell(),
        );
        let joined = argv.join(" ");
        assert!(
            joined
                .starts_with("bwrap --unshare-user --unshare-pid --die-with-parent --ro-bind / /"),
            "{joined}"
        );
        assert!(joined.contains(&triple("--bind", &root)), "{joined}");
        assert!(
            joined.contains(&triple("--ro-bind", &root.join(".git"))),
            "{joined}"
        );
        assert!(joined.contains("--unshare-net"), "{joined}");
        assert!(joined.ends_with("-- /bin/sh -c echo hi"), "{joined}");
        let bind = joined.find(&triple("--bind", &root));
        let ro = joined.find(&triple("--ro-bind", &root.join(".git")));
        assert!(bind < ro, "the read-only bind must come last to win");
    }

    #[test]
    fn bwrap_read_only_binds_nothing_writable_and_network_flag_drops_unshare_net() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        let argv = argv(
            &policy(SandboxMode::ReadOnly, true),
            std::slice::from_ref(&root),
            &[],
            &shell(),
        );
        let joined = argv.join(" ");
        assert!(!joined.contains("--bind "), "{joined}");
        assert!(!joined.contains("--unshare-net"), "{joined}");
    }

    #[test]
    fn bwrap_skips_missing_sources_and_scratch_under_tmp() {
        let missing = PathBuf::from("/definitely/not/here");
        let argv = argv(
            &policy(SandboxMode::WorkspaceWrite, false),
            std::slice::from_ref(&missing),
            &[PathBuf::from("/tmp")],
            &shell(),
        );
        let joined = argv.join(" ");
        assert!(!joined.contains("/definitely/not/here"), "{joined}");
        assert!(!joined.contains("--bind /tmp /tmp"), "{joined}");
    }
}
