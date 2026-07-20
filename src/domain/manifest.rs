use std::collections::BTreeMap;

use serde::Deserialize;

/// A parsed `multirepo.toml`, keyed by repo name.
///
/// Repos are stored in a `BTreeMap` so iteration is always sorted by name —
/// callers that need deterministic output (e.g. `list`) get it for free.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub repos: BTreeMap<String, RepoSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSpec {
    pub remote: String,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// The TOML itself doesn't parse.
    Malformed(String),
    /// A `[repos.<name>]` table is missing the required `remote` field.
    MissingRemote(String),
    /// `add_repo` was asked to register a name already present in the manifest.
    DuplicateRepo(String),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Malformed(msg) => write!(f, "malformed multirepo.toml: {msg}"),
            ManifestError::MissingRemote(name) => {
                write!(f, "repo \"{name}\" is missing a required `remote` field")
            }
            ManifestError::DuplicateRepo(name) => {
                write!(f, "\"{name}\" is already declared in multirepo.toml")
            }
        }
    }
}

impl std::error::Error for ManifestError {}

#[derive(Deserialize)]
struct RawManifest {
    #[serde(default)]
    repos: BTreeMap<String, RawRepoSpec>,
}

#[derive(Deserialize)]
struct RawRepoSpec {
    remote: Option<String>,
    branch: Option<String>,
}

pub fn parse_manifest(contents: &str) -> Result<Manifest, ManifestError> {
    let raw: RawManifest =
        toml::from_str(contents).map_err(|e| ManifestError::Malformed(e.to_string()))?;

    let mut repos = BTreeMap::new();
    for (name, raw_spec) in raw.repos {
        let remote = raw_spec
            .remote
            .ok_or_else(|| ManifestError::MissingRemote(name.clone()))?;
        repos.insert(
            name,
            RepoSpec {
                remote,
                branch: raw_spec.branch,
            },
        );
    }

    Ok(Manifest { repos })
}

/// Register a new repo entry, returning an updated `Manifest` with it
/// appended. Rejects a `name` already present — callers (the `add` command)
/// use that name as both the manifest key and the on-disk path, so a
/// collision there means "already declared", never a silent overwrite.
pub fn add_repo(
    manifest: &Manifest,
    name: &str,
    remote: &str,
    branch: Option<String>,
) -> Result<Manifest, ManifestError> {
    if manifest.repos.contains_key(name) {
        return Err(ManifestError::DuplicateRepo(name.to_string()));
    }

    let mut repos = manifest.repos.clone();
    repos.insert(
        name.to_string(),
        RepoSpec {
            remote: remote.to_string(),
            branch,
        },
    );
    Ok(Manifest { repos })
}

/// Serialize a `Manifest` back to `multirepo.toml` text. Round-trips
/// through the parsed `Manifest` rather than editing raw TOML in place —
/// this manifest is small and entirely machine-managed, so byte-for-byte
/// formatting/comment preservation isn't a requirement (logged decision,
/// story F). `BTreeMap` iteration keeps output sorted and deterministic.
pub fn serialize_manifest(manifest: &Manifest) -> String {
    let mut out = String::new();
    for (name, spec) in &manifest.repos {
        out.push_str(&format!("[repos.{name}]\n"));
        out.push_str(&format!("remote = \"{}\"\n", spec.remote));
        if let Some(branch) = &spec.branch {
            out.push_str(&format!("branch = \"{branch}\"\n"));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_valid_manifest_with_multiple_repos() {
        let toml = r#"
            [repos.api]
            remote = "git@github.com:org/api.git"
            branch = "main"

            [repos.web]
            remote = "git@github.com:org/web.git"
        "#;

        let manifest = parse_manifest(toml).expect("valid manifest should parse");

        assert_eq!(manifest.repos.len(), 2);
        assert_eq!(
            manifest.repos["api"],
            RepoSpec {
                remote: "git@github.com:org/api.git".to_string(),
                branch: Some("main".to_string()),
            }
        );
        assert_eq!(
            manifest.repos["web"],
            RepoSpec {
                remote: "git@github.com:org/web.git".to_string(),
                branch: None,
            }
        );
    }

    #[test]
    fn rejects_a_repo_missing_its_required_remote() {
        let toml = r#"
            [repos.api]
            branch = "main"
        "#;

        let err = parse_manifest(toml).expect_err("missing remote should be rejected");
        assert_eq!(err, ManifestError::MissingRemote("api".to_string()));
    }

    #[test]
    fn rejects_malformed_toml() {
        let toml = "this is not [ valid toml";
        let err = parse_manifest(toml).expect_err("malformed toml should be rejected");
        assert!(matches!(err, ManifestError::Malformed(_)));
    }

    #[test]
    fn add_repo_appends_a_new_entry_and_preserves_existing_ones() {
        let mut repos = BTreeMap::new();
        repos.insert(
            "web".to_string(),
            RepoSpec {
                remote: "git@github.com:org/web.git".to_string(),
                branch: None,
            },
        );
        let manifest = Manifest { repos };

        let updated = add_repo(&manifest, "api", "git@github.com:org/api.git", None)
            .expect("add_repo should succeed for a new name");

        assert_eq!(updated.repos.len(), 2);
        assert!(updated.repos.contains_key("web"), "expected existing entry preserved");
        assert_eq!(
            updated.repos["api"],
            RepoSpec {
                remote: "git@github.com:org/api.git".to_string(),
                branch: None,
            }
        );
    }

    #[test]
    fn add_repo_records_an_explicit_branch() {
        let manifest = Manifest { repos: BTreeMap::new() };

        let updated = add_repo(&manifest, "api", "git@github.com:org/api.git", Some("main".to_string()))
            .expect("add_repo should succeed");

        assert_eq!(updated.repos["api"].branch, Some("main".to_string()));
    }

    #[test]
    fn add_repo_rejects_a_name_already_present() {
        let mut repos = BTreeMap::new();
        repos.insert(
            "api".to_string(),
            RepoSpec {
                remote: "git@github.com:org/api.git".to_string(),
                branch: None,
            },
        );
        let manifest = Manifest { repos };

        let err = add_repo(&manifest, "api", "git@github.com:org/other.git", None)
            .expect_err("add_repo should reject a duplicate name");

        assert_eq!(err, ManifestError::DuplicateRepo("api".to_string()));
    }

    #[test]
    fn serialize_manifest_round_trips_through_parse_manifest() {
        let toml = r#"
            [repos.api]
            remote = "git@github.com:org/api.git"
            branch = "main"

            [repos.web]
            remote = "git@github.com:org/web.git"
        "#;
        let manifest = parse_manifest(toml).expect("valid manifest should parse");

        let serialized = serialize_manifest(&manifest);
        let reparsed = parse_manifest(&serialized).expect("serialized manifest should reparse");

        assert_eq!(reparsed, manifest);
    }
}
