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
    let Some(root) = domain::discover::find_workspace_root(cwd, shell::fs::exists) else {
        writeln!(
            out,
            "error: no workspaces.toml found in {} or any parent directory",
            cwd.display()
        )
        .ok();
        return ExitCode::FAILURE;
    };

    let manifest_path = root.join("workspaces.toml");
    let contents = match shell::fs::read_to_string(&manifest_path) {
        Ok(contents) => contents,
        Err(e) => {
            writeln!(
                out,
                "error: failed to read {}: {e}",
                manifest_path.display()
            )
            .ok();
            return ExitCode::FAILURE;
        }
    };

    let manifest = match domain::manifest::parse_manifest(&contents) {
        Ok(manifest) => manifest,
        Err(e) => {
            writeln!(
                out,
                "error: failed to parse {}: {e}",
                manifest_path.display()
            )
            .ok();
            return ExitCode::FAILURE;
        }
    };

    for name in manifest.repos.keys() {
        writeln!(out, "{name}").ok();
    }

    ExitCode::SUCCESS
}
