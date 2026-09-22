//! Git facts the surfaces show: the current branch, the working tree's
//! `+n −m` line counts, the worktree diff, and the branch names a composer
//! completes. Here rather than in `cox-tui` because §1.1 keeps process I/O
//! out of the TUI crate, and *not* a model-facing tool because `bash`
//! already runs git — this is what the user's screen needs, not the model's.
//!
//! Everything returned is someone else's text (a branch name, a diff body)
//! and reaches the terminal through `cox_tui::text::sanitize` like any other
//! untrusted string.
//!
//! Worktrees (T27.3) live here too: `worktree_add`/`worktree_remove` follow
//! the workspace `worktrees` skill literally — one location, one name, one
//! owner in the lock reason, one worktree per task — and `GitWorktrees`
//! hands the same thing to the loop for `agent(isolation: "worktree")`.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use cox_protocol::errors::WorktreeError;
use cox_protocol::traits::{Worktree, Worktrees};
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

/// Lock reasons that let another cox session reuse or remove a worktree:
/// every cox owner is `cox / <session>`, so the prefix is the family.
pub const OWNER_PREFIX: &str = "cox /";

/// The worktree `name` of the repository around `from`, created when it
/// does not exist: `<root>/_worktrees/<repo>-<name>` on branch `<name>`,
/// cut from a freshly fetched `origin/<default>` (else `HEAD`) with no
/// upstream, locked with `"<owner> | <name> | <date>"`. `<root>` is the
/// nearest ancestor of the main checkout that already holds `_worktrees/`,
/// else the main checkout's parent; `WT_ROOT` overrides it. An existing
/// worktree is reused when its lock is empty or names a cox owner, and
/// refused when it names anyone else.
pub async fn worktree_add(from: &Path, name: &str, owner: &str) -> Result<Worktree, WorktreeError> {
    let name = name.to_ascii_lowercase();
    let ok = name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c))
        && name.starts_with(|c: char| c.is_ascii_alphanumeric());
    if !ok {
        return Err(WorktreeError::BadName { name });
    }
    let main = main_checkout(from).await?;
    let root = std::env::var_os("WT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| worktrees_root(&main));
    let repo = main
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".to_string());
    let path = root.join(format!("{repo}-{name}"));
    if let Some(record) = worktree_record(&main, &path).await? {
        match record.locked.as_deref() {
            None => {}
            Some(reason) if reason.starts_with(OWNER_PREFIX) => {}
            Some(reason) => {
                return Err(WorktreeError::LockedByOther {
                    path,
                    reason: reason.to_string(),
                });
            }
        }
        return Ok(Worktree {
            path,
            branch: record.branch.unwrap_or_else(|| name.clone()),
            main,
        });
    }
    let base = base_ref(&main).await;
    if let Some(remote) = base.strip_prefix("origin/") {
        // Best effort: offline, the last fetched tip is the start point.
        let _ = git(&main, &["fetch", "--quiet", "origin", remote]).await;
    }
    std::fs::create_dir_all(&root).map_err(|e| WorktreeError::Git {
        args: format!("worktree add {}", path.display()),
        stderr: e.to_string(),
    })?;
    let reason = format!("{owner} | {name} | {}", today());
    let path_s = path.display().to_string();
    let exists = git(
        &main,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{name}"),
        ],
    )
    .await
    .is_some();
    let mut args = vec!["worktree", "add", "--quiet", "--lock", "--reason", &reason];
    if exists {
        args.extend(["--no-track", &path_s, &name]);
    } else {
        args.extend(["--no-track", "-b", &name, &path_s, &base]);
    }
    git_or_err(&main, &args).await?;
    Ok(Worktree {
        path,
        branch: name,
        main,
    })
}

