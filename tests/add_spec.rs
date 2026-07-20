mod support;

use support::Workspace;

/// Write an empty `workspaces.toml` so the workspace root can be
/// discovered before the very first repo has been added to it.
fn empty_manifest(workspace: &Workspace) {
    std::fs::write(workspace.root().join("workspaces.toml"), "").expect("write empty manifest");
}
#[test]
fn add_clones_the_repo_and_appends_a_manifest_entry() {
    let workspace = Workspace::new();
    empty_manifest(&workspace);
    let remote = workspace.fixture_remote_with_commit("api");

    let result = workspace.run(&["add", "api", remote.to_str().unwrap()]);

    assert!(
        result.success,
        "expected add to succeed, stdout={} stderr={}",
        result.stdout, result.stderr
    );
    assert!(workspace.repo("api").exists(), "expected api to be cloned onto disk");

    let manifest = std::fs::read_to_string(workspace.root().join("workspaces.toml"))
        .expect("read manifest after add");
    assert!(
        manifest.contains("[repos.api]"),
        "expected manifest to contain the new repo entry, got: {manifest}"
    );
    assert!(
        manifest.contains(remote.to_str().unwrap()),
        "expected manifest to contain the remote, got: {manifest}"
    );
}

#[test]
fn add_records_an_explicit_branch_when_given() {
    let workspace = Workspace::new();
    empty_manifest(&workspace);
    let remote = workspace.fixture_remote_with_commit("api");

    let result = workspace.run(&["add", "api", remote.to_str().unwrap(), "--branch", "main"]);

    assert!(
        result.success,
        "expected add to succeed, stdout={} stderr={}",
        result.stdout, result.stderr
    );

    let manifest = std::fs::read_to_string(workspace.root().join("workspaces.toml"))
        .expect("read manifest after add");
    assert!(
        manifest.contains("branch = \"main\""),
        "expected manifest to record the branch, got: {manifest}"
    );
}

#[test]
fn add_preserves_existing_manifest_entries() {
    let workspace = Workspace::new();
    let web_remote = workspace.fixture_remote_with_commit("web");
    workspace.declares_repo("web", web_remote.to_str().unwrap());
    let api_remote = workspace.fixture_remote_with_commit("api");

    let result = workspace.run(&["add", "api", api_remote.to_str().unwrap()]);

    assert!(
        result.success,
        "expected add to succeed, stdout={} stderr={}",
        result.stdout, result.stderr
    );

    let manifest = std::fs::read_to_string(workspace.root().join("workspaces.toml"))
        .expect("read manifest after add");
    assert!(manifest.contains("[repos.web]"), "expected existing web entry preserved, got: {manifest}");
    assert!(manifest.contains("[repos.api]"), "expected new api entry appended, got: {manifest}");
}

#[test]
fn add_rejects_a_path_already_declared_in_the_manifest() {
    let workspace = Workspace::new();
    let remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", remote.to_str().unwrap());

    let result = workspace.run(&["add", "api", remote.to_str().unwrap()]);

    assert!(!result.success, "expected add to fail for a duplicate path");
    assert!(
        result.stdout.contains("already declared") || result.stderr.contains("already declared"),
        "expected a clear duplicate-entry error, stdout={} stderr={}",
        result.stdout, result.stderr
    );
}

#[test]
fn list_and_sync_reflect_a_repo_added_in_a_prior_process() {
    let workspace = Workspace::new();
    empty_manifest(&workspace);
    let remote = workspace.fixture_remote_with_commit("api");

    let add_result = workspace.run(&["add", "api", remote.to_str().unwrap()]);
    assert!(add_result.success, "expected add to succeed, stderr={}", add_result.stderr);

    let list_result = workspace.run(&["list"]);
    assert!(list_result.success, "expected list to succeed after add");
    assert!(
        list_result.stdout.contains("api"),
        "expected list to reflect the added repo, got: {}",
        list_result.stdout
    );

    // sync on the now-existing repo should be a no-op pull, not a re-clone,
    // and must still succeed end-to-end in a fresh process.
    let sync_result = workspace.run(&["sync"]);
    assert!(
        sync_result.success,
        "expected sync to succeed after add, stdout={} stderr={}",
        sync_result.stdout, sync_result.stderr
    );
}
