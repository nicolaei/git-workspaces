use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use clap::Parser;

/// `git workspaces` — manifest-driven multi-repo git plugin.
///
/// This is the walking-skeleton entrypoint: it parses args and exits.
/// No subcommands yet — that lands in later stories.
#[derive(Parser, Debug)]
#[command(name = "git-workspaces", version, about, long_about = None)]
struct Cli;

/// The one true entrypoint. `main.rs` is a thin wrapper around this.
///
/// Takes an explicit argument iterator, cwd, and output sink so it can be
/// exercised in-process by tests without touching real env args or stdout.
pub fn run(args: impl Iterator<Item = String>, _cwd: &Path, _out: &mut impl Write) -> ExitCode {
    match Cli::try_parse_from(args) {
        Ok(_) => ExitCode::SUCCESS,
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
