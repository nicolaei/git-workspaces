mod support;

use support::Workspace;

#[test]
fn worktree_add_creates_a_real_independent_copy_for_every_repo() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    let web_remote = workspace.fixture_remote_with_commit("web");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.declares_repo("web", web_remote.to_str().unwrap());
    workspace.run(&["sync"]);

    let result = workspace.run(&["worktree", "add", "feature-x"]);

    assert!(result.success, "expected worktree add to succeed, stdout={} stderr={}", result.stdout, result.stderr);
    assert!(workspace.root().join(".worktrees/feature-x/api").is_dir(), "expected api worktree copy to exist");
    assert!(workspace.root().join(".worktrees/feature-x/web").is_dir(), "expected web worktree copy to exist");
    // Independent working copies: a file written into the worktree must not
    // appear in the primary clone.
    std::fs::write(workspace.root().join(".worktrees/feature-x/api/only-in-worktree.txt"), "x").unwrap();
    assert!(!workspace.root().join("api/only-in-worktree.txt").exists(), "expected primary clone to be untouched by worktree edits");
}

#[test]
fn worktree_add_defaults_the_branch_name_to_the_worktree_name() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.run(&["sync"]);

    let result = workspace.run(&["worktree", "add", "feature-x"]);

    assert!(result.success, "expected worktree add to succeed, stdout={} stderr={}", result.stdout, result.stderr);
    let output = std::process::Command::new("git")
        .args(["-C"])
        .arg(workspace.root().join(".worktrees/feature-x/api"))
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .expect("run git rev-parse in worktree");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "feature-x");
}

#[test]
fn worktree_add_branch_flag_overrides_the_default_branch_name() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.run(&["sync"]);

    let result = workspace.run(&["worktree", "add", "feature-x", "--branch", "custom-branch"]);

    assert!(result.success, "expected worktree add to succeed, stdout={} stderr={}", result.stdout, result.stderr);
    let output = std::process::Command::new("git")
        .args(["-C"])
        .arg(workspace.root().join(".worktrees/feature-x/api"))
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .expect("run git rev-parse in worktree");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "custom-branch");
}

#[test]
fn worktree_add_writes_a_manifest_copy_making_the_worktree_independently_discoverable() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    let web_remote = workspace.fixture_remote_with_commit("web");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.declares_repo("web", web_remote.to_str().unwrap());
    workspace.run(&["sync"]);
    workspace.run(&["worktree", "add", "feature-x"]);

    let nested_cwd = workspace.root().join(".worktrees/feature-x");
    let result = workspace.run_from(&nested_cwd, &["list"]);

    assert!(result.success, "expected list to succeed from inside the worktree root, stdout={} stderr={}", result.stdout, result.stderr);
    assert!(result.stdout.contains("api"), "expected list from inside the worktree to see api, got: {}", result.stdout);
    assert!(result.stdout.contains("web"), "expected list from inside the worktree to see web, got: {}", result.stdout);

    let status_result = workspace.run_from(&nested_cwd, &["status"]);
    assert!(
        status_result.success,
        "expected status to succeed from inside the worktree root, stdout={} stderr={}",
        status_result.stdout, status_result.stderr
    );
}

#[test]
fn worktree_add_fails_fast_when_a_repo_has_not_been_synced_yet() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    // Deliberately skip `sync` — api is declared but never cloned.

    let result = workspace.run(&["worktree", "add", "feature-x"]);

    assert!(!result.success, "expected worktree add to fail when a repo isn't cloned yet");
    assert!(!workspace.root().join(".worktrees").exists(), "expected no partial .worktrees directory to be created");
}

#[test]
fn worktree_list_reports_a_previously_added_worktree() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.run(&["sync"]);
    workspace.run(&["worktree", "add", "feature-x"]);

    let result = workspace.run(&["worktree", "list"]);

    assert!(result.success, "expected worktree list to succeed, stdout={} stderr={}", result.stdout, result.stderr);
    assert!(result.stdout.contains("feature-x"), "expected worktree list to mention feature-x, got: {}", result.stdout);
}

#[test]
fn worktree_remove_cleans_up_every_repo_and_the_directory() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    let web_remote = workspace.fixture_remote_with_commit("web");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.declares_repo("web", web_remote.to_str().unwrap());
    workspace.run(&["sync"]);
    workspace.run(&["worktree", "add", "feature-x"]);

    let result = workspace.run(&["worktree", "remove", "feature-x"]);

    assert!(result.success, "expected worktree remove to succeed, stdout={} stderr={}", result.stdout, result.stderr);
    assert!(!workspace.root().join(".worktrees/feature-x").exists(), "expected the whole feature-x worktree directory to be gone");
    // Primary clones must be untouched by removing the worktree copy.
    assert!(workspace.repo("api").exists(), "expected primary api clone to remain");
    assert!(workspace.repo("web").exists(), "expected primary web clone to remain");
}

#[test]
fn worktree_remove_errors_clearly_on_a_name_that_was_never_added() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.run(&["sync"]);
    // Deliberately skip `worktree add` — "feature-x" was never created.

    let result = workspace.run(&["worktree", "remove", "feature-x"]);

    assert!(!result.success, "expected worktree remove to fail for a name that was never added");
    assert!(
        result.stdout.contains("feature-x") || result.stderr.contains("feature-x"),
        "expected the error to name the missing worktree, stdout={} stderr={}",
        result.stdout, result.stderr
    );
}

#[test]
fn worktree_list_is_empty_when_no_worktree_has_ever_been_added() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.run(&["sync"]);
    // Deliberately skip `worktree add` — no `.worktrees/` directory exists yet.

    let result = workspace.run(&["worktree", "list"]);

    assert!(result.success, "expected worktree list to succeed with no worktrees yet, stdout={} stderr={}", result.stdout, result.stderr);
    assert!(result.stdout.trim().is_empty(), "expected no output, got: {}", result.stdout);
}

#[test]
fn worktree_list_reports_a_worktree_missing_a_repo_subdirectory_as_broken() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    let web_remote = workspace.fixture_remote_with_commit("web");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.declares_repo("web", web_remote.to_str().unwrap());
    workspace.run(&["sync"]);
    workspace.run(&["worktree", "add", "feature-x"]);
    // Simulate a partially interrupted worktree by deleting one repo's copy
    // out from under it, without going through `worktree remove`.
    std::fs::remove_dir_all(workspace.root().join(".worktrees/feature-x/web")).expect("remove web's worktree copy");

    let result = workspace.run(&["worktree", "list"]);

    assert!(result.success, "expected worktree list to succeed even with a broken worktree, stdout={} stderr={}", result.stdout, result.stderr);
    assert!(
        result.stdout.contains("feature-x: broken") && result.stdout.contains("web"),
        "expected feature-x to be reported broken, naming web, got: {}",
        result.stdout
    );
}

#[test]
fn worktree_add_fails_clearly_for_a_name_that_already_has_a_worktree() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.run(&["sync"]);
    workspace.run(&["worktree", "add", "feature-x"]);

    let result = workspace.run(&["worktree", "add", "feature-x"]);

    assert!(!result.success, "expected a second worktree add with the same name to fail");
    // The original worktree must survive a failed duplicate attempt.
    assert!(workspace.root().join(".worktrees/feature-x/api").is_dir(), "expected the original worktree to remain intact");
}
