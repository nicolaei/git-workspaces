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
