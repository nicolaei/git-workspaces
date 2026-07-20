use std::io;
use std::path::Path;

/// Real filesystem check for `domain::discover::find_workspace_root`'s
/// injected `exists` predicate.
pub fn exists(path: &Path) -> bool {
    path.exists()
}

/// Real filesystem read for the manifest contents.
pub fn read_to_string(path: &Path) -> io::Result<String> {
    std::fs::read_to_string(path)
}

/// Real filesystem write for the manifest contents (used by `add` to
/// persist an updated `multirepo.toml`).
pub fn write_string(path: &Path, contents: &str) -> io::Result<()> {
    std::fs::write(path, contents)
}

/// Outcome of `write_manifest_if_unchanged`: whether the write happened, or
/// was refused because the file no longer matches the caller's baseline.
#[derive(Debug, PartialEq, Eq)]
pub enum ManifestWriteOutcome {
    Written,
    ConcurrentModification,
}

/// Optimistic-concurrency guard for `add` (the only command that reads,
/// modifies, and rewrites `multirepo.toml`). Re-reads the file immediately
/// before writing and compares it byte-for-byte against `baseline` — the
/// raw text the caller loaded at the start of its command. A mismatch means
/// something else (a concurrently running `add`) wrote to the file in
/// between, so writing `contents` now would silently discard that other
/// change. Refuses the write and reports `ConcurrentModification` instead
/// of clobbering it.
///
/// `baseline` must be the exact raw text read from disk, not a
/// re-serialization of the parsed manifest — comparing against a
/// re-serialized form would false-positive on any formatting the parser
/// normalizes away (comments, key order, whitespace).
pub fn write_manifest_if_unchanged(
    path: &Path,
    baseline: &str,
    contents: &str,
) -> io::Result<ManifestWriteOutcome> {
    let current = std::fs::read_to_string(path)?;
    if current != baseline {
        return Ok(ManifestWriteOutcome::ConcurrentModification);
    }
    std::fs::write(path, contents)?;
    Ok(ManifestWriteOutcome::Written)
}

/// Real recursive directory creation (used by `worktree add` to ensure a
/// target's parent directories exist before git creates the worktree
/// itself).
pub fn create_dir_all(path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)
}

/// Real recursive directory removal (used by `worktree remove` to delete
/// the now-empty `.worktrees/<name>/` directory, including the manifest
/// copy inside it).
pub fn remove_dir_all(path: &Path) -> io::Result<()> {
    std::fs::remove_dir_all(path)
}

/// Names of the direct subdirectories of `path`, sorted, non-directory
/// entries skipped. Returns an empty list if `path` doesn't exist at all —
/// `worktree list` on a workspace with no `.worktrees/` yet is empty, not
/// an error.
pub fn list_subdirectory_names(path: &Path) -> io::Result<Vec<String>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut names: Vec<String> = std::fs::read_dir(path)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();
    Ok(names)
}

const BEGIN_MARKER: &str = "# >>> git multirepo managed (do not edit below) >>>";
const END_MARKER: &str = "# <<< git multirepo managed <<<";

/// Regenerate the managed block in `<workspace_root>/.gitignore` so every
/// cloned repo path is ignored by the workspace root's own git repo.
///
/// Only the marked block is replaced; everything else in the file is left
/// untouched. No-op if `<workspace_root>/.git` doesn't exist — the
/// workspace root isn't itself a git repo, so there's nothing to ignore
/// anything from.
pub fn ensure_gitignored(workspace_root: &Path, repo_paths: &[String], extra_paths: &[&str]) -> io::Result<()> {
    if !workspace_root.join(".git").exists() {
        return Ok(());
    }

    let gitignore_path = workspace_root.join(".gitignore");
    let existing = std::fs::read_to_string(&gitignore_path).unwrap_or_default();

    let before = section_before_marker(&existing);
    let after = section_after_marker(&existing);

    let mut content = String::new();
    content.push_str(&before);
    if !before.is_empty() && !before.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(BEGIN_MARKER);
    content.push('\n');
    for path in repo_paths.iter().map(String::as_str).chain(extra_paths.iter().copied()) {
        content.push('/');
        content.push_str(path);
        content.push('\n');
    }
    content.push_str(END_MARKER);
    content.push('\n');
    content.push_str(&after);

    std::fs::write(&gitignore_path, content)
}

