//! Thin wrappers spawning the real system `git` binary. No branching or
//! business logic here — `domain::plan`/`domain::status` decide what to
//! do, this executes it and surfaces git's own output.

use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitError {
    pub message: String,
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for GitError {}

/// `git init <path>`.
pub fn init(path: &Path) -> Result<(), GitError> {
    run(Command::new("git").arg("init").arg(path))
}

/// `git clone <remote> <path>`.
pub fn clone(remote: &str, path: &Path) -> Result<(), GitError> {
    run(Command::new("git").args(["clone", remote]).arg(path))
}

/// `git -C <path> pull --ff-only`.
///
/// Fast-forward only: fails loudly on local divergence instead of
/// fabricating a merge commit in a repo the user didn't touch by hand
/// (logged decision, story C).
pub fn pull(path: &Path) -> Result<(), GitError> {
    run(Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["pull", "--ff-only"]))
}

/// `git -C <path> checkout <branch>`. Fails clearly (surfacing git's own
/// error text) if the branch doesn't exist locally — no auto-create.
pub fn checkout(path: &Path, branch: &str) -> Result<(), GitError> {
    run(Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["checkout", branch]))
}

/// `git -C <path> checkout -b <branch>`. Fails clearly (surfacing git's own
/// error text) if the branch already exists — checkout never silently
/// moves onto an existing branch when `--create` was asked for (logged
/// decision, story G).
pub fn create_branch(path: &Path, branch: &str) -> Result<(), GitError> {
    run(Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["checkout", "-b", branch]))
}

/// Whether `branch` exists as a local branch in the repo at `path`.
pub fn branch_exists(path: &Path, branch: &str) -> Result<bool, GitError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--verify", "--quiet"])
        .arg(format!("refs/heads/{branch}"))
        .output()
        .map_err(|e| GitError {
            message: format!("failed to run git: {e}"),
        })?;
    Ok(output.status.success())
}

/// `git -C <repo_path> worktree add <target_path> [<branch> | -b <branch>]`.
///
/// Checks out `branch` into the new worktree if it already exists in
/// `repo_path`, creates it fresh otherwise — same create-or-checkout
/// pattern as `checkout`/`create_branch` above, so a fleet-wide `worktree
/// add` behaves consistently whether a repo already has the branch or
/// not (logged decision, story H).
pub fn worktree_add(repo_path: &Path, target_path: &Path, branch: &str) -> Result<(), GitError> {
    let exists = branch_exists(repo_path, branch)?;
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo_path).args(["worktree", "add"]);
    if exists {
        cmd.arg(target_path).arg(branch);
    } else {
        cmd.args(["-b", branch]).arg(target_path);
    }
    run(&mut cmd)
}

/// `git -C <repo_path> worktree remove <target_path>`.
pub fn worktree_remove(repo_path: &Path, target_path: &Path) -> Result<(), GitError> {
    run(Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["worktree", "remove"])
        .arg(target_path))
}

/// Gather the raw per-repo state `domain::status::compute_status` needs:
/// current branch, dirty file count, and ahead/behind vs upstream.
///
/// Fetches quietly first so ahead/behind reflects the real remote, not a
/// stale remote-tracking ref from the last sync (logged decision, story D).
/// A repo with no upstream configured reports ahead/behind as 0/0 rather
/// than erroring — there's nothing to compare against.
pub fn gather_status(path: &Path) -> Result<crate::domain::status::RawRepoState, GitError> {
    fetch_quietly(path);

    let branch = current_branch(path)?;
    let dirty_count = porcelain_status(path)?.len();
    let (ahead, behind) = ahead_behind(path);

    Ok(crate::domain::status::RawRepoState {
        branch,
        dirty_count,
        ahead,
        behind,
    })
}

/// `git -C <path> fetch --quiet`. Best-effort: a repo whose remote is
/// unreachable still gets a status report, just with stale ahead/behind.
fn fetch_quietly(path: &Path) {
    Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["fetch", "--quiet"])
        .output()
        .ok();
}

/// `git -C <path> rev-parse --abbrev-ref HEAD`.
pub fn current_branch(path: &Path) -> Result<String, GitError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .map_err(|e| GitError {
            message: format!("failed to run git: {e}"),
        })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(GitError {
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

/// `git -C <path> status --porcelain`, split into non-empty lines — one
/// line per changed file.
fn porcelain_status(path: &Path) -> Result<Vec<String>, GitError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["status", "--porcelain"])
        .output()
        .map_err(|e| GitError {
            message: format!("failed to run git: {e}"),
        })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.to_string())
            .collect())
    } else {
        Err(GitError {
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

/// `git -C <path> rev-list --left-right --count @{u}...HEAD`, parsed into
/// (ahead, behind). No upstream configured — or any other failure — reports
/// (0, 0) rather than erroring; there's nothing meaningful to compare.
fn ahead_behind(path: &Path) -> (u32, u32) {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-list", "--left-right", "--count", "@{u}...HEAD"])
        .output();

    let Ok(output) = output else {
        return (0, 0);
    };
    if !output.status.success() {
        return (0, 0);
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut parts = text.split_whitespace();
    let behind = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let ahead = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (ahead, behind)
}

fn run(command: &mut Command) -> Result<(), GitError> {
    let output = command.output().map_err(|e| GitError {
        message: format!("failed to run git: {e}"),
    })?;

    if output.status.success() {
        Ok(())
    } else {
        Err(GitError {
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}
