//! `compute_status` — decide the per-repo status report.
//!
//! Pure: takes the manifest (for each repo's declared `branch`) and the
//! already-gathered raw git state per repo, and produces the `RepoStatus`
//! rows `status` prints. No git/process calls here — `shell::git` gathers
//! the real `RawRepoState`, this just interprets it.

use std::collections::BTreeMap;

use super::manifest::Manifest;

/// Raw per-repo state as gathered by `shell::git` — current branch, dirty
/// file count, and ahead/behind counts vs upstream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRepoState {
    pub branch: String,
    pub dirty_count: usize,
    pub ahead: u32,
    pub behind: u32,
}

/// The decided status report for one repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoStatus {
    pub name: String,
    pub branch: String,
    pub dirty_count: usize,
    pub ahead: u32,
    pub behind: u32,
    /// Populated only when the checked-out branch differs from the
    /// manifest's declared `branch` field.
    pub note: Option<String>,
}

/// Build one `RepoStatus` per manifest repo present in `raw` (callers
/// narrow by only gathering raw state for the repos they selected).
pub fn compute_status(
    manifest: &Manifest,
    raw: &BTreeMap<String, RawRepoState>,
) -> Vec<RepoStatus> {
    manifest
        .repos
        .iter()
        .filter_map(|(name, spec)| {
            raw.get(name).map(|state| {
                let note = spec
                    .branch
                    .as_ref()
                    .filter(|declared| **declared != state.branch)
                    .map(|declared| format!("expected branch {declared}"));

                RepoStatus {
                    name: name.clone(),
                    branch: state.branch.clone(),
                    dirty_count: state.dirty_count,
                    ahead: state.ahead,
                    behind: state.behind,
                    note,
                }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::manifest::RepoSpec;

    fn manifest_with(repos: &[(&str, Option<&str>)]) -> Manifest {
        let mut map = BTreeMap::new();
        for (name, branch) in repos {
            map.insert(
                name.to_string(),
                RepoSpec {
                    remote: format!("git@github.com:org/{name}.git"),
                    branch: branch.map(str::to_string),
                },
            );
        }
        Manifest { repos: map }
    }

    fn raw(branch: &str, dirty_count: usize, ahead: u32, behind: u32) -> RawRepoState {
        RawRepoState {
            branch: branch.to_string(),
            dirty_count,
            ahead,
            behind,
        }
    }

    #[test]
    fn reports_clean_and_up_to_date() {
        let manifest = manifest_with(&[("api", Some("main"))]);
        let mut states = BTreeMap::new();
        states.insert("api".to_string(), raw("main", 0, 0, 0));

        let statuses = compute_status(&manifest, &states);

        assert_eq!(
            statuses,
            vec![RepoStatus {
                name: "api".to_string(),
                branch: "main".to_string(),
                dirty_count: 0,
                ahead: 0,
                behind: 0,
                note: None,
            }]
        );
    }

    #[test]
    fn reports_dirty_with_the_changed_file_count() {
        let manifest = manifest_with(&[("api", Some("main"))]);
        let mut states = BTreeMap::new();
        states.insert("api".to_string(), raw("main", 3, 0, 0));

        let statuses = compute_status(&manifest, &states);

        assert_eq!(statuses[0].dirty_count, 3);
    }

    #[test]
    fn reports_ahead() {
        let manifest = manifest_with(&[("api", Some("main"))]);
        let mut states = BTreeMap::new();
        states.insert("api".to_string(), raw("main", 0, 2, 0));

        let statuses = compute_status(&manifest, &states);

        assert_eq!(statuses[0].ahead, 2);
        assert_eq!(statuses[0].behind, 0);
    }

    #[test]
    fn reports_behind() {
        let manifest = manifest_with(&[("api", Some("main"))]);
        let mut states = BTreeMap::new();
        states.insert("api".to_string(), raw("main", 0, 0, 1));

        let statuses = compute_status(&manifest, &states);

        assert_eq!(statuses[0].ahead, 0);
        assert_eq!(statuses[0].behind, 1);
    }

    #[test]
    fn reports_ahead_and_behind_when_diverged() {
        let manifest = manifest_with(&[("api", Some("main"))]);
        let mut states = BTreeMap::new();
        states.insert("api".to_string(), raw("main", 0, 2, 1));

        let statuses = compute_status(&manifest, &states);

        assert_eq!(statuses[0].ahead, 2);
        assert_eq!(statuses[0].behind, 1);
    }

    #[test]
    fn adds_a_note_when_checked_out_branch_differs_from_the_manifest() {
        let manifest = manifest_with(&[("api", Some("main"))]);
        let mut states = BTreeMap::new();
        states.insert("api".to_string(), raw("hotfix/234", 0, 0, 0));

        let statuses = compute_status(&manifest, &states);

        assert_eq!(statuses[0].note, Some("expected branch main".to_string()));
    }

    #[test]
    fn no_note_when_manifest_declares_no_branch() {
        let manifest = manifest_with(&[("api", None)]);
        let mut states = BTreeMap::new();
        states.insert("api".to_string(), raw("whatever", 0, 0, 0));

        let statuses = compute_status(&manifest, &states);

        assert_eq!(statuses[0].note, None);
    }

    #[test]
    fn only_includes_repos_present_in_the_raw_state_map() {
        let manifest = manifest_with(&[("api", Some("main")), ("web", Some("main"))]);
        let mut states = BTreeMap::new();
        states.insert("api".to_string(), raw("main", 0, 0, 0));

        let statuses = compute_status(&manifest, &states);

        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].name, "api");
    }
}
