mod support;

use support::Workspace;

#[test]
fn sync_clones_a_repo_declared_in_the_manifest_but_missing_on_disk() {
    let workspace = Workspace::new();
    let remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", remote.to_str().unwrap());

    let result = workspace.run(&["sync"]);

    assert!(
        result.success,
        "expected sync to succeed, stdout={} stderr={}",
        result.stdout, result.stderr
    );
    assert!(
        workspace.repo("api").exists(),
        "expected api to be cloned onto disk"
    );
}

#[test]
fn sync_pulls_an_existing_repo_forward_when_the_remote_has_a_new_commit() {
    let workspace = Workspace::new();
    let remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", remote.to_str().unwrap());

    let first = workspace.run(&["sync"]);
    assert!(first.success, "expected first sync to succeed");
    let commit_before = workspace.repo("api").head_commit();

    workspace.push_new_commit_to_fixture_remote(&remote);

    let second = workspace.run(&["sync"]);
    assert!(
        second.success,
        "expected second sync to succeed, stdout={} stderr={}",
        second.stdout, second.stderr
    );

    let commit_after = workspace.repo("api").head_commit();
    assert_ne!(
        commit_before, commit_after,
        "expected sync to pull the new commit forward"
    );
}

#[test]
fn sync_narrows_to_explicitly_named_repos() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    let web_remote = workspace.fixture_remote_with_commit("web");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.declares_repo("web", web_remote.to_str().unwrap());

    let result = workspace.run(&["sync", "api"]);

    assert!(
        result.success,
        "expected sync to succeed, stdout={} stderr={}",
        result.stdout, result.stderr
    );
    assert!(workspace.repo("api").exists(), "expected api to be cloned");
    assert!(
        !workspace.repo("web").exists(),
        "expected web to be left untouched by narrowed sync"
    );
}

#[test]
fn sync_rejects_a_repo_named_more_than_once() {
    let workspace = Workspace::new();
    let remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", remote.to_str().unwrap());

    let result = workspace.run(&["sync", "api", "api"]);

    assert!(!result.success, "expected sync to reject a repo named more than once");
    assert!(
        result.stdout.contains("more than once") || result.stderr.contains("more than once"),
        "expected a clear duplicate-name error, stdout={} stderr={}",
        result.stdout, result.stderr
    );
}

#[test]
fn sync_keeps_the_workspace_roots_own_git_status_clean_via_the_managed_gitignore() {
    let workspace = Workspace::new();
    let remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", remote.to_str().unwrap());
    workspace.init_as_git_repo();

    let first = workspace.run(&["sync"]);
    assert!(
        first.success,
        "expected first sync to succeed, stdout={} stderr={}",
        first.stdout, first.stderr
    );

    // First sync writes a fresh .gitignore — legitimately untracked until
    // the user commits it themselves, same as any newly created file.
    workspace.commit_all("commit managed gitignore");

    // A second sync (no new clones, just a pull) must not reintroduce any
    // untracked or modified state: the managed block is already correct.
    workspace.push_new_commit_to_fixture_remote(&remote);
    let second = workspace.run(&["sync"]);
    assert!(
        second.success,
        "expected second sync to succeed, stdout={} stderr={}",
        second.stdout, second.stderr
    );

    let status = workspace.git_status_porcelain();
    assert!(
        status.trim().is_empty(),
        "expected clean git status after sync, got: {status}"
    );
}

#[test]
fn sync_ensures_gitignore_even_when_nothing_new_to_clone() {
    let workspace = Workspace::new();
    let remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", remote.to_str().unwrap());
    workspace.init_as_git_repo();

    let first = workspace.run(&["sync"]);
    assert!(first.success, "expected first sync to clone api, stderr={}", first.stderr);

    // Simulate a workspace whose .gitignore predates this clone (e.g. it was
    // added outside of sync's clone path) by removing the managed file.
    std::fs::remove_file(workspace.root().join(".gitignore"))
        .expect("remove gitignore to simulate a workspace missing it");

    // Second sync finds api already on disk — only a Pull action, no Clone
    // action — and must still (re)write the managed .gitignore block rather
    // than gating that step on having cloned something new in this call.
    let second = workspace.run(&["sync"]);
    assert!(
        second.success,
        "expected second sync to succeed, stdout={} stderr={}",
        second.stdout, second.stderr
    );

    let gitignore = std::fs::read_to_string(workspace.root().join(".gitignore"))
        .expect("expected sync to (re)write .gitignore even with nothing new to clone");
    assert!(
        gitignore.contains("/api"),
        "expected the managed block to list api, got: {gitignore}"
    );
}

#[test]
fn sync_reports_an_unreachable_remote_as_a_per_repo_error_and_still_syncs_the_rest() {
    let workspace = Workspace::new();
    let web_remote = workspace.fixture_remote_with_commit("web");
    workspace.declares_repo("api", "/tmp/definitely-does-not-exist-git-multirepo-fixture.git");
    workspace.declares_repo("web", web_remote.to_str().unwrap());

    let result = workspace.run(&["sync"]);

    assert!(!result.success, "expected overall sync to fail because api's remote is unreachable");
    assert!(
        result.stdout.contains("api: error:"),
        "expected api's clone failure to be reported, got: {}",
        result.stdout
    );
    assert!(workspace.repo("web").exists(), "expected web to still be cloned despite api's failure");
}

#[test]
fn sync_reports_a_diverged_repo_as_a_per_repo_error_without_fabricating_a_merge() {
    let workspace = Workspace::new();
    let remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", remote.to_str().unwrap());
    workspace.run(&["sync"]);
    // Diverge: a local commit not yet pushed, and a new commit on the
    // remote it never saw — `pull --ff-only` refuses this by design.
    workspace.repo("api").commit_new_file("a local change");
    workspace.push_new_commit_to_fixture_remote(&remote);
    let local_head_before = workspace.repo("api").head_commit();

    let result = workspace.run(&["sync"]);

    assert!(!result.success, "expected sync to fail on a diverged repo rather than merge");
    assert!(
        result.stdout.contains("api: error:"),
        "expected the diverged repo's failure to be reported, got: {}",
        result.stdout
    );
    assert_eq!(
        workspace.repo("api").head_commit(), local_head_before,
        "expected sync to leave the repo exactly where it was, not fabricate a merge commit"
    );
}
