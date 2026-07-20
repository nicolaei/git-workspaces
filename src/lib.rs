use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod domain;
mod shell;

/// `git multirepo` — manifest-driven multi-repo git plugin.
#[derive(Parser, Debug)]
#[command(name = "git-multirepo", version, about, long_about = None)]
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
    /// register it in `multirepo.toml`. The manifest key doubles as the
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
    /// Manage full-fleet worktrees — one named, independent copy of every
    /// repo in the manifest at once. Whole workspace only; there's no
    /// per-repo narrowing here (logged decision, story H).
    Worktree {
        #[command(subcommand)]
        action: WorktreeAction,
    },
    /// Bootstrap a brand-new workspace: write a genuinely empty
    /// `multirepo.toml` and `git init` if `path` isn't already a git repo.
    /// `path` defaults to cwd (same convention as `git init <dir>`) and is
    /// created if it doesn't exist. Refuses to clobber an existing
    /// manifest. Doesn't walk up parent directories looking for an
    /// ancestor workspace — mirrors how `git init` itself doesn't stop you
    /// from nesting repos.
    Init {
        /// Where to create the workspace. Defaults to the current directory.
        path: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum WorktreeAction {
    /// Add a new named worktree covering every repo in the manifest.
    /// Branch defaults to `<name>`; `--branch` decouples the two.
    Add {
        /// The worktree's name, also the default branch name.
        name: String,
        /// Use this branch name instead of `<name>`.
        #[arg(long)]
        branch: Option<String>,
    },
    /// List named worktrees under `.worktrees/`, flagging any that don't
    /// cover every manifest repo.
    List,
    /// Remove a named worktree: `git worktree remove` per repo, then
    /// delete the now-empty `.worktrees/<name>/` directory.
    Remove {
        /// The worktree's name.
        name: String,
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
            Some(Command::List) => run_list(cwd, out).unwrap_or_else(|c| c),
            Some(Command::Sync { repos }) => run_sync(cwd, &repos, out).unwrap_or_else(|c| c),
            Some(Command::Status { repos }) => run_status(cwd, &repos, out).unwrap_or_else(|c| c),
            Some(Command::Exec { repos, parallel, command }) => {
                run_exec(cwd, &repos, &command, parallel, out).unwrap_or_else(|c| c)
            }
            Some(Command::Add { path, remote, branch }) => {
                run_add(cwd, &path, &remote, branch.as_deref(), out).unwrap_or_else(|c| c)
            }
            Some(Command::Checkout { branch, create, repos }) => {
                run_checkout(cwd, &branch, create, &repos, out).unwrap_or_else(|c| c)
            }
            Some(Command::Worktree { action }) => run_worktree(cwd, action, out).unwrap_or_else(|c| c),
            Some(Command::Init { path }) => run_init(cwd, path.as_deref(), out),
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

fn run_list(cwd: &Path, out: &mut impl Write) -> Result<ExitCode, ExitCode> {
    let (_, manifest) = load_manifest(cwd, out)?;

    for name in manifest.repos.keys() {
        writeln!(out, "{name}").ok();
    }

    Ok(ExitCode::SUCCESS)
}

fn run_sync(cwd: &Path, repos: &[String], out: &mut impl Write) -> Result<ExitCode, ExitCode> {
    let (root, manifest) = load_manifest(cwd, out)?;

    // Delegates to the same narrowing logic status/checkout/exec use,
    // rather than reimplementing it — the old inline version missed the
    // duplicate-name check `resolve_targets` already provides (Fowler:
    // Divergent Change).
    let targets = domain::select::resolve_targets(&manifest, repos).map_err(|e| {
        writeln!(out, "error: {e}").ok();
        ExitCode::FAILURE
    })?;
    let target_manifest = domain::select::manifest_subset(&manifest, &targets);

    let actions = domain::plan::plan_sync(&target_manifest, &|name: &str| root.join(name).exists());
    let failed = execute_sync_actions(&root, &actions, out);

    if !manifest.repos.is_empty() {
        update_gitignore(&root, &manifest, &[], out)?;
    }

    Ok(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}

/// Clone or pull every planned action, collecting-and-continuing rather
/// than aborting on the first failure — one repo's clone/pull error never
/// stops the rest. Returns whether anything failed. Split out of `run_sync`
/// itself (Fowler: Long Function — Extract Function).
fn execute_sync_actions(root: &Path, actions: &[domain::plan::SyncAction], out: &mut impl Write) -> bool {
    let mut failed = false;

    for action in actions {
        match action {
            domain::plan::SyncAction::Clone { name, remote } => {
                let full_path = root.join(name);
                match shell::git::clone(remote, &full_path) {
                    Ok(()) => {
                        writeln!(out, "{name}: cloned").ok();
                    }
                    Err(e) => {
                        writeln!(out, "{name}: error: {e}").ok();
                        failed = true;
                    }
                }
            }
            domain::plan::SyncAction::Pull { name } => {
                let full_path = root.join(name);
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

    failed
}

/// Regenerate the workspace root's managed `.gitignore` block for every
/// repo in `manifest`, plus any `extra` entries (e.g. `.worktrees`).
/// `sync`/`add`/`worktree add` each used to repeat the same
/// collect-keys-then-call block (Fowler: Duplicate Code — Extract
/// Function).
fn update_gitignore(
    root: &Path,
    manifest: &domain::manifest::Manifest,
    extra: &[&str],
    out: &mut impl Write,
) -> Result<(), ExitCode> {
    let paths = manifest.repos.keys().map(String::as_str).chain(extra.iter().copied());
    shell::fs::ensure_gitignored(root, paths).map_err(|e| {
        writeln!(out, "error: failed to update .gitignore: {e}").ok();
        ExitCode::FAILURE
    })
}

/// Bootstrap a brand-new workspace at `path` (cwd if omitted), creating the
/// directory if needed. Refuses to clobber an existing `multirepo.toml` at
/// that exact path — no upward discovery here, unlike every other command;
/// this one is about creating a workspace, not finding one. `git init`s the
/// target only if it isn't already a git repo, so an existing repo's
/// history is left untouched.
fn run_init(cwd: &Path, path: Option<&str>, out: &mut impl Write) -> ExitCode {
    let target = match path {
        Some(p) => cwd.join(p),
        None => cwd.to_path_buf(),
    };

    if let Err(e) = shell::fs::create_dir_all(&target) {
        writeln!(out, "error: failed to create {}: {e}", target.display()).ok();
        return ExitCode::FAILURE;
    }

    let manifest_path = target.join("multirepo.toml");
    if shell::fs::exists(&manifest_path) {
        writeln!(out, "error: multirepo.toml already exists at {}", target.display()).ok();
        return ExitCode::FAILURE;
    }

    if !shell::fs::exists(&target.join(".git")) {
        if let Err(e) = shell::git::init(&target) {
            writeln!(out, "error: {e}").ok();
            return ExitCode::FAILURE;
        }
    }

    let empty = domain::manifest::Manifest { repos: std::collections::BTreeMap::new() };
    let serialized = domain::manifest::serialize_manifest(&empty);
    if let Err(e) = shell::fs::write_string(&manifest_path, &serialized) {
        writeln!(out, "error: failed to write {}: {e}", manifest_path.display()).ok();
        return ExitCode::FAILURE;
    }

    writeln!(out, "initialized workspace at {}", target.display()).ok();
    ExitCode::SUCCESS
}

/// Clone `remote` to `<workspace root>/<path>` and register it in
/// `multirepo.toml`. The duplicate-path check happens before the clone so
/// a repeat `add` on an already-declared name fails fast without touching
/// the filesystem (logged decision, story F).
fn run_add(cwd: &Path, path: &str, remote: &str, branch: Option<&str>, out: &mut impl Write) -> Result<ExitCode, ExitCode> {
    let (root, manifest) = load_manifest(cwd, out)?;

    if manifest.repos.contains_key(path) {
        writeln!(out, "error: \"{path}\" is already declared in multirepo.toml").ok();
        return Err(ExitCode::FAILURE);
    }

    // Captured now, before the clone (which can take a while) — the baseline
    // `write_manifest_if_unchanged` checks against right before writing, to
    // detect a concurrently running `add` racing on the same manifest file.
    let manifest_path = root.join("multirepo.toml");
    let baseline = shell::fs::read_to_string(&manifest_path).map_err(|e| {
        writeln!(out, "error: failed to read {}: {e}", manifest_path.display()).ok();
        ExitCode::FAILURE
    })?;

    let full_path = root.join(path);
    shell::git::clone(remote, &full_path).map_err(|e| {
        writeln!(out, "error: {e}").ok();
        ExitCode::FAILURE
    })?;

    let updated = domain::manifest::add_repo(&manifest, path, remote, branch.map(str::to_string)).map_err(|e| {
        writeln!(out, "error: {e}").ok();
        ExitCode::FAILURE
    })?;

    let serialized = domain::manifest::serialize_manifest(&updated);
    match shell::fs::write_manifest_if_unchanged(&manifest_path, &baseline, &serialized) {
        Ok(shell::fs::ManifestWriteOutcome::Written) => {}
        Ok(shell::fs::ManifestWriteOutcome::ConcurrentModification) => {
            writeln!(
                out,
                "error: multirepo.toml changed on disk since it was read — refusing to overwrite a concurrent change. {path} was already cloned to {}; add it to multirepo.toml by hand, or remove the directory and re-run `git multirepo add`.",
                full_path.display()
            )
            .ok();
            return Err(ExitCode::FAILURE);
        }
        Err(e) => {
            writeln!(out, "error: failed to write {}: {e}", manifest_path.display()).ok();
            return Err(ExitCode::FAILURE);
        }
    }

    update_gitignore(&root, &updated, &[], out)?;

    writeln!(out, "{path}: cloned and added to multirepo.toml").ok();
    Ok(ExitCode::SUCCESS)
}

/// Checkout (or create) `branch` across every selected repo. A failure in
/// one repo never aborts the others — same collect-and-continue family as
/// `sync`/`exec`, so a multi-repo checkout reports every repo's outcome
/// instead of leaving the caller guessing which ones already moved (logged
/// decision, story G).
fn run_checkout(cwd: &Path, branch: &str, create: bool, repos: &[String], out: &mut impl Write) -> Result<ExitCode, ExitCode> {
    let (root, manifest) = load_manifest(cwd, out)?;

    let targets = domain::select::resolve_targets(&manifest, repos).map_err(|e| {
        writeln!(out, "error: {e}").ok();
        ExitCode::FAILURE
    })?;

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

    Ok(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}

fn run_worktree(cwd: &Path, action: WorktreeAction, out: &mut impl Write) -> Result<ExitCode, ExitCode> {
    match action {
        WorktreeAction::Add { name, branch } => run_worktree_add(cwd, &name, branch.as_deref(), out),
        WorktreeAction::List => run_worktree_list(cwd, out),
        WorktreeAction::Remove { name } => run_worktree_remove(cwd, &name, out),
    }
}

/// Add a full-fleet worktree named `name`. Every repo in the manifest must
/// already be cloned in its primary location — verified up front, with no
/// side effects, so a missing repo fails fast with a clear "run sync
/// first" message instead of leaving a half-built `.worktrees/<name>/`
/// behind (logged decision, story H).
///
/// A failure partway through the fleet stops immediately rather than
/// collecting-and-continuing like `sync`/`checkout`/`exec`: a worktree is
/// one atomic named copy of the whole fleet, and an already-created
/// worktree from a failed run would sit there half-finished with no
/// manifest copy — worse than failing loudly on the first repo that can't
/// be added (logged decision, story H).
fn run_worktree_add(cwd: &Path, name: &str, branch: Option<&str>, out: &mut impl Write) -> Result<ExitCode, ExitCode> {
    let (root, manifest) = load_manifest(cwd, out)?;

    if let Some(missing) = manifest.repos.keys().find(|repo| !root.join(repo).exists()) {
        writeln!(out, "error: \"{missing}\" is not cloned yet — run `git multirepo sync` first").ok();
        return Err(ExitCode::FAILURE);
    }

    let branch = branch.unwrap_or(name);
    let layout = domain::worktree::worktree_layout(&manifest, name);

    for (repo, rel_path) in &layout {
        let repo_path = root.join(repo);
        let target_path = root.join(rel_path);
        if let Some(parent) = target_path.parent() {
            shell::fs::create_dir_all(parent).map_err(|e| {
                writeln!(out, "error: failed to create {}: {e}", parent.display()).ok();
                ExitCode::FAILURE
            })?;
        }
        shell::git::worktree_add(&repo_path, &target_path, branch).map_err(|e| {
            writeln!(out, "{repo}: error: {e}").ok();
            ExitCode::FAILURE
        })?;
        writeln!(out, "{repo}: worktree added at {}", target_path.display()).ok();
    }

    let worktree_root = root.join(".worktrees").join(name);
    let manifest_path = worktree_root.join("multirepo.toml");
    let serialized = domain::manifest::serialize_manifest(&manifest);
    shell::fs::write_string(&manifest_path, &serialized).map_err(|e| {
        writeln!(out, "error: failed to write {}: {e}", manifest_path.display()).ok();
        ExitCode::FAILURE
    })?;

    update_gitignore(&root, &manifest, &[".worktrees"], out)?;

    writeln!(out, "{name}: worktree added ({branch})").ok();
    Ok(ExitCode::SUCCESS)
}

/// List named worktrees under `.worktrees/`. A worktree missing a
/// subdirectory for one or more manifest repos is reported as broken
/// rather than hidden — a partial worktree (e.g. from an interrupted
/// `add`) still needs to be visible so it can be cleaned up (logged
/// decision, story H).
fn run_worktree_list(cwd: &Path, out: &mut impl Write) -> Result<ExitCode, ExitCode> {
    let (root, manifest) = load_manifest(cwd, out)?;

    let worktrees_dir = root.join(".worktrees");
    let names = shell::fs::list_subdirectory_names(&worktrees_dir).map_err(|e| {
        writeln!(out, "error: failed to read {}: {e}", worktrees_dir.display()).ok();
        ExitCode::FAILURE
    })?;

    for name in &names {
        let layout = domain::worktree::worktree_layout(&manifest, name);
        let missing: Vec<&str> = layout
            .iter()
            .filter(|(_, rel_path)| !root.join(rel_path).is_dir())
            .map(|(repo, _)| repo.as_str())
            .collect();

        let worktree_root = worktrees_dir.join(name);
        if missing.is_empty() {
            let branch = layout
                .first()
                .and_then(|(_, rel_path)| shell::git::current_branch(&root.join(rel_path)).ok())
                .unwrap_or_else(|| "?".to_string());
            writeln!(out, "{name}: {branch} ({})", worktree_root.display()).ok();
        } else {
            writeln!(out, "{name}: broken, missing {} ({})", missing.join(", "), worktree_root.display()).ok();
        }
    }

    Ok(ExitCode::SUCCESS)
}

/// Remove a named worktree: `git worktree remove` per repo from its
/// primary clone, then delete the now-empty `.worktrees/<name>/`
/// directory (including the manifest copy). A failure removing one repo's
/// worktree stops immediately and leaves the directory in place — this is
/// cleanup of one atomic unit, not a fan-out command where partial
/// completion across independent repos is acceptable (logged decision,
/// story H).
///
/// Errors on a `name` that was never added — same convention as `git
/// worktree remove`/`git branch -d`/`rm`: a remove command fails loudly on
/// a nonexistent target by default, and silent idempotent success is
/// always an opt-in (`rm -f`), never the default (logged decision).
fn run_worktree_remove(cwd: &Path, name: &str, out: &mut impl Write) -> Result<ExitCode, ExitCode> {
    let (root, manifest) = load_manifest(cwd, out)?;

    let worktree_root = root.join(".worktrees").join(name);
    if !worktree_root.exists() {
        writeln!(out, "error: no worktree named \"{name}\"").ok();
        return Err(ExitCode::FAILURE);
    }

    let layout = domain::worktree::worktree_layout(&manifest, name);

    for (repo, rel_path) in &layout {
        let repo_path = root.join(repo);
        let target_path = root.join(rel_path);
        if !target_path.exists() {
            continue;
        }
        shell::git::worktree_remove(&repo_path, &target_path).map_err(|e| {
            writeln!(out, "{repo}: error: {e}").ok();
            ExitCode::FAILURE
        })?;
        writeln!(out, "{repo}: worktree removed").ok();
    }

    shell::fs::remove_dir_all(&worktree_root).map_err(|e| {
        writeln!(out, "error: failed to remove {}: {e}", worktree_root.display()).ok();
        ExitCode::FAILURE
    })?;

    writeln!(out, "{name}: worktree removed").ok();
    Ok(ExitCode::SUCCESS)
}

fn run_status(cwd: &Path, repos: &[String], out: &mut impl Write) -> Result<ExitCode, ExitCode> {
    let (root, manifest) = load_manifest(cwd, out)?;

    let targets = domain::select::resolve_targets(&manifest, repos).map_err(|e| {
        writeln!(out, "error: {e}").ok();
        ExitCode::FAILURE
    })?;

    let mut raw_states = std::collections::BTreeMap::new();
    for name in &targets {
        let path = root.join(name);
        match shell::git::gather_status(&path) {
            Ok(state) => {
                raw_states.insert(name.clone(), state);
            }
            Err(e) => {
                writeln!(out, "{name}: error: {e}").ok();
                return Err(ExitCode::FAILURE);
            }
        }
    }

    let statuses = domain::status::compute_status(&manifest, &raw_states);
    for line in format_status_table(&statuses) {
        writeln!(out, "{line}").ok();
    }

    Ok(ExitCode::SUCCESS)
}

/// Run `command` in each selected repo's directory. Sequential by default;
/// `parallel` fans the runs out across threads instead. A non-zero exit in
/// one repo never aborts the others — every repo runs to completion and
/// gets a line in the summary, and the overall exit code is failure if any
/// repo failed (or couldn't even be spawned).
fn run_exec(cwd: &Path, repos: &[String], command: &[String], parallel: bool, out: &mut impl Write) -> Result<ExitCode, ExitCode> {
    let (root, manifest) = load_manifest(cwd, out)?;

    let targets = domain::select::resolve_targets(&manifest, repos).map_err(|e| {
        writeln!(out, "error: {e}").ok();
        ExitCode::FAILURE
    })?;

    let results = run_command_across(&root, &targets, command, parallel);

    let failed = print_exec_output(&results, out);
    print_exec_summary(&results, out);

    Ok(if failed { ExitCode::FAILURE } else { ExitCode::SUCCESS })
}

type ExecResults = [(String, Result<shell::exec::ExecOutcome, shell::exec::ExecError>)];

/// Stream each repo's captured stdout/stderr in target order. Returns
/// whether any repo's command exited non-zero or failed to spawn. Split
/// from `print_exec_summary` — `run_exec` used to interleave "emit live
/// output" and "emit summary" in one function (Fowler: Long Function —
/// Extract Function).
fn print_exec_output(results: &ExecResults, out: &mut impl Write) -> bool {
    let mut failed = false;
    for (_name, result) in results {
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
    failed
}

/// Print the trailing `Summary:` block — one line per repo naming its exit
/// code or spawn error (Fowler: Long Function — Extract Function, split
/// from `print_exec_output`).
fn print_exec_summary(results: &ExecResults, out: &mut impl Write) {
    writeln!(out).ok();
    writeln!(out, "Summary:").ok();
    for (name, result) in results {
        match result {
            Ok(outcome) => {
                writeln!(out, "{name}: exit {}", outcome.exit_code).ok();
            }
            Err(e) => {
                writeln!(out, "{name}: error: {e}").ok();
            }
        }
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
            "error: no multirepo.toml found in {} or any parent directory",
            cwd.display()
        )
        .ok();
        return Err(ExitCode::FAILURE);
    };

    let manifest_path = root.join("multirepo.toml");
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
