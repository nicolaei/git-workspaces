mod support;

use support::Workspace;

#[test]
fn checkout_moves_every_repo_to_an_existing_branch_by_default() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    let web_remote = workspace.fixture_remote_with_commit("web");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.declares_repo("web", web_remote.to_str().unwrap());
    workspace.run(&["sync"]);
    // Create "feature" as a real branch in both fixture remotes' clones so
    // checkout (without --create) has an existing branch to move to.
    workspace.repo("api").checkout_new_branch("feature");
    workspace.repo("web").checkout_new_branch("feature");
    // Back to main so the checkout command is the one doing the work.
    workspace.repo("api").checkout_existing_branch("main");
    workspace.repo("web").checkout_existing_branch("main");

    let result = workspace.run(&["checkout", "feature"]);

    assert!(result.success, "expected checkout to succeed, stdout={} stderr={}", result.stdout, result.stderr);
    assert_eq!(workspace.repo("api").current_branch(), "feature");
    assert_eq!(workspace.repo("web").current_branch(), "feature");
}

#[test]
fn checkout_narrows_to_an_explicitly_named_repo_leaving_others_untouched() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    let web_remote = workspace.fixture_remote_with_commit("web");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.declares_repo("web", web_remote.to_str().unwrap());
    workspace.run(&["sync"]);
    workspace.repo("api").checkout_new_branch("feature");
    workspace.repo("web").checkout_new_branch("feature");
    workspace.repo("api").checkout_existing_branch("main");
    workspace.repo("web").checkout_existing_branch("main");

    let result = workspace.run(&["checkout", "feature", "api"]);

    assert!(result.success, "expected checkout to succeed, stdout={} stderr={}", result.stdout, result.stderr);
    assert_eq!(workspace.repo("api").current_branch(), "feature");
    assert_eq!(workspace.repo("web").current_branch(), "main", "expected web to be untouched by narrowing");
}

#[test]
fn checkout_create_makes_a_brand_new_branch_across_selected_repos() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    let web_remote = workspace.fixture_remote_with_commit("web");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.declares_repo("web", web_remote.to_str().unwrap());
    workspace.run(&["sync"]);

    let result = workspace.run(&["checkout", "new-feature", "--create"]);

    assert!(result.success, "expected checkout --create to succeed, stdout={} stderr={}", result.stdout, result.stderr);
    assert_eq!(workspace.repo("api").current_branch(), "new-feature");
    assert_eq!(workspace.repo("web").current_branch(), "new-feature");
}

#[test]
fn checkout_create_fails_clearly_when_the_branch_already_exists_in_a_repo() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.run(&["sync"]);
    workspace.repo("api").checkout_new_branch("feature");
    workspace.repo("api").checkout_existing_branch("main");

    let result = workspace.run(&["checkout", "feature", "--create", "api"]);

    assert!(!result.success, "expected checkout --create to fail on an already-existing branch");
    assert!(
        result.stdout.contains("api: error:"),
        "expected a clear per-repo error, got: {}",
        result.stdout
    );
    // The repo must stay exactly where it was — no silent checkout, no
    // corrupted half-applied state.
    assert_eq!(workspace.repo("api").current_branch(), "main");
}

#[test]
fn checkout_without_create_fails_clearly_when_the_branch_does_not_exist() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.run(&["sync"]);

    let result = workspace.run(&["checkout", "does-not-exist"]);

    assert!(!result.success, "expected checkout to fail for a nonexistent branch without --create");
    assert!(
        result.stdout.contains("api: error:"),
        "expected a clear per-repo error, got: {}",
        result.stdout
    );
    assert_eq!(workspace.repo("api").current_branch(), "main");
}

#[test]
fn checkout_errors_on_an_unknown_repo_name() {
    let workspace = Workspace::new();
    let remote = workspace.fixture_remote_with_commit("api");
    workspace.declares_repo("api", remote.to_str().unwrap());
    workspace.run(&["sync"]);

    let result = workspace.run(&["checkout", "feature", "nonexistent"]);

    assert!(!result.success, "expected checkout to fail for an unknown repo");
    assert!(
        result.stdout.contains("nonexistent") || result.stderr.contains("nonexistent"),
        "expected error to name the unknown repo, got: {}",
        result.stdout
    );
}

#[test]
fn checkout_reports_every_repos_outcome_when_some_have_the_branch_and_some_dont() {
    let workspace = Workspace::new();
    let api_remote = workspace.fixture_remote_with_commit("api");
    let web_remote = workspace.fixture_remote_with_commit("web");
    workspace.declares_repo("api", api_remote.to_str().unwrap());
    workspace.declares_repo("web", web_remote.to_str().unwrap());
    workspace.run(&["sync"]);
    // Only api has "feature" as a real branch — web never gets it.
    workspace.repo("api").checkout_new_branch("feature");
    workspace.repo("api").checkout_existing_branch("main");

    let result = workspace.run(&["checkout", "feature"]);

    assert!(!result.success, "expected overall checkout to fail because web lacks the branch");
    assert_eq!(workspace.repo("api").current_branch(), "feature", "expected api to still move even though web failed");
    assert_eq!(workspace.repo("web").current_branch(), "main", "expected web to stay put on failure");
    assert!(
        result.stdout.contains("api: checked out feature"),
        "expected api's success to be reported, got: {}",
        result.stdout
    );
    assert!(
        result.stdout.contains("web: error:"),
        "expected web's failure to be reported, got: {}",
        result.stdout
    );
}
