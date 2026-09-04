//! Git facts the surfaces show: the current branch, the working tree's
//! `+n −m` line counts, the worktree diff, and the branch names a composer
//! completes. Here rather than in `cox-tui` because §1.1 keeps process I/O
//! out of the TUI crate, and *not* a model-facing tool because `bash`
//! already runs git — this is what the user's screen needs, not the model's.
//!
//! Everything returned is someone else's text (a branch name, a diff body)
//! and reaches the terminal through `cox_tui::text::sanitize` like any other
//! untrusted string.

use std::path::Path;

use tokio::process::Command;

/// Branch and working-tree line counts, as the status line shows them.
/// Counts cover tracked changes against `HEAD` (staged and unstaged);
/// untracked files are not counted, because counting them means writing to
/// the index and the status line is a reader.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// `HEAD` when detached.
    pub branch: String,
    pub added: usize,
    pub removed: usize,
}

/// `None` whenever git cannot answer — not a repository, no `git` on `PATH`,
/// an unborn or broken `HEAD`. Fail open: a missing git costs a status
/// segment, never a session.
pub async fn status(dir: &Path) -> Option<Status> {
    let branch = git(dir, &["rev-parse", "--abbrev-ref", "HEAD"]).await?;
    let (added, removed) = numstat(&git(dir, &["diff", "--numstat", "HEAD"]).await?);
    Some(Status {
        branch: branch.trim().to_string(),
        added,
        removed,
    })
}

/// The worktree diff against `HEAD`, staged and unstaged, as unified text —
/// what `cox_tui::diff` renders in the Diff view.
pub async fn diff(dir: &Path) -> Option<String> {
    git(dir, &["diff", "HEAD"]).await
}

/// Local branch names, most recently committed first: completion candidates
/// for a `git checkout`/`switch`/`merge` line in the composer.
pub async fn branches(dir: &Path) -> Vec<String> {
    let out = git(
        dir,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname:short)",
            "refs/heads",
        ],
    )
    .await
    .unwrap_or_default();
    out.lines().map(str::to_string).collect()
}

/// Sums `git diff --numstat` columns. A binary file reports `-` for both,
/// which parses as nothing rather than aborting the count.
fn numstat(out: &str) -> (usize, usize) {
    out.lines().fold((0, 0), |(a, r), line| {
        let mut cols = line.split('\t');
        let parse = |c: Option<&str>| c.and_then(|c| c.parse::<usize>().ok()).unwrap_or(0);
        (a + parse(cols.next()), r + parse(cols.next()))
    })
}

/// One `git` run in `dir`; `None` on a non-zero exit or a missing binary.
/// `GIT_OPTIONAL_LOCKS=0` because the status line polls: it must never take
/// the index lock out from under the user's own git.
async fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .await
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn numstat_sums_columns_and_ignores_binary_dashes() {
        assert_eq!(
            numstat("3\t1\tsrc/a.rs\n10\t0\tsrc/b.rs\n-\t-\tlogo.png\n"),
            (13, 1)
        );
        assert_eq!(numstat(""), (0, 0));
    }

    /// A repository with one commit and one edited line, built with `-c`
    /// identity so the developer's global config never decides the result.
    async fn repo() -> Option<tempfile::TempDir> {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path();
        git(path, &["init", "--initial-branch=trunk"]).await?;
        fs::write(path.join("a.txt"), "one\ntwo\n").expect("write");
        git(path, &["add", "a.txt"]).await?;
        git(
            path,
            &[
                "-c",
                "user.email=t@example.invalid",
                "-c",
                "user.name=t",
                "commit",
                "-m",
                "first",
            ],
        )
        .await?;
        fs::write(path.join("a.txt"), "one\nTWO\nthree\n").expect("write");
        Some(dir)
    }

    #[tokio::test]
    async fn status_reports_branch_and_worktree_line_counts() {
        let Some(dir) = repo().await else {
            return; // no usable `git` here; the pure parser above still runs.
        };
        let status = status(dir.path()).await.expect("status");
        assert_eq!(status.branch, "trunk");
        assert_eq!((status.added, status.removed), (2, 1));
        assert_eq!(branches(dir.path()).await, vec!["trunk".to_string()]);
        assert!(diff(dir.path()).await.expect("diff").contains("+three"));
    }

    #[tokio::test]
    async fn status_is_none_outside_a_repository() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(status(dir.path()).await, None);
        assert!(branches(dir.path()).await.is_empty());
    }
}
