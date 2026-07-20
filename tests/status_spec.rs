mod support;

use support::Workspace;

#[test]
fn status_reports_clean_and_up_to_date_for_freshly_synced_repos() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    let web_remote = workspace.fixture_remote_with_commit("web");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.declares_repo("web", web_remote.to_str().unwrap());
    workspace.run(&["sync"]);

    let result = workspace.run(&["status"]);

    assert!(
        result.success,
        "expected status to succeed, stdout={} stderr={}",
        result.stdout, result.stderr
    );
    let lines: Vec<&str> = result.stdout.lines().collect();
    assert_eq!(lines[0], "REPO  BRANCH  STATE  SYNC");
    assert_eq!(lines[1], "api   main    clean  up to date");
    assert_eq!(lines[2], "web   main    clean  up to date");
}

#[test]
fn status_reports_dirty_with_changed_file_count() {
    let workspace = Workspace::new();
    let remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", remote.to_str().unwrap());
    workspace.run(&["sync"]);
    workspace.repo("api").make_dirty();

    let result = workspace.run(&["status"]);

    assert!(result.success, "expected status to succeed, stderr={}", result.stderr);
    assert!(
        result.stdout.contains("dirty (1)"),
        "expected dirty (1) in output, got: {}",
        result.stdout
    );
}

#[test]
fn status_reports_ahead_when_local_commits_are_not_yet_pushed() {
    let workspace = Workspace::new();
    let remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", remote.to_str().unwrap());
    workspace.run(&["sync"]);
    workspace.repo("api").commit_new_file("a local change");

    let result = workspace.run(&["status"]);

    assert!(result.success, "expected status to succeed, stderr={}", result.stderr);
    assert!(
        result.stdout.contains("ahead 1"),
        "expected ahead 1 in output, got: {}",
        result.stdout
    );
}

#[test]
fn status_reports_behind_when_the_remote_has_new_commits() {
    let workspace = Workspace::new();
    let remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", remote.to_str().unwrap());
    workspace.run(&["sync"]);
    workspace.push_new_commit_to_fixture_remote(&remote);

    let result = workspace.run(&["status"]);

    assert!(result.success, "expected status to succeed, stderr={}", result.stderr);
    assert!(
        result.stdout.contains("behind 1"),
        "expected behind 1 in output, got: {}",
        result.stdout
    );
}

#[test]
fn status_reports_ahead_and_behind_when_diverged() {
    let workspace = Workspace::new();
    let remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", remote.to_str().unwrap());
    workspace.run(&["sync"]);
    workspace.repo("api").commit_new_file("a local change");
    workspace.push_new_commit_to_fixture_remote(&remote);

    let result = workspace.run(&["status"]);

    assert!(result.success, "expected status to succeed, stderr={}", result.stderr);
    assert!(
        result.stdout.contains("ahead 1, behind 1"),
        "expected ahead 1, behind 1 in output, got: {}",
        result.stdout
    );
}

#[test]
fn status_reports_a_note_when_the_checked_out_branch_differs_from_the_manifest() {
    let workspace = Workspace::new();
    let remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo_with_branch("api", remote.to_str().unwrap(), "main");
    workspace.run(&["sync"]);
    workspace.repo("api").checkout_new_branch("hotfix/234");

    let result = workspace.run(&["status"]);

    assert!(result.success, "expected status to succeed, stderr={}", result.stderr);
    let lines: Vec<&str> = result.stdout.lines().collect();
    assert!(
        lines[0].contains("NOTE"),
        "expected a NOTE column header, got: {}",
        lines[0]
    );
    assert!(
        result.stdout.contains("expected branch main"),
        "expected branch-mismatch note, got: {}",
        result.stdout
    );
}

#[test]
fn status_narrows_to_an_explicitly_named_repo() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    let web_remote = workspace.fixture_remote_with_commit("web");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.declares_repo("web", web_remote.to_str().unwrap());
    workspace.run(&["sync"]);

    let result = workspace.run(&["status", "api"]);

    assert!(result.success, "expected status to succeed, stderr={}", result.stderr);
    assert!(result.stdout.contains("api"), "expected api in output, got: {}", result.stdout);
    assert!(!result.stdout.contains("web"), "expected web to be excluded, got: {}", result.stdout);
}

#[test]
fn status_errors_on_an_unknown_repo_name() {
    let workspace = Workspace::new();
    let remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", remote.to_str().unwrap());
    workspace.run(&["sync"]);

    let result = workspace.run(&["status", "nonexistent"]);

    assert!(!result.success, "expected status to fail for an unknown repo");
    assert!(
        result.stdout.contains("nonexistent"),
        "expected error to name the unknown repo, got: {}",
        result.stdout
    );
}

#[test]
fn status_errors_clearly_on_a_repo_that_has_never_been_synced() {
    let workspace = Workspace::new();
    let remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", remote.to_str().unwrap());
    // Deliberately skip `sync` — api is declared but never cloned onto disk.

    let result = workspace.run(&["status"]);

    assert!(!result.success, "expected status to fail for a repo that was never cloned");
    assert!(
        result.stdout.contains("api"),
        "expected the error to name the uncloned repo, got: {}",
        result.stdout
    );
}
