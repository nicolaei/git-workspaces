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
        result.stdout.contains("multirepo.toml"),
        "expected error to mention multirepo.toml, got: {}",
        result.stdout
    );
}

#[test]
fn list_succeeds_with_empty_output_for_an_empty_manifest() {
    let workspace = Workspace::new();
    std::fs::write(workspace.root().join("multirepo.toml"), "").expect("write empty manifest");

    let result = workspace.run(&["list"]);

    assert!(result.success, "expected list to succeed on an empty manifest, stderr={}", result.stderr);
    assert!(result.stdout.trim().is_empty(), "expected no output, got: {}", result.stdout);
}

#[test]
fn list_reports_a_clear_error_on_malformed_toml() {
    let workspace = Workspace::new();
    std::fs::write(workspace.root().join("multirepo.toml"), "this is not [ valid toml").expect("write malformed manifest");

    let result = workspace.run(&["list"]);

    assert!(!result.success, "expected list to fail on malformed toml");
    assert!(
        result.stdout.contains("multirepo.toml"),
        "expected the error to reference the manifest file, got: {}",
        result.stdout
    );
}
