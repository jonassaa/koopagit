//! Moving files to the OS trash / recycle bin (`RevertWithRecycleBin`).

use std::path::PathBuf;

/// Moves existing paths to the trash. Returns the paths that could not be
/// trashed (callers fall back to deleting or report them).
pub fn move_to_trash(paths: &[PathBuf]) -> Vec<PathBuf> {
    let existing: Vec<&PathBuf> = paths.iter().filter(|p| p.exists()).collect();
    if existing.is_empty() || trash::delete_all(&existing).is_ok() {
        return Vec::new();
    }
    // Retry one by one so a single failure doesn't block the rest.
    existing
        .into_iter()
        .filter(|p| trash::delete(p).is_err())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn missing_paths_are_ignored() {
        assert!(super::move_to_trash(&["/nonexistent/koopagit-x".into()]).is_empty());
    }
}
