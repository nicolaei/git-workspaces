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