/// Unlocks and removes the worktree at `path`, keeping its branch (merging
/// is the user's action). Refuses the main checkout, a path git does not
/// list, a lock that does not start with `owner`, and a tree with
/// uncommitted or untracked files — never with `--force`.
pub async fn worktree_remove(path: &Path, owner: &str) -> Result<(), WorktreeError> {
    let main = main_checkout(path).await?;
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if path == main {
        return Err(WorktreeError::MainCheckout { path });
    }
    let record = worktree_record(&main, &path)
        .await?
        .ok_or_else(|| WorktreeError::NotRegistered { path: path.clone() })?;
    if let Some(reason) = record.locked.filter(|r| !r.starts_with(owner)) {
        return Err(WorktreeError::LockedByOther { path, reason });
    }
    if !is_clean(&path).await.unwrap_or(false) {
        return Err(WorktreeError::Dirty { path });
    }
    let path_s = path.display().to_string();
    let _ = git(&main, &["worktree", "unlock", &path_s]).await;
    git_or_err(&main, &["worktree", "remove", &path_s]).await?;
    Ok(())
}

/// `Some(true)` when `git status --porcelain` prints nothing; `None`
/// outside a repository.
pub async fn is_clean(dir: &Path) -> Option<bool> {
    git(dir, &["status", "--porcelain"])
        .await
        .map(|out| out.trim().is_empty())
}

/// The `Worktrees` implementation the surfaces install (T27.3).
pub struct GitWorktrees;

#[async_trait]
impl Worktrees for GitWorktrees {
    async fn add(&self, from: &Path, name: &str, owner: &str) -> Result<Worktree, WorktreeError> {
        worktree_add(from, name, owner).await
    }
}

/// The main checkout of the repository `dir` is in: the parent of the
/// common git dir, which is the same from every worktree.
async fn main_checkout(dir: &Path) -> Result<PathBuf, WorktreeError> {
    let common = git(
        dir,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .await
    .ok_or_else(|| WorktreeError::NotARepository {
        dir: dir.to_path_buf(),
    })?;
    let common = PathBuf::from(common.trim());
    let common = std::fs::canonicalize(&common).unwrap_or(common);
    common
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| WorktreeError::NotARepository {
            dir: dir.to_path_buf(),
        })
}

/// Nearest ancestor of `main` holding `_worktrees/`, else a new one next to
/// it — the skill's "one location" rule.
fn worktrees_root(main: &Path) -> PathBuf {
    let mut dir = main.parent();
    while let Some(d) = dir {
        if d.join("_worktrees").is_dir() {
            return d.join("_worktrees");
        }
        dir = d.parent();
    }
    main.parent().unwrap_or(main).join("_worktrees")
}

/// `origin/<default>` when the remote has a default branch, else `HEAD`,
/// so a repository without a remote still gets a worktree.
async fn base_ref(main: &Path) -> String {
    git(
        main,
        &["symbolic-ref", "--short", "-q", "refs/remotes/origin/HEAD"],
    )
    .await
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "HEAD".to_string())
}

/// One record of `git worktree list --porcelain`.
struct Record {
    branch: Option<String>,
    /// The lock reason; `Some("")` when locked without one.
    locked: Option<String>,
}

/// The record for `path`, matched on canonical paths because git prints
/// the path as it was given at `add` time.
async fn worktree_record(main: &Path, path: &Path) -> Result<Option<Record>, WorktreeError> {
    let out = git_or_err(main, &["worktree", "list", "--porcelain"]).await?;
    let want = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    for block in out.split("\n\n") {
        let mut lines = block.lines();
        let Some(head) = lines.next().and_then(|l| l.strip_prefix("worktree ")) else {
            continue;
        };
        let listed = PathBuf::from(head);
        let listed = std::fs::canonicalize(&listed).unwrap_or(listed);
        if listed != want {
            continue;
        }
        let mut record = Record {
            branch: None,
            locked: None,
        };
        for line in lines {
            if let Some(b) = line.strip_prefix("branch ") {
                record.branch = Some(b.strip_prefix("refs/heads/").unwrap_or(b).to_string());
            } else if let Some(r) = line.strip_prefix("locked") {
                record.locked = Some(r.trim().to_string());
            }
        }
        return Ok(Some(record));
    }
    Ok(None)
}

