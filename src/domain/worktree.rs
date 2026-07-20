//! `worktree_layout` — compute the on-disk layout for one named worktree.
//!
//! Whole-fleet only: a worktree is one named copy of the *entire* manifest,
//! never a subset (corrected from an earlier per-repo-selectable design —
//! logged decision, story H). There is deliberately no selection parameter
//! here, unlike `select::resolve_targets`.

use std::path::PathBuf;

use super::manifest::Manifest;

/// For every repo in the manifest, the path (relative to the workspace
/// root) where its copy of a named worktree lives:
/// `.worktrees/<name>/<repo>`.
pub fn worktree_layout(manifest: &Manifest, name: &str) -> Vec<(String, PathBuf)> {
    manifest
        .repos
        .keys()
        .map(|repo_name| {
            let path = PathBuf::from(".worktrees").join(name).join(repo_name);
            (repo_name.clone(), path)
        })
        .collect()
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
    fn computes_a_path_per_repo_under_dot_worktrees_slash_name() {
        let manifest = manifest_with(&["api", "web"]);

        let layout = worktree_layout(&manifest, "feature-x");

        assert_eq!(
            layout,
            vec![
                ("api".to_string(), PathBuf::from(".worktrees/feature-x/api")),
                ("web".to_string(), PathBuf::from(".worktrees/feature-x/web")),
            ]
        );
    }

    #[test]
    fn covers_every_repo_in_the_manifest_unconditionally() {
        let manifest = manifest_with(&["api", "web", "infra"]);

        let layout = worktree_layout(&manifest, "any-name");

        assert_eq!(layout.len(), 3, "expected every manifest repo to appear, no selection");
    }

    #[test]
    fn is_empty_for_an_empty_manifest() {
        let manifest = manifest_with(&[]);

        let layout = worktree_layout(&manifest, "feature-x");

        assert!(layout.is_empty());
    }
}
