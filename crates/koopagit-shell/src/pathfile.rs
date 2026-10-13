//! `/pathfile:` handling: like the Explorer extension, pass many paths through
//! a UTF-16LE temp file instead of the command line.

use std::path::{Path, PathBuf};

use koopagit_core::launcher::Invocation;

/// Rewrites an invocation with more than one `/path:` entry to use
/// `/pathfile:<tmp> /deletepathfile`, writing the file into `dir`.
/// Invocations with zero or one path are returned unchanged.
pub fn use_pathfile(inv: &Invocation, dir: &Path) -> std::io::Result<Invocation> {
    let paths: Vec<&str> = inv
        .get("path")
        .map(|p| p.split('*').filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    if paths.len() < 2 {
        return Ok(inv.clone());
    }
    let file = write_pathfile(dir, &paths)?;
    let args: Vec<String> = inv
        .to_args()
        .into_iter()
        .filter(|a| !a.starts_with("/path:"))
        .chain([
            format!("/pathfile:{}", file.to_string_lossy()),
            "/deletepathfile".to_owned(),
        ])
        .collect();
    Invocation::parse(args).map_err(|e| std::io::Error::other(e.to_string()))
}

/// Writes one path per line as UTF-16LE with BOM (`CTGitPathList::WriteToFile`).
pub fn write_pathfile(dir: &Path, paths: &[&str]) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let file = dir.join(format!("git{}-{nanos}.tmp", std::process::id()));
    let mut bytes = vec![0xFF, 0xFE];
    for p in paths {
        for unit in p.encode_utf16().chain("\n".encode_utf16()) {
            bytes.extend(unit.to_le_bytes());
        }
    }
    std::fs::write(&file, bytes)?;
    Ok(file)
}

/// The temp directory pathfiles go into.
pub fn temp_dir() -> PathBuf {
    std::env::temp_dir().join("koopagit")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn many_paths_go_through_a_pathfile() {
        let dir = tempfile::tempdir().unwrap();
        let inv = Invocation::new("commit").with_paths(["/r/a", "/r/b"]);
        let out = use_pathfile(&inv, dir.path()).unwrap();
        assert!(out.get("path").is_none());
        assert!(out.has("deletepathfile"));
        let file = PathBuf::from(out.get("pathfile").unwrap());
        assert!(std::fs::read(&file).unwrap().starts_with(&[0xFF, 0xFE]));
        // The launcher side reads it back (and deletes it).
        assert_eq!(
            out.paths().unwrap(),
            [PathBuf::from("/r/a"), PathBuf::from("/r/b")]
        );
        assert!(!file.exists());

        let single = Invocation::new("log").with_paths(["/r/a"]);
        assert_eq!(use_pathfile(&single, dir.path()).unwrap(), single);
    }
}
