//! `Workspace` — the acceptance-test DSL harness every story extends.
//!
//! Runs the real compiled `git-workspace` binary via `assert_cmd`, not
//! `git_workspace::run()` in-process — `std::process::ExitCode` can't be
//! inspected on stable Rust, so a real child process is the only way to get
//! at both a boolean success/failure and captured stdout/stderr from the
//! same call. See the decision log on story B for the tradeoff.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use assert_cmd::Command;

pub struct Workspace {
    dir: tempfile::TempDir,
    repos: RefCell<Vec<(String, String, Option<String>)>>,
    /// Fixture "remotes" live in a sibling tempdir, not under the
    /// workspace root — they stand in for repos hosted elsewhere, and
    /// living outside the workspace keeps `git status` at the workspace
    /// root free of harness-only noise.
    fixture_remotes_dir: tempfile::TempDir,
}

impl Workspace {
    pub fn new() -> Self {
        Workspace {
            dir: tempfile::tempdir().expect("create workspace tempdir"),
            repos: RefCell::new(Vec::new()),
            fixture_remotes_dir: tempfile::tempdir().expect("create fixture remotes tempdir"),
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

    /// Declare a repo in `workspace.toml`, writing/updating the real file
    /// on disk in the workspace's tempdir.
    pub fn declares_repo(&self, name: &str, remote: &str) -> &Self {
        self.repos
            .borrow_mut()
            .push((name.to_string(), remote.to_string(), None));
        self.write_manifest();
        self
    }

    /// Declare a repo with an explicit manifest `branch =` field — used to
    /// exercise the status command's branch-mismatch note.
    pub fn declares_repo_with_branch(&self, name: &str, remote: &str, branch: &str) -> &Self {
        self.repos
            .borrow_mut()
            .push((name.to_string(), remote.to_string(), Some(branch.to_string())));
        self.write_manifest();
        self
    }

    fn write_manifest(&self) {
        let mut content = String::new();
        for (name, remote, branch) in self.repos.borrow().iter() {
            content.push_str(&format!("[repos.{name}]\nremote = \"{remote}\"\n"));
            if let Some(branch) = branch {
                content.push_str(&format!("branch = \"{branch}\"\n"));
            }
            content.push('\n');
        }
        std::fs::write(self.root().join("workspace.toml"), content).expect("write manifest");
    }

    /// Run `git-workspace` with the given args from the workspace root.
    pub fn run(&self, args: &[&str]) -> RunResult {
        self.run_from(self.root(), args)
    }

    /// Run `git-workspace` with the given args from an explicit cwd (e.g.
    /// a nested subfolder).
    pub fn run_from(&self, cwd: &Path, args: &[&str]) -> RunResult {
        let output = Command::cargo_bin("git-workspace")
            .expect("locate built git-workspace binary")
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("run git-workspace");

        RunResult {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }

    /// Create a real local bare git repo with one commit, standing in for
    /// "the remote" — no mocks, real git. Returns the file path suitable
    /// for use as a `remote =` value in the manifest.
    pub fn fixture_remote_with_commit(&self, name: &str) -> PathBuf {
        let remotes_dir = self.fixture_remotes_dir.path().to_path_buf();
        let bare_path = remotes_dir.join(format!("{name}.git"));

        let bare_name = format!("{name}.git");
        run_git(&remotes_dir, &["init", "--bare", "--initial-branch=main", bare_name.as_str()]).expect("init bare fixture remote");

        let checkout_dir = remotes_dir.join(format!("{name}-seed"));
        run_git(
            &remotes_dir,
            &["clone", bare_path.to_str().unwrap(), checkout_dir.to_str().unwrap()],
        )
        .expect("clone fixture remote for seeding");
        self.commit_fixture_change(&checkout_dir, "seed commit");
        run_git(&checkout_dir, &["push", "origin", "main"]).expect("push seed commit");

        bare_path
    }

    /// Add a new commit to an existing fixture remote by cloning it into a
    /// scratch checkout, writing a file, committing, and pushing back.
    pub fn push_new_commit_to_fixture_remote(&self, bare_path: &Path) {
        let scratch = tempfile::tempdir().expect("create scratch checkout dir");
        run_git(
            scratch.path(),
            &["clone", bare_path.to_str().unwrap(), "."],
        )
        .expect("clone fixture remote for a new commit");
        self.commit_fixture_change(scratch.path(), "a later commit");
        run_git(scratch.path(), &["push", "origin", "main"]).expect("push new commit");
    }

    fn commit_fixture_change(&self, checkout_dir: &Path, message: &str) {
        std::fs::write(checkout_dir.join("file.txt"), message).expect("write fixture file");
        run_git(checkout_dir, &["add", "."]).expect("stage fixture file");
        run_git(
            checkout_dir,
            &["-c", "user.email=fixture@example.com", "-c", "user.name=fixture", "commit", "-m", message],
        )
        .expect("commit fixture file");
    }

    /// A handle onto `<workspace root>/<name>` for asserting real git state
    /// after a `sync`.
    pub fn repo(&self, name: &str) -> RepoHandle {
        RepoHandle {
            path: self.root().join(name),
        }
    }

    /// Turn the workspace root itself into a real git repo (so cloned
    /// child repos need a managed .gitignore to keep its status clean).
    pub fn init_as_git_repo(&self) -> &Self {
        run_git(self.root(), &["init", "--initial-branch=main"]).expect("init workspace root as git repo");
        run_git(self.root(), &["add", "workspace.toml"]).expect("stage manifest");
        run_git(
            self.root(),
            &[
                "-c", "user.email=fixture@example.com", "-c", "user.name=fixture",
                "commit", "-m", "initial commit",
            ],
        )
        .expect("commit manifest");
        self
    }

    /// Stage and commit everything currently in the workspace root's own
    /// git repo (e.g. a freshly written managed .gitignore).
    pub fn commit_all(&self, message: &str) -> &Self {
        run_git(self.root(), &["add", "-A"]).expect("stage all changes");
        run_git(
            self.root(),
            &[
                "-c", "user.email=fixture@example.com", "-c", "user.name=fixture",
                "commit", "-m", message,
            ],
        )
        .expect("commit changes");
        self
    }

    /// `git status --porcelain` from the workspace root, for asserting the
    /// managed gitignore keeps the root repo clean after a sync.
    pub fn git_status_porcelain(&self) -> String {
        let output = std::process::Command::new("git")
            .args(["-C"])
            .arg(self.root())
            .args(["status", "--porcelain"])
            .output()
            .expect("run git status");
        assert!(
            output.status.success(),
            "git status failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
}

/// Run a real `git` subprocess in `dir` — used only by the fixture-remote
/// builder above, not by the CLI under test.
fn run_git(dir: &Path, args: &[&str]) -> Result<(), String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("failed to run git {args:?}: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

pub struct RepoHandle {
    path: PathBuf,
}

impl RepoHandle {
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn current_branch(&self) -> String {
        let output = std::process::Command::new("git")
            .args(["-C"])
            .arg(&self.path)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .expect("run git rev-parse");
        assert!(output.status.success(), "git rev-parse failed: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    pub fn head_commit(&self) -> String {
        let output = std::process::Command::new("git")
            .args(["-C"])
            .arg(&self.path)
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("run git rev-parse HEAD");
        assert!(output.status.success(), "git rev-parse HEAD failed: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Write an untracked file into the repo, making `git status --porcelain`
    /// report it dirty — used to exercise the status command's dirty count.
    pub fn make_dirty(&self) -> &Self {
        std::fs::write(self.path.join("scratch.txt"), "uncommitted").expect("write scratch file");
        self
    }

    /// Commit a new file locally without pushing — puts the repo ahead of
    /// its upstream by one commit.
    pub fn commit_new_file(&self, message: &str) -> &Self {
        let file_name = format!("{}.txt", message.replace(' ', "_"));
        std::fs::write(self.path.join(&file_name), message).expect("write file for commit");
        run_git(&self.path, &["add", "."]).expect("stage file");
        run_git(
            &self.path,
            &[
                "-c", "user.email=fixture@example.com", "-c", "user.name=fixture",
                "commit", "-m", message,
            ],
        )
        .expect("commit file");
        self
    }

    /// Write an untracked file into the repo at the given relative path —
    /// used by exec tests to make a command's outcome depend on real
    /// per-repo state on disk.
    pub fn write_file(&self, relative_path: &str, contents: &str) -> &Self {
        std::fs::write(self.path.join(relative_path), contents).expect("write file into repo");
        self
    }

    /// Check out a new branch, diverging from the manifest's declared
    /// branch — used to exercise the status command's branch-mismatch note.
    pub fn checkout_new_branch(&self, name: &str) -> &Self {
        run_git(&self.path, &["checkout", "-b", name]).expect("checkout new branch");
        self
    }

    /// Check out an already-existing branch — used by checkout tests to
    /// move a repo back to a known branch before exercising the command
    /// under test.
    pub fn checkout_existing_branch(&self, name: &str) -> &Self {
        run_git(&self.path, &["checkout", name]).expect("checkout existing branch");
        self
    }
}

pub struct RunResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}
