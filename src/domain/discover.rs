use std::path::{Path, PathBuf};

/// Walk upward from `start` looking for a `workspaces.toml`, exactly like
/// git walks upward looking for `.git` — passing straight through any
/// nested child repo's own `.git` boundary on the way up.
///
/// Pure: filesystem existence is injected via `exists` so this needs no
/// real I/O and can be exhaustively unit tested.
pub fn find_workspace_root(start: &Path, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let mut current = start;
    loop {
        let candidate = current.join("workspaces.toml");
        if exists(&candidate) {
            return Some(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn exists_set(paths: &[&str]) -> HashSet<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn finds_manifest_in_the_starting_directory() {
        let manifest_paths = exists_set(&["/workspace/workspaces.toml"]);
        let found = find_workspace_root(Path::new("/workspace"), |p| manifest_paths.contains(p));
        assert_eq!(found, Some(PathBuf::from("/workspace")));
    }

    #[test]
    fn finds_manifest_several_levels_up() {
        let manifest_paths = exists_set(&["/workspace/workspaces.toml"]);
        let found = find_workspace_root(Path::new("/workspace/api/src/domain"), |p| {
            manifest_paths.contains(p)
        });
        assert_eq!(found, Some(PathBuf::from("/workspace")));
    }

    #[test]
    fn returns_none_when_no_ancestor_declares_a_manifest() {
        let found = find_workspace_root(Path::new("/workspace/api/src"), |_| false);
        assert_eq!(found, None);
    }

    #[test]
    fn passes_straight_through_a_nested_child_repos_git_boundary() {
        // `/workspace/api` is itself a git repo (has its own `.git`), but the
        // manifest lives at the workspace root above it. Discovery must not
        // stop at the child repo's `.git` boundary — we don't even look for
        // `.git`, so this falls out for free, but assert it explicitly.
        let manifest_paths = exists_set(&["/workspace/workspaces.toml"]);
        let found =
            find_workspace_root(Path::new("/workspace/api"), |p| manifest_paths.contains(p));
        assert_eq!(found, Some(PathBuf::from("/workspace")));
    }
}
