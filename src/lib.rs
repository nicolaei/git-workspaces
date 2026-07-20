use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod domain;
mod shell;

/// `git workspaces` — manifest-driven multi-repo git plugin.
#[derive(Parser, Debug)]
#[command(name = "git-workspaces", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// List the repos declared in the workspace manifest.
    List,
    /// Clone anything in the manifest missing on disk, pull the rest.
    /// Whole workspace by default; pass repo names to narrow.
    Sync {
        /// Repo names to narrow to. Omit to sync the whole workspace.
        repos: Vec<String>,
    },
    /// Report live per-repo state: branch, dirty/clean, ahead/behind
    /// upstream, and whether the checked-out branch matches the manifest.
    /// Whole workspace by default; pass repo names to narrow.
    Status {
        /// Repo names to narrow to. Omit to report on the whole workspace.
        repos: Vec<String>,
    },
    /// Run an arbitrary command inside each selected repo's directory.
    /// Sequential by default; a non-zero exit in one repo is collected and
    /// reported at the end, never aborts the others. Whole workspace by
    /// default; pass repo names before `--` to narrow.
    Exec {
        /// Repo names to narrow to. Omit to run in the whole workspace.
        repos: Vec<String>,
        /// Run repos concurrently instead of one after another.
        #[arg(long)]
        parallel: bool,
        /// The command and its arguments, given after `--`.
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    /// Clone `<remote>` to `<path>` (relative to the workspace root) and
    /// register it in `workspaces.toml`. The manifest key doubles as the
    /// on-disk path, same convention `list`/`sync` rely on.
    Add {
        /// Where to clone the repo, relative to the workspace root. Also
        /// becomes its manifest key.
        path: String,
        /// The remote to clone.
        remote: String,
        /// Record an explicit branch in the manifest entry.
        #[arg(long)]
        branch: Option<String>,
    },
    /// Checkout (creating if `--create`) the same branch name across
    /// selected repos. Whole workspace by default; pass repo names to
    /// narrow. Without `--create`, fails clearly if the branch doesn't
    /// exist in a repo. With `--create`, fails clearly if it already does.
    Checkout {
        /// The branch name to check out (or create).
        branch: String,
        /// Create the branch instead of checking out an existing one.
        #[arg(long)]
        create: bool,
        /// Repo names to narrow to. Omit to act on the whole workspace.
        repos: Vec<String>,
    },
}

/// The one true entrypoint. `main.rs` is a thin wrapper around this.
///
/// Takes an explicit argument iterator, cwd, and output sink so it can be
/// exercised in-process by tests without touching real env args or stdout.
pub fn run(args: impl Iterator<Item = String>, cwd: &Path, out: &mut impl Write) -> ExitCode {
    match Cli::try_parse_from(args) {
        Ok(cli) => match cli.command {
            None => ExitCode::SUCCESS,
            Some(Command::List) => run_list(cwd, out),
            Some(Command::Sync { repos }) => run_sync(cwd, &repos, out),
            Some(Command::Status { repos }) => run_status(cwd, &repos, out),
            Some(Command::Exec { repos, parallel, command }) => run_exec(cwd, &repos, &command, parallel, out),
            Some(Command::Add { path, remote, branch }) => run_add(cwd, &path, &remote, branch.as_deref(), out),
            Some(Command::Checkout { branch, create, repos }) => run_checkout(cwd, &branch, create, &repos, out),
        },
        Err(e) => {
            // clap's Error already renders --help/--version/usage-error text
            // to the right stream (stdout for help/version, stderr for
            // usage errors) and carries the right exit code.
            e.print().ok();
            match e.exit_code() {
                0 => ExitCode::SUCCESS,
                _ => ExitCode::FAILURE,
            }
        }
    }
}

fn run_list(cwd: &Path, out: &mut impl Write) -> ExitCode {
    let manifest = match load_manifest(cwd, out) {
        Ok((_, manifest)) => manifest,
        Err(code) => return code,
    };

    for name in manifest.repos.keys() {
        writeln!(out, "{name}").ok();
    }

    ExitCode::SUCCESS
}

