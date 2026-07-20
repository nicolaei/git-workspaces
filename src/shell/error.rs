//! Shared error type for shell-level operations.
//!
//! `git` and `exec` each spawn a child process and only ever get shown to
//! the user via `Display` — nothing in this crate pattern-matches on which
//! module raised the error. `GitError` and `ExecError` used to be two
//! structurally identical structs; one type now serves both (Fowler:
//! Duplicate Code).

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellError {
    pub message: String,
}

impl fmt::Display for ShellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ShellError {}
