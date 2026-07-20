//! Thin wrappers spawning the real system `git` binary. No branching or
//! business logic here — `domain::plan` decides what to do, this executes
//! it and surfaces git's own error output.

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
