use std::env;
use std::io;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args = env::args();
    let cwd = env::current_dir().expect("cwd must be readable");
    git_multirepo::run(args, &cwd, &mut io::stdout())
}