/// `YYYY-MM-DD` of today in UTC, for the lock reason; the civil-date
/// arithmetic is here because no date crate is in the workspace.
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    // Howard Hinnant's days-from-civil inverse (public domain).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// One `git` run whose failure matters: git's stderr, trimmed.
async fn git_or_err(dir: &Path, args: &[&str]) -> Result<String, WorktreeError> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .await
        .map_err(|e| WorktreeError::Git {
            args: args.join(" "),
            stderr: e.to_string(),
        })?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(WorktreeError::Git {
            args: args.join(" "),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        })
    }
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
        repo_in(dir.path()).await.map(|()| dir)
    }

    /// The same repository at `<tempdir>/repo`, so the `_worktrees/` the
    /// worktree tests create lands inside the tempdir, not beside it.
    async fn nested() -> Option<(tempfile::TempDir, PathBuf)> {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("repo");
        fs::create_dir(&path).expect("mkdir");
        repo_in(&path).await?;
        let path = fs::canonicalize(&path).expect("canon");
        Some((dir, path))
    }

    async fn repo_in(path: &Path) -> Option<()> {
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
        Some(())
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

    /// A worktree is created once under `<parent>/_worktrees/`, on its own
    /// lower-case branch, locked for its cox owner; asking again returns
    /// the same one, and another owner's lock is refused.
    #[tokio::test]
    async fn worktree_add_is_idempotent() {
        let Some((_dir, main)) = nested().await else {
            return;
        };
        let first = worktree_add(&main, "T42", "cox / s1").await.expect("add");
        let expected_root = main.parent().expect("parent").join("_worktrees");
        assert_eq!(first.path, expected_root.join("repo-t42"));
        assert_eq!(first.branch, "t42", "lower case, no cox/ prefix");
        assert_eq!(first.main, main);
        assert_eq!(
            status(&first.path).await.map(|s| s.branch),
            Some("t42".to_string())
        );
        let list = git(&main, &["worktree", "list", "--porcelain"])
            .await
            .expect("list");
        assert!(list.contains("locked cox / s1 | t42 | "), "{list}");

        let again = worktree_add(&first.path, "t42", "cox / s2")
            .await
            .expect("reuse");
        assert_eq!(again, first, "one worktree per task, any cox session");
        assert!(matches!(
            worktree_add(&main, "bad name", "cox / s1").await,
            Err(WorktreeError::BadName { .. })
        ));

        git(
            &main,
            &["worktree", "unlock", &first.path.display().to_string()],
        )
        .await;
        git(
            &main,
            &[
                "worktree",
                "lock",
                "--reason",
                "Cursor / grok | t42 | 2026-01-01",
                &first.path.display().to_string(),
            ],
        )
        .await;
        assert!(matches!(
            worktree_add(&main, "t42", "cox / s1").await,
            Err(WorktreeError::LockedByOther { .. })
        ));
    }

    /// Removal is refused while the tree is dirty and for another owner's
    /// lock; a clean tree under a cox owner goes, its branch stays.
    #[tokio::test]
    async fn worktree_remove_refuses_dirty() {
        let Some((_dir, main)) = nested().await else {
            return;
        };
        let wt = worktree_add(&main, "t7", "cox / s1").await.expect("add");
        fs::write(wt.path.join("new.txt"), "x").expect("write");
        assert!(matches!(
            worktree_remove(&wt.path, OWNER_PREFIX).await,
            Err(WorktreeError::Dirty { .. })
        ));
        fs::remove_file(wt.path.join("new.txt")).expect("rm");
        assert!(matches!(
            worktree_remove(&wt.path, "Cursor /").await,
            Err(WorktreeError::LockedByOther { .. })
        ));
        assert!(matches!(
            worktree_remove(&main, OWNER_PREFIX).await,
            Err(WorktreeError::MainCheckout { .. })
        ));
        worktree_remove(&wt.path, OWNER_PREFIX)
            .await
            .expect("remove");
        assert!(!wt.path.exists());
        assert!(
            branches(&main).await.contains(&"t7".to_string()),
            "the branch is kept for the user to merge"
        );
        assert!(matches!(
            worktree_remove(&wt.path, OWNER_PREFIX).await,
            Err(WorktreeError::NotRegistered { .. } | WorktreeError::NotARepository { .. })
        ));
    }

    #[test]
    fn today_is_an_iso_date() {
        let d = today();
        assert_eq!(d.len(), 10, "{d}");
        assert!(d.starts_with("20"), "{d}");
    }

    #[tokio::test]
    async fn status_is_none_outside_a_repository() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(status(dir.path()).await, None);
        assert!(branches(dir.path()).await.is_empty());
    }
}
