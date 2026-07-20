//! Spawn an arbitrary command inside a repo's real directory on disk. No
//! branching or business logic here — `lib.rs`'s `run_exec` decides which
//! repos to target and how to report the results, this just runs one
//! command in one directory and hands back what happened.

use std::path::Path;
use std::process::Command;

/// Alias, not a new type — see `shell::error` (Fowler: Duplicate Code).
pub type ExecError = super::error::ShellError;

/// The outcome of running a command to completion: its exit code and
/// captured stdout/stderr, kept separate rather than combined so callers
/// can decide how to interleave or label them (logged decision, story E).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOutcome {
    /// The process's real exit code. A process killed by a signal (no
    /// exit code on Unix) reports -1 rather than erroring — that's still
    /// a completed run with a result, not a failure to run at all.
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Run `argv` (program followed by its arguments — a real argv, never a
/// shell string) with its working directory set to `path`. Returns
/// `Err` only when the command couldn't be spawned at all (e.g. the
/// program doesn't exist) — a non-zero exit from a command that *did*
/// run is a normal `Ok(ExecOutcome)`, not an error, since the caller
/// needs to keep going and report it per-repo.
pub fn run(path: &Path, argv: &[String]) -> Result<ExecOutcome, ExecError> {
    let (program, args) = argv.split_first().ok_or_else(|| ExecError {
        message: "no command given".to_string(),
    })?;

    let output = Command::new(program)
        .args(args)
        .current_dir(path)
        .output()
        .map_err(|e| ExecError {
            message: format!("failed to run \"{program}\": {e}"),
        })?;

    Ok(ExecOutcome {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}
