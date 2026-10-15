//! Diff/merge tool selection (`CAppUtils::PickDiffTool`, `StartExtDiff`,
//! `StartExtMerge`) and revision blob extraction for the viewers.

use std::path::{Path, PathBuf};

use crate::settings::Settings;
use crate::{Error, Repo, Result};

/// Which program shows a diff or merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tool {
    /// KoopaGit's built-in viewer (TortoiseGitMerge).
    Internal,
    /// An external command line template with `%base`-style placeholders.
    External(String),
}

fn file_name_lower(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn ext_lower(p: &Path) -> Option<String> {
    p.extension()
        .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
}

/// Applies the `#` "commented out" convention and the `/alternative` switch.
fn resolve(value: String, alternative: bool) -> Tool {
    let commented = value.starts_with('#');
    let cmd = if alternative {
        if commented {
            value[1..].to_owned()
        } else {
            String::new()
        }
    } else if commented {
        String::new()
    } else {
        value
    };
    if cmd.trim().is_empty() {
        Tool::Internal
    } else {
        Tool::External(cmd)
    }
}

/// `PickDiffTool`: `DiffTools\<filename>` of either file, then
/// `DiffTools\<.ext>`, then `Diff`.
pub fn pick_diff_tool(s: &Settings, file1: &Path, file2: &Path, alternative: bool) -> Tool {
    let key = |k: &str| s.string(&format!(r"DiffTools\{k}"), "");
    let mut value = key(&file_name_lower(file2));
    if value.is_empty() {
        value = key(&file_name_lower(file1));
    }
    if value.is_empty()
        && let Some(ext) = ext_lower(file2)
    {
        value = key(&ext);
    }
    if value.is_empty() {
        value = s.string("Diff", "");
    }
    resolve(value, alternative)
}

/// Merge tool for a conflicted file: `MergeTools\<.ext>`, `MergeTools\.<filename>`, then `Merge`.
pub fn pick_merge_tool(s: &Settings, merged: &Path, alternative: bool) -> Tool {
    let mut value = s.string("Merge", "");
    if let Some(ext) = ext_lower(merged) {
        let v = s.string(&format!(r"MergeTools\{ext}"), "");
        if !v.is_empty() {
            value = v;
        }
    }
    let v = s.string(&format!(r"MergeTools\.{}", file_name_lower(merged)), "");
    if !v.is_empty() {
        value = v;
    }
    resolve(value, alternative)
}

/// Splits a command template into arguments (double quotes group, as on Windows)
/// and replaces `%name` placeholders. Without any of `required` placeholders the
/// listed defaults are appended (upstream appends `%base %mine` / `%base %theirs %mine %merged`).
pub fn expand_command(template: &str, vars: &[(&str, &str)], required: &[&str]) -> Vec<String> {
    let mut template = template.to_owned();
    if !required.iter().any(|r| template.contains(&format!("%{r}"))) {
        for r in required {
            template.push_str(&format!(" %{r}"));
        }
    }
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut has = false;
    for c in template.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                has = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if has {
                    args.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            c => {
                cur.push(c);
                has = true;
            }
        }
    }
    if has {
        args.push(cur);
    }
    // Longest names first so `%base` does not eat `%basename`-like names.
    let mut vars: Vec<&(&str, &str)> = vars.iter().collect();
    vars.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));
    args.into_iter()
        .map(|a| {
            let mut a = a;
            for (k, v) in &vars {
                a = a.replace(&format!("%{k}"), v);
            }
            a
        })
        .collect()
}

/// Temporary directory for extracted revisions.
pub fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("koopagit");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

impl Repo {
    /// Abbreviated id of a revision.
    pub fn short_id(&self, rev: &str) -> Result<String> {
        Ok(self
            .git()
            .run(["rev-parse", "--short", &format!("{rev}^{{}}")])?
            .trim()
            .to_owned())
    }

    /// Writes `<rev>:<rel_path>` to a temp file named `<file>-<shorthash>.<ext>`
    /// (`GetOneFile`) and returns it. A path missing at that revision yields an
    /// empty file (`DiffNull`).
    pub fn extract_blob(&self, rev: &str, rel_path: &str) -> Result<PathBuf> {
        let short = self
            .short_id(rev)
            .unwrap_or_else(|_| rev.replace(['/', ':'], "_"));
        let p = Path::new(rel_path);
        let stem = p
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".into());
        let name = match p.extension() {
            Some(e) => format!("{stem}-{short}.{}", e.to_string_lossy()),
            None => format!("{stem}-{short}"),
        };
        let out = temp_dir().join(name);
        let res = self
            .git()
            .command(["show", &format!("{rev}:{rel_path}")])
            .output()?;
        let bytes = if res.status.success() {
            res.stdout
        } else {
            Vec::new()
        };
        std::fs::write(&out, bytes).map_err(Error::Spawn)?;
        Ok(out)
    }

