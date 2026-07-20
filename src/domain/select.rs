//! `resolve_targets` — shared explicit narrowing logic.
//!
//! Every fan-out command (`status`, and later `exec`, `checkout`, `worktree`)
//! takes an optional list of repo names to narrow to. Empty means "whole
//! workspace"; non-empty means exactly those repos, and any name that isn't
//! declared in the manifest is an error. Pure: no I/O, generic over the
//! caller's command — nothing status-specific baked in here.

use std::collections::HashSet;

use super::manifest::Manifest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectError {
    /// A named repo isn't declared in the manifest at all.
    UnknownRepo(String),
    /// The same repo name was named more than once.
    DuplicateRepo(String),
}

impl std::fmt::Display for SelectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SelectError::UnknownRepo(name) => {
                write!(f, "unknown repo \"{name}\" in multirepo.toml")
            }
            SelectError::DuplicateRepo(name) => {
                write!(f, "repo \"{name}\" named more than once")
            }
        }
    }
}

impl std::error::Error for SelectError {}

/// Resolve the repo names a command should act on.
///
/// Empty `args` means the whole workspace (every repo in the manifest, in
/// manifest order). Non-empty `args` means exactly those repos — order is
/// preserved as given, duplicates and unknown names are rejected.
pub fn resolve_targets(manifest: &Manifest, args: &[String]) -> Result<Vec<String>, SelectError> {
    if args.is_empty() {
        return Ok(manifest.repos.keys().cloned().collect());
    }

    let mut seen = HashSet::new();
    for name in args {
        if !manifest.repos.contains_key(name) {
            return Err(SelectError::UnknownRepo(name.clone()));
        }
        if !seen.insert(name) {
            return Err(SelectError::DuplicateRepo(name.clone()));
        }
    }

    Ok(args.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::manifest::RepoSpec;
    use std::collections::BTreeMap;

    fn manifest_with(names: &[&str]) -> Manifest {
        let mut repos = BTreeMap::new();
        for name in names {
            repos.insert(
                name.to_string(),
                RepoSpec {
                    remote: format!("git@github.com:org/{name}.git"),
                    branch: None,
                },
            );
        }
        Manifest { repos }
    }

    #[test]
    fn empty_args_resolves_to_the_whole_workspace() {
        let manifest = manifest_with(&["api", "web"]);
        let targets = resolve_targets(&manifest, &[]).expect("should resolve");
        assert_eq!(targets, vec!["api".to_string(), "web".to_string()]);
    }

    #[test]
    fn explicit_args_narrow_to_exactly_those_repos() {
        let manifest = manifest_with(&["api", "web", "infra"]);
        let args = vec!["web".to_string()];
        let targets = resolve_targets(&manifest, &args).expect("should resolve");
        assert_eq!(targets, vec!["web".to_string()]);
    }

    #[test]
    fn unknown_repo_name_is_an_error_naming_the_bad_repo() {
        let manifest = manifest_with(&["api"]);
        let args = vec!["nonexistent".to_string()];
        let err = resolve_targets(&manifest, &args).expect_err("should reject unknown repo");
        assert_eq!(err, SelectError::UnknownRepo("nonexistent".to_string()));
    }

    #[test]
    fn duplicate_repo_name_is_an_error() {
        let manifest = manifest_with(&["api"]);
        let args = vec!["api".to_string(), "api".to_string()];
        let err = resolve_targets(&manifest, &args).expect_err("should reject duplicates");
        assert_eq!(err, SelectError::DuplicateRepo("api".to_string()));
    }

    #[test]
    fn preserves_the_order_the_caller_gave() {
        let manifest = manifest_with(&["api", "web", "infra"]);
        let args = vec!["infra".to_string(), "api".to_string()];
        let targets = resolve_targets(&manifest, &args).expect("should resolve");
        assert_eq!(targets, vec!["infra".to_string(), "api".to_string()]);
    }
}