/// Everything before the managed block, unchanged.
fn section_before_marker(existing: &str) -> String {
    match existing.find(BEGIN_MARKER) {
        Some(idx) => existing[..idx].to_string(),
        None => existing.to_string(),
    }
}

/// Everything after the managed block, unchanged.
fn section_after_marker(existing: &str) -> String {
    match existing.find(END_MARKER) {
        Some(idx) => {
            let after_marker = idx + END_MARKER.len();
            let rest = &existing[after_marker..];
            rest.strip_prefix('\n').unwrap_or(rest).to_string()
        }
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_managed_block_when_gitignore_is_absent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        ensure_gitignored(dir.path(), &["api".to_string(), "web".to_string()], &[]).unwrap();

        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert_eq!(
            content,
            "# >>> git multirepo managed (do not edit below) >>>\n/api\n/web\n# <<< git multirepo managed <<<\n"
        );
    }

    #[test]
    fn appends_extra_literal_entries_after_the_repo_paths() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        ensure_gitignored(dir.path(), &["api".to_string()], &[".worktrees"]).unwrap();

        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert_eq!(
            content,
            "# >>> git multirepo managed (do not edit below) >>>\n/api\n/.worktrees\n# <<< git multirepo managed <<<\n"
        );
    }

    #[test]
    fn preserves_unrelated_content_around_the_managed_block() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(
            dir.path().join(".gitignore"),
            "*.log\n# >>> git multirepo managed (do not edit below) >>>\n/old\n# <<< git multirepo managed <<<\ntarget/\n",
        )
        .unwrap();

        ensure_gitignored(dir.path(), &["api".to_string()], &[]).unwrap();

        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert_eq!(
            content,
            "*.log\n# >>> git multirepo managed (do not edit below) >>>\n/api\n# <<< git multirepo managed <<<\ntarget/\n"
        );
    }

    #[test]
    fn is_idempotent_when_run_twice_with_the_same_repos() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        ensure_gitignored(dir.path(), &["api".to_string()], &[]).unwrap();
        ensure_gitignored(dir.path(), &["api".to_string()], &[]).unwrap();

        let content = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert_eq!(
            content,
            "# >>> git multirepo managed (do not edit below) >>>\n/api\n# <<< git multirepo managed <<<\n"
        );
    }

    #[test]
    fn is_a_noop_when_workspace_root_is_not_a_git_repo() {
        let dir = tempfile::tempdir().unwrap();

        ensure_gitignored(dir.path(), &["api".to_string()], &[]).unwrap();

        assert!(!dir.path().join(".gitignore").exists());
    }

    #[test]
    fn write_manifest_if_unchanged_writes_when_the_file_still_matches_the_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("multirepo.toml");
        std::fs::write(&path, "baseline").unwrap();

        let outcome = write_manifest_if_unchanged(&path, "baseline", "updated").unwrap();

        assert_eq!(outcome, ManifestWriteOutcome::Written);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "updated");
    }

    #[test]
    fn write_manifest_if_unchanged_refuses_when_the_file_changed_since_the_baseline_was_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("multirepo.toml");
        std::fs::write(&path, "baseline").unwrap();

        // Simulate a concurrent writer landing between the caller's read and
        // this write — the file on disk no longer matches what the caller
        // loaded.
        std::fs::write(&path, "someone else's concurrent edit").unwrap();

        let outcome = write_manifest_if_unchanged(&path, "baseline", "updated").unwrap();

        assert_eq!(outcome, ManifestWriteOutcome::ConcurrentModification);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "someone else's concurrent edit",
            "expected the concurrent writer's content to survive, not be clobbered"
        );
    }
}