    /// Index stage of a conflicted path (1 base, 2 ours, 3 theirs) into
    /// `<path>.<TYPE><ext>` next to the file, like `GetMergeTempFile`.
    pub fn extract_stage(&self, stage: u8, rel_path: &str, kind: &str) -> Result<PathBuf> {
        let abs = self.workdir().join(rel_path);
        let ext = abs
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        let out = PathBuf::from(format!("{}.{kind}{ext}", abs.to_string_lossy()));
        let res = self
            .git()
            .command(["show", &format!(":{stage}:{rel_path}")])
            .output()?;
        let bytes = if res.status.success() {
            res.stdout
        } else {
            Vec::new()
        };
        std::fs::write(&out, bytes).map_err(Error::Spawn)?;
        Ok(out)
    }

    /// For a file added in the index, the path it was renamed/copied from at HEAD
    /// (`git diff-index --raw HEAD -M -C -z`).
    pub fn rename_source(&self, rel_path: &str) -> Option<String> {
        let out = self
            .git()
            .run(["diff-index", "--raw", "HEAD", "-M50%", "-C50%", "-z", "--"])
            .ok()?;
        let mut it = out.split('\0');
        while let Some(meta) = it.next() {
            if meta.is_empty() {
                continue;
            }
            let status = meta.rsplit(' ').next().unwrap_or_default();
            let first = it.next()?;
            if status.starts_with('R') || status.starts_with('C') {
                let second = it.next()?;
                if second == rel_path {
                    return Some(first.to_owned());
                }
            }
        }
        None
    }

    /// The two most recent commits touching a path (`PrevDiff`), newest first.
    pub fn last_two_revisions(&self, rel_path: &str) -> Result<Vec<String>> {
        let out = self
            .git()
            .run(["log", "-2", "--format=%H", "--", rel_path])?;
        Ok(out.lines().map(str::to_owned).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::tests::fixture;

    #[test]
    fn tool_selection() {
        let mut s = Settings::default();
        let (a, b) = (Path::new("/x/a.rs"), Path::new("/y/b.RS"));
        assert_eq!(pick_diff_tool(&s, a, b, false), Tool::Internal);
        s.set("Diff", "#meld");
        assert_eq!(pick_diff_tool(&s, a, b, false), Tool::Internal);
        assert_eq!(
            pick_diff_tool(&s, a, b, true),
            Tool::External("meld".into())
        );
        s.set(r"DiffTools\.rs", "kdiff3 %base %mine");
        assert_eq!(
            pick_diff_tool(&s, a, b, false),
            Tool::External("kdiff3 %base %mine".into())
        );
        assert_eq!(pick_diff_tool(&s, a, b, true), Tool::Internal);
        s.set("Merge", "bc");
        s.set(r"MergeTools\.txt", "p4merge");
        assert_eq!(
            pick_merge_tool(&s, Path::new("m.txt"), false),
            Tool::External("p4merge".into())
        );
    }

    #[test]
    fn expands_placeholders() {
        let args = expand_command(
            r#""C:\Program Files\x.exe" /t1=%bname %base"#,
            &[("base", "/tmp/a"), ("bname", "A name"), ("mine", "/b")],
            &["base", "mine"],
        );
        assert_eq!(args, [r"C:\Program Files\x.exe", "/t1=A name", "/tmp/a"]);
        let appended = expand_command("meld", &[("base", "1"), ("mine", "2")], &["base", "mine"]);
        assert_eq!(appended, ["meld", "1", "2"]);
    }

    #[test]
    fn extracts_blobs_and_history() {
        let (dir, repo) = fixture();
        std::fs::write(dir.path().join("a.txt"), "v2\n").unwrap();
        repo.git().run(["commit", "-qam", "two"]).unwrap();
        let revs = repo.last_two_revisions("a.txt").unwrap();
        assert_eq!(revs.len(), 2);
        let old = repo.extract_blob(&revs[1], "a.txt").unwrap();
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "a\n");
        assert!(old.file_name().unwrap().to_string_lossy().starts_with("a-"));
        let missing = repo.extract_blob("HEAD", "nope.txt").unwrap();
        assert_eq!(std::fs::read(&missing).unwrap(), b"");

        repo.git().run(["mv", "a.txt", "moved.txt"]).unwrap();
        assert_eq!(repo.rename_source("moved.txt").as_deref(), Some("a.txt"));
    }
}
