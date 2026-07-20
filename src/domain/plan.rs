//! `plan_sync` — decide, per manifest repo, whether to clone or pull.
//!
//! Pure: takes a `Manifest` and a `DiskState` (an injected predicate saying
//! which repo paths already exist), returns a plan. No filesystem or git
//! calls here — `shell` gathers the real `DiskState` and executes the plan.

use super::manifest::Manifest;

/// What to do about one repo in the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAction {
    /// Not present on disk — clone it from `remote` into `path`.
    Clone {
        name: String,
        remote: String,
        path: String,
    },
    /// Already present on disk — pull it forward.
    Pull { name: String, path: String },
}

/// Which repo paths already exist on disk, injected so `plan_sync` stays
/// pure and unit-testable without touching a real filesystem.
pub trait DiskState {
    fn exists(&self, path: &str) -> bool;
}

impl<F: Fn(&str) -> bool> DiskState for F {
    fn exists(&self, path: &str) -> bool {
        self(path)
    }
}

/// Decide a `Clone` or `Pull` action for every repo in the manifest.
///
/// Repo name doubles as its on-disk path relative to the workspace root
/// (see manifest schema decision in the epic's architecture note).
pub fn plan_sync(manifest: &Manifest, disk_state: &impl DiskState) -> Vec<SyncAction> {
    manifest
        .repos
        .iter()
        .map(|(name, spec)| {
            if disk_state.exists(name) {
                SyncAction::Pull {
                    name: name.clone(),
                    path: name.clone(),
                }
            } else {
                SyncAction::Clone {
                    name: name.clone(),
                    remote: spec.remote.clone(),
                    path: name.clone(),
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::manifest::RepoSpec;
    use std::collections::BTreeMap;

    fn manifest_with(repos: &[(&str, &str)]) -> Manifest {
        let mut map = BTreeMap::new();
        for (name, remote) in repos {
            map.insert(
                name.to_string(),
                RepoSpec {
                    remote: remote.to_string(),
                    branch: None,
                },
            );
        }
        Manifest { repos: map }
    }

    #[test]
    fn plans_a_clone_for_a_repo_missing_on_disk() {
        let manifest = manifest_with(&[("api", "git@github.com:org/api.git")]);
        let actions = plan_sync(&manifest, &|_: &str| false);

        assert_eq!(
            actions,
            vec![SyncAction::Clone {
                name: "api".to_string(),
                remote: "git@github.com:org/api.git".to_string(),
                path: "api".to_string(),
            }]
        );
    }

    #[test]
    fn plans_a_pull_for_a_repo_already_present_on_disk() {
        let manifest = manifest_with(&[("api", "git@github.com:org/api.git")]);
        let actions = plan_sync(&manifest, &|_: &str| true);

        assert_eq!(
            actions,
            vec![SyncAction::Pull {
                name: "api".to_string(),
                path: "api".to_string(),
            }]
        );
    }

    #[test]
    fn plans_a_mix_of_clone_and_pull_across_a_manifest() {
        let manifest = manifest_with(&[
            ("api", "git@github.com:org/api.git"),
            ("web", "git@github.com:org/web.git"),
        ]);
        let actions = plan_sync(&manifest, &|name: &str| name == "api");

        assert_eq!(
            actions,
            vec![
                SyncAction::Pull {
                    name: "api".to_string(),
                    path: "api".to_string(),
                },
                SyncAction::Clone {
                    name: "web".to_string(),
                    remote: "git@github.com:org/web.git".to_string(),
                    path: "web".to_string(),
                },
            ]
        );
    }
}
