//! `Workspace` — the acceptance-test DSL harness every story extends.
//!
//! Runs the real compiled `git-workspaces` binary via `assert_cmd`, not
//! `git_workspaces::run()` in-process — `std::process::ExitCode` can't be
//! inspected on stable Rust, so a real child process is the only way to get
//! at both a boolean success/failure and captured stdout/stderr from the
//! same call. See the decision log on story B for the tradeoff.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use assert_cmd::Command;

pub struct Workspace {
    dir: tempfile::TempDir,
    repos: RefCell<Vec<(String, String)>>,
}

impl Workspace {
    pub fn new() -> Self {
        Workspace {
            dir: tempfile::tempdir().expect("create workspace tempdir"),
            repos: RefCell::new(Vec::new()),
        }
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Create (if needed) a nested subfolder inside the workspace and
    /// return its path — for exercising upward manifest discovery.
    pub fn subfolder(&self, relative: &str) -> PathBuf {
        let path = self.root().join(relative);
        std::fs::create_dir_all(&path).expect("create nested subfolder");
        path
    }

    /// Declare a repo in `workspaces.toml`, writing/updating the real file
    /// on disk in the workspace's tempdir.
    pub fn declares_repo(&self, name: &str, remote: &str) -> &Self {
        self.repos
            .borrow_mut()
            .push((name.to_string(), remote.to_string()));
        self.write_manifest();
        self
    }

    fn write_manifest(&self) {
        let mut content = String::new();
        for (name, remote) in self.repos.borrow().iter() {
            content.push_str(&format!("[repos.{name}]\nremote = \"{remote}\"\n\n"));
        }
        std::fs::write(self.root().join("workspaces.toml"), content).expect("write manifest");
    }

    /// Run `git-workspaces` with the given args from the workspace root.
    pub fn run(&self, args: &[&str]) -> RunResult {
        self.run_from(self.root(), args)
    }

    /// Run `git-workspaces` with the given args from an explicit cwd (e.g.
    /// a nested subfolder).
    pub fn run_from(&self, cwd: &Path, args: &[&str]) -> RunResult {
        let output = Command::cargo_bin("git-workspaces")
            .expect("locate built git-workspaces binary")
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("run git-workspaces");

        RunResult {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

pub struct RunResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}
