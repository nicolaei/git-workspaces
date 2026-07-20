mod support;

use support::Workspace;

#[test]
fn exec_runs_the_command_in_every_repo_and_reports_a_per_repo_exit_summary() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    let web_remote = workspace.fixture_remote_with_commit("web");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.declares_repo("web", web_remote.to_str().unwrap());
    workspace.run(&["sync"]);
    // Only "api" has this file — a command that depends on it succeeds in
    // api and fails in web, proving both actually ran rather than one being
    // skipped after the other's failure.
    workspace.repo("api").write_file("marker.txt", "present");

    let result = workspace.run(&["exec", "--", "test", "-f", "marker.txt"]);

    assert!(!result.success, "expected overall exec to fail because one repo failed");
    assert!(
        result.stdout.contains("api: exit 0"),
        "expected api to succeed, got: {}",
        result.stdout
    );
    assert!(
        result.stdout.contains("web: exit 1"),
        "expected web to fail, got: {}",
        result.stdout
    );
}

#[test]
fn exec_narrows_to_an_explicitly_named_repo() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    let web_remote = workspace.fixture_remote_with_commit("web");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.declares_repo("web", web_remote.to_str().unwrap());
    workspace.run(&["sync"]);

    let result = workspace.run(&["exec", "api", "--", "true"]);

    assert!(result.success, "expected exec to succeed, stdout={} stderr={}", result.stdout, result.stderr);
    assert!(result.stdout.contains("api: exit 0"), "expected api in summary, got: {}", result.stdout);
    assert!(!result.stdout.contains("web"), "expected web to be excluded, got: {}", result.stdout);
}

#[test]
fn exec_errors_on_an_unknown_repo_name() {
    let workspace = Workspace::new();
    let remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", remote.to_str().unwrap());
    workspace.run(&["sync"]);

    let result = workspace.run(&["exec", "nonexistent", "--", "true"]);

    assert!(!result.success, "expected exec to fail for an unknown repo");
    assert!(
        result.stdout.contains("nonexistent"),
        "expected error to name the unknown repo, got: {}",
        result.stdout
    );
}

#[test]
fn exec_parallel_still_attributes_results_to_the_correct_repo() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    let web_remote = workspace.fixture_remote_with_commit("web");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.declares_repo("web", web_remote.to_str().unwrap());
    workspace.run(&["sync"]);
    workspace.repo("api").write_file("marker.txt", "present");

    let result = workspace.run(&["exec", "--parallel", "--", "test", "-f", "marker.txt"]);

    assert!(!result.success, "expected overall exec to fail because one repo failed");
    assert!(
        result.stdout.contains("api: exit 0"),
        "expected api to succeed under --parallel, got: {}",
        result.stdout
    );
    assert!(
        result.stdout.contains("web: exit 1"),
        "expected web to fail under --parallel, got: {}",
        result.stdout
    );
}