fn run_sync(cwd: &Path, repos: &[String], out: &mut impl Write) -> ExitCode {
    let (root, manifest) = match load_manifest(cwd, out) {
        Ok(pair) => pair,
        Err(code) => return code,
    };

    if let Some(unknown) = repos.iter().find(|name| !manifest.repos.contains_key(*name)) {
        writeln!(out, "error: unknown repo \"{unknown}\" in workspaces.toml").ok();
        return ExitCode::FAILURE;
    }

    let targets: domain::manifest::Manifest = if repos.is_empty() {
        manifest.clone()
    } else {
        domain::manifest::Manifest {
            repos: manifest
                .repos
                .iter()
                .filter(|(name, _)| repos.contains(name))
                .map(|(name, spec)| (name.clone(), spec.clone()))
                .collect(),
        }
    };

    let actions = domain::plan::plan_sync(&targets, &|name: &str| root.join(name).exists());

    let mut failed = false;
    let mut cloned_paths: Vec<String> = Vec::new();

    for action in &actions {
        match action {
            domain::plan::SyncAction::Clone { name, remote, path } => {
                let full_path = root.join(path);
                match shell::git::clone(remote, &full_path) {
                    Ok(()) => {
                        writeln!(out, "{name}: cloned").ok();
                        cloned_paths.push(path.clone());
                    }
                    Err(e) => {
                        writeln!(out, "{name}: error: {e}").ok();
                        failed = true;
                    }
                }
            }
            domain::plan::SyncAction::Pull { name, path } => {
                let full_path = root.join(path);
                match shell::git::pull(&full_path) {
                    Ok(()) => {
                        writeln!(out, "{name}: pulled").ok();
                    }
                    Err(e) => {
                        writeln!(out, "{name}: error: {e}").ok();
                        failed = true;
                    }
                }
            }
        }
    }

    if !cloned_paths.is_empty() {
        let all_repo_paths: Vec<String> = manifest.repos.keys().cloned().collect();
        if let Err(e) = shell::fs::ensure_gitignored(&root, &all_repo_paths) {
            writeln!(out, "error: failed to update .gitignore: {e}").ok();
            failed = true;
        }
    }

    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Clone `remote` to `<workspace root>/<path>` and register it in
/// `workspaces.toml`. The duplicate-path check happens before the clone so
/// a repeat `add` on an already-declared name fails fast without touching
/// the filesystem (logged decision, story F).
fn run_add(cwd: &Path, path: &str, remote: &str, branch: Option<&str>, out: &mut impl Write) -> ExitCode {
    let (root, manifest) = match load_manifest(cwd, out) {
        Ok(pair) => pair,
        Err(code) => return code,
    };

    if manifest.repos.contains_key(path) {
        writeln!(out, "error: \"{path}\" is already declared in workspaces.toml").ok();
        return ExitCode::FAILURE;
    }

    let full_path = root.join(path);
    if let Err(e) = shell::git::clone(remote, &full_path) {
        writeln!(out, "error: {e}").ok();
        return ExitCode::FAILURE;
    }

    let updated = match domain::manifest::add_repo(&manifest, path, remote, branch.map(str::to_string)) {
        Ok(updated) => updated,
        Err(e) => {
            writeln!(out, "error: {e}").ok();
            return ExitCode::FAILURE;
        }
    };

    let manifest_path = root.join("workspaces.toml");
    let serialized = domain::manifest::serialize_manifest(&updated);
    if let Err(e) = shell::fs::write_string(&manifest_path, &serialized) {
        writeln!(out, "error: failed to write {}: {e}", manifest_path.display()).ok();
        return ExitCode::FAILURE;
    }

    writeln!(out, "{path}: cloned and added to workspaces.toml").ok();
    ExitCode::SUCCESS
}

/// Checkout (or create) `branch` across every selected repo. A failure in
/// one repo never aborts the others — same collect-and-continue family as
/// `sync`/`exec`, so a multi-repo checkout reports every repo's outcome
/// instead of leaving the caller guessing which ones already moved (logged
/// decision, story G).
fn run_checkout(cwd: &Path, branch: &str, create: bool, repos: &[String], out: &mut impl Write) -> ExitCode {
    let (root, manifest) = match load_manifest(cwd, out) {
        Ok(pair) => pair,
        Err(code) => return code,
    };

    let targets = match domain::select::resolve_targets(&manifest, repos) {
        Ok(targets) => targets,
        Err(e) => {
            writeln!(out, "error: {e}").ok();
            return ExitCode::FAILURE;
        }
    };

    let mut failed = false;
    for name in &targets {
        let path = root.join(name);
        let result = if create {
            shell::git::create_branch(&path, branch)
        } else {
            shell::git::checkout(&path, branch)
        };

        match result {
            Ok(()) => {
                let verb = if create { "created and checked out" } else { "checked out" };
                writeln!(out, "{name}: {verb} {branch}").ok();
            }
            Err(e) => {
                writeln!(out, "{name}: error: {e}").ok();
                failed = true;
            }
        }
    }

    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn run_status(cwd: &Path, repos: &[String], out: &mut impl Write) -> ExitCode {
    let (root, manifest) = match load_manifest(cwd, out) {
        Ok(pair) => pair,
        Err(code) => return code,
    };

    let targets = match domain::select::resolve_targets(&manifest, repos) {
        Ok(targets) => targets,
        Err(e) => {
            writeln!(out, "error: {e}").ok();
            return ExitCode::FAILURE;
        }
    };

    let mut raw_states = std::collections::BTreeMap::new();
    for name in &targets {
        let path = root.join(name);
        match shell::git::gather_status(&path) {
            Ok(state) => {
                raw_states.insert(name.clone(), state);
            }
            Err(e) => {
                writeln!(out, "{name}: error: {e}").ok();
                return ExitCode::FAILURE;
            }
        }
    }

    let statuses = domain::status::compute_status(&manifest, &raw_states);
    for line in format_status_table(&statuses) {
        writeln!(out, "{line}").ok();
    }

    ExitCode::SUCCESS
}

/// Run `command` in each selected repo's directory. Sequential by default;
/// `parallel` fans the runs out across threads instead. A non-zero exit in
/// one repo never aborts the others — every repo runs to completion and
/// gets a line in the summary, and the overall exit code is failure if any
/// repo failed (or couldn't even be spawned).
fn run_exec(cwd: &Path, repos: &[String], command: &[String], parallel: bool, out: &mut impl Write) -> ExitCode {
    let (root, manifest) = match load_manifest(cwd, out) {
        Ok(pair) => pair,
        Err(code) => return code,
    };

    let targets = match domain::select::resolve_targets(&manifest, repos) {
        Ok(targets) => targets,
        Err(e) => {
            writeln!(out, "error: {e}").ok();
            return ExitCode::FAILURE;
        }
    };

    let results = run_command_across(&root, &targets, command, parallel);

    let mut failed = false;
    for (_name, result) in &results {
        match result {
            Ok(outcome) => {
                if !outcome.stdout.is_empty() {
                    write!(out, "{}", outcome.stdout).ok();
                }
                if !outcome.stderr.is_empty() {
                    write!(out, "{}", outcome.stderr).ok();
                }
                if outcome.exit_code != 0 {
                    failed = true;
                }
            }
            Err(_) => failed = true,
        }
    }

    writeln!(out).ok();
    writeln!(out, "Summary:").ok();
    for (name, result) in &results {
        match result {
            Ok(outcome) => {
                writeln!(out, "{name}: exit {}", outcome.exit_code).ok();
            }
            Err(e) => {
                writeln!(out, "{name}: error: {e}").ok();
            }
        }
    }

    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Run `command` across every target's directory, either one after another
/// or concurrently, always returning results in the same order as
/// `targets` regardless of which mode ran or which thread finished first —
/// that ordering-by-submission-not-by-completion is what keeps a repo's
/// result correctly attributed to its name under `--parallel` (logged
/// decision, story E).
fn run_command_across(
    root: &Path,
    targets: &[String],
    command: &[String],
    parallel: bool,
) -> Vec<(String, Result<shell::exec::ExecOutcome, shell::exec::ExecError>)> {
    if !parallel {
        return targets
            .iter()
            .map(|name| {
                let path = root.join(name);
                (name.clone(), shell::exec::run(&path, command))
            })
            .collect();
    }

    std::thread::scope(|scope| {
        let handles: Vec<_> = targets
            .iter()
            .map(|name| {
                let path = root.join(name);
                scope.spawn(move || (name.clone(), shell::exec::run(&path, command)))
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("exec worker thread panicked"))
            .collect()
    })
}

/// Render the `status` output table: REPO/BRANCH/STATE/SYNC, plus a NOTE
/// column only when at least one row has one. Columns are aligned: each
/// column's width is max(header length, longest cell in that column) + 2
/// trailing spaces, except the last column which is left unpadded.
fn format_status_table(rows: &[domain::status::RepoStatus]) -> Vec<String> {
    let has_note = rows.iter().any(|r| r.note.is_some());

    let mut header = vec!["REPO".to_string(), "BRANCH".to_string(), "STATE".to_string(), "SYNC".to_string()];
    if has_note {
        header.push("NOTE".to_string());
    }

    let body: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let state = if r.dirty_count == 0 {
                "clean".to_string()
            } else {
                format!("dirty ({})", r.dirty_count)
            };
            let sync = match (r.ahead, r.behind) {
                (0, 0) => "up to date".to_string(),
                (a, 0) => format!("ahead {a}"),
                (0, b) => format!("behind {b}"),
                (a, b) => format!("ahead {a}, behind {b}"),
            };
            let mut cells = vec![r.name.clone(), r.branch.clone(), state, sync];
            if has_note {
                cells.push(r.note.clone().unwrap_or_default());
            }
            cells
        })
        .collect();

    let col_count = header.len();
    let mut widths = vec![0usize; col_count];
    for (i, cell) in header.iter().enumerate() {
        widths[i] = cell.len();
    }
    for row in &body {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.len());
        }
    }

    let render_row = |cells: &[String]| -> String {
        let mut line = String::new();
        for (i, cell) in cells.iter().enumerate() {
            if i + 1 == col_count {
                line.push_str(cell);
            } else {
                line.push_str(&format!("{:<width$}", cell, width = widths[i] + 2));
            }
        }
        line
    };

    let mut lines = vec![render_row(&header)];
    lines.extend(body.iter().map(|row| render_row(row)));
    lines
}

/// Discover the workspace root from `cwd` and load+parse its manifest,
/// writing a clear error to `out` and returning a failure exit code on any
/// problem. Shared by every command that needs the manifest.
fn load_manifest(
    cwd: &Path,
    out: &mut impl Write,
) -> Result<(std::path::PathBuf, domain::manifest::Manifest), ExitCode> {
    let Some(root) = domain::discover::find_workspace_root(cwd, shell::fs::exists) else {
        writeln!(
            out,
            "error: no workspaces.toml found in {} or any parent directory",
            cwd.display()
        )
        .ok();
        return Err(ExitCode::FAILURE);
    };

    let manifest_path = root.join("workspaces.toml");
    let contents = match shell::fs::read_to_string(&manifest_path) {
        Ok(contents) => contents,
        Err(e) => {
            writeln!(out, "error: failed to read {}: {e}", manifest_path.display()).ok();
            return Err(ExitCode::FAILURE);
        }
    };

    let manifest = match domain::manifest::parse_manifest(&contents) {
        Ok(manifest) => manifest,
        Err(e) => {
            writeln!(out, "error: failed to parse {}: {e}", manifest_path.display()).ok();
            return Err(ExitCode::FAILURE);
        }
    };

    Ok((root, manifest))
}
