//! Include/exclude path lists (`OverlayIncludeList`, `OverlayExcludeList`)
//! and `NoContextPaths` matching (`ShellCache.cpp`).

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    path: String,
    recursive: bool,
    include: bool,
}

/// Overlay path filter: entries are newline-separated; a trailing `?` limits an
/// entry to that folder, otherwise it is recursive (a trailing `*` or separator
/// is ignored). The most specific entry wins; a single-folder entry beats a
/// recursive one and include beats exclude for the same path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathFilter {
    rules: Vec<Rule>,
}

fn normalize(p: &str) -> String {
    let p = p.replace('\\', "/");
    let p = p.trim_end_matches('/');
    if cfg!(any(windows, target_os = "macos")) {
        p.to_lowercase()
    } else {
        p.to_owned()
    }
}

fn parse(list: &str, include: bool, out: &mut Vec<Rule>) {
    for line in list.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (body, recursive) = match line.strip_suffix('?') {
            Some(b) => (b, false),
            None => (line.trim_end_matches('*'), true),
        };
        out.push(Rule {
            path: normalize(body),
            recursive,
            include,
        });
    }
}

impl PathFilter {
    pub fn new(include: &str, exclude: &str) -> Self {
        let mut rules = Vec::new();
        parse(include, true, &mut rules);
        parse(exclude, false, &mut rules);
        PathFilter { rules }
    }

    /// Whether overlays may be shown for `path` (no matching rule = allowed).
    pub fn is_allowed(&self, path: &Path) -> bool {
        let p = normalize(&path.to_string_lossy());
        self.rules
            .iter()
            .filter(|r| p == r.path || (r.recursive && p.starts_with(&format!("{}/", r.path))))
            .max_by_key(|r| (r.path.len(), !r.recursive, r.include))
            .is_none_or(|r| r.include)
    }
}

/// `NoContextPaths`: trailing `*` = prefix match, trailing separator = that
/// folder and everything below, otherwise exact; case-insensitive.
pub fn is_no_context_path(list: &str, path: &Path) -> bool {
    let p = path.to_string_lossy().replace('\\', "/").to_lowercase();
    list.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .any(|entry| {
            let e = entry.replace('\\', "/").to_lowercase();
            if let Some(prefix) = e.strip_suffix('*') {
                p.starts_with(prefix)
            } else if let Some(dir) = e.strip_suffix('/') {
                p == dir || p.starts_with(&format!("{dir}/"))
            } else {
                p == e
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specificity_rules() {
        let f = PathFilter::new("/w/keep\n/w/only?", "/w\n/w/keep/drop*");
        assert!(!f.is_allowed(Path::new("/w/x")));
        assert!(f.is_allowed(Path::new("/w/keep/a")));
        assert!(!f.is_allowed(Path::new("/w/keep/drop/b")));
        assert!(f.is_allowed(Path::new("/w/only")));
        assert!(!f.is_allowed(Path::new("/w/only/child")));
        assert!(f.is_allowed(Path::new("/elsewhere")));
    }

    #[test]
    fn no_context_paths() {
        let list = "/tmp/x*\n/data/\n/exact";
        assert!(is_no_context_path(list, Path::new("/tmp/xyz")));
        assert!(is_no_context_path(list, Path::new("/data/a/b")));
        assert!(is_no_context_path(list, Path::new("/EXACT")));
        assert!(!is_no_context_path(list, Path::new("/exact/sub")));
    }
}
