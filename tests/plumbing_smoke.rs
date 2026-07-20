//! The ONE test in the whole suite allowed to spawn the real `git` binary.
//!
//! It proves git's own subcommand dispatch finds our binary on PATH when
//! named `git-multirepo` — nothing else should ever re-test that mechanism.

use std::fs;
use std::path::PathBuf;

use assert_cmd::cargo::cargo_bin;

#[test]
fn git_multirepo_version_resolves_via_real_git_path_dispatch() {
    let built_binary = cargo_bin("git-multirepo");

    let temp_dir = tempfile::tempdir().expect("create temp dir for fake PATH");
    let linked_binary = temp_dir.path().join("git-multirepo");
    fs::copy(&built_binary, &linked_binary).expect("copy built binary onto temp PATH");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&linked_binary).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&linked_binary, perms).unwrap();
    }

    let existing_path = std::env::var_os("PATH").unwrap_or_default();
    let mut path_entries: Vec<PathBuf> = vec![temp_dir.path().to_path_buf()];
    path_entries.extend(std::env::split_paths(&existing_path));
    let new_path = std::env::join_paths(path_entries).expect("join PATH entries");

    let output = std::process::Command::new("git")
        .arg("multirepo")
        .arg("--version")
        .env("PATH", &new_path)
        .output()
        .expect("run real system git");

    assert!(
        output.status.success(),
        "git multirepo --version failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("git-multirepo"),
        "expected version output to mention git-multirepo, got: {stdout}"
    );
}
