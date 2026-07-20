mod support;

use support::Workspace;

#[test]
fn init_in_a_fresh_dir_creates_manifest_and_git_repo() {
    let workspace = Workspace::new();

    let result = workspace.run(&["init"]);

    assert!(
        result.success,
        "expected init to succeed, stdout={} stderr={}",
        result.stdout, result.stderr
    );
    assert!(
        workspace.root().join("workspace.toml").exists(),
        "expected workspace.toml to be created"
    );
    assert!(
        workspace.root().join(".git").exists(),
        "expected .git to be created"
    );

    let manifest = std::fs::read_to_string(workspace.root().join("workspace.toml"))
        .expect("read manifest after init");
    assert!(manifest.is_empty(), "expected a genuinely empty manifest, got: {manifest:?}");

    let list_result = workspace.run(&["list"]);
    assert!(
        list_result.success,
        "expected list to run successfully against a freshly init'd workspace, stdout={} stderr={}",
        list_result.stdout, list_result.stderr
    );
}

#[test]
fn init_refuses_to_clobber_an_existing_manifest() {
    let workspace = Workspace::new();
    workspace.declares_repo("api", "git@github.com:org/api.git");
    let before = std::fs::read_to_string(workspace.root().join("workspace.toml"))
        .expect("read manifest before init");

    let result = workspace.run(&["init"]);

    assert!(!result.success, "expected init to fail against an existing manifest");
    assert!(
        result.stdout.contains("already exists") || result.stderr.contains("already exists"),
        "expected a clear already-exists error, stdout={} stderr={}",
        result.stdout, result.stderr
    );

    let after = std::fs::read_to_string(workspace.root().join("workspace.toml"))
        .expect("read manifest after failed init");
    assert_eq!(after, before, "expected the existing manifest to be left untouched");
}

#[test]
fn init_does_not_reinit_or_destroy_history_in_an_existing_git_repo() {
    let workspace = Workspace::new();
    std::fs::write(workspace.root().join("workspace.toml"), "").expect("seed empty manifest");
    workspace.init_as_git_repo();
    std::fs::remove_file(workspace.root().join("workspace.toml")).expect("remove manifest to test init again");
    // Note: workspace.toml is now absent but .git and its commit history
    // remain, exercising "already a git repo" without a manifest present.

    let before_head = std::process::Command::new("git")
        .args(["-C"])
        .arg(workspace.root())
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("run git rev-parse HEAD before init");
    let before_head = String::from_utf8_lossy(&before_head.stdout).trim().to_string();

    let result = workspace.run(&["init"]);

    assert!(
        result.success,
        "expected init to succeed against an existing git repo, stdout={} stderr={}",
        result.stdout, result.stderr
    );
    assert!(workspace.root().join("workspace.toml").exists(), "expected workspace.toml to be written");

    let after_head = std::process::Command::new("git")
        .args(["-C"])
        .arg(workspace.root())
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("run git rev-parse HEAD after init");
    let after_head = String::from_utf8_lossy(&after_head.stdout).trim().to_string();

    assert_eq!(after_head, before_head, "expected existing history to survive init");
}

#[test]
fn init_creates_the_target_directory_if_it_does_not_exist_yet() {
    let workspace = Workspace::new();
    let target = workspace.root().join("nested/new-workspace");
    assert!(!target.exists(), "precondition: target directory should not exist yet");

    let result = workspace.run(&["init", target.to_str().unwrap()]);

    assert!(
        result.success,
        "expected init to succeed for a not-yet-existing target dir, stdout={} stderr={}",
        result.stdout, result.stderr
    );
    assert!(target.join("workspace.toml").exists(), "expected workspace.toml under the created directory");
    assert!(target.join(".git").exists(), "expected .git under the created directory");
}
