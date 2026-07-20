use std::collections::BTreeMap;

use serde::Deserialize;

/// A parsed `workspaces.toml`, keyed by repo name.
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
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Malformed(msg) => write!(f, "malformed workspaces.toml: {msg}"),
            ManifestError::MissingRemote(name) => {
                write!(f, "repo \"{name}\" is missing a required `remote` field")
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
}
