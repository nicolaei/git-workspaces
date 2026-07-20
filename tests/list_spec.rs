mod support;

use support::Workspace;

#[test]
fn list_prints_every_declared_repo_when_run_from_a_nested_subfolder() {
    let workspace = Workspace::new();
    workspace.declares_repo("api", "git@github.com:org/api.git");
    workspace.declares_repo("web", "git@github.com:org/web.git");

    let nested = workspace.subfolder("some/nested/folder");
    let result = workspace.run_from(&nested, &["list"]);

    assert!(
        result.success,
        "expected list to succeed, stdout={} stderr={}",
        result.stdout, result.stderr
    );
    assert!(
        result.stdout.contains("api"),
        "expected 'api' in output, got: {}",
        result.stdout
    );
    assert!(
        result.stdout.contains("web"),
        "expected 'web' in output, got: {}",
        result.stdout
    );
}

#[test]
fn list_reports_a_clear_error_when_no_manifest_is_found() {
    let workspace = Workspace::new();

    let result = workspace.run(&["list"]);

    assert!(!result.success, "expected list to fail with no manifest");
    assert!(
        result.stdout.contains("workspaces.toml"),
        "expected error to mention workspaces.toml, got: {}",
        result.stdout
    );
}
