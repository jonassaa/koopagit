//! Issue tracker integration properties (`bugtraq.*`, `ProjectProperties`):
//! the commit dialog's Bug-ID field.

use crate::Repo;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BugTraq {
    pub url: String,
    /// Contains `%BUGID%`; the Bug-ID field is shown only when this is set.
    pub message: String,
    /// Field label (`bugtraq.label`, default "Bug-ID / Issue-Nr:").
    pub label: String,
    /// Append (default) or prepend the message line.
    pub append: bool,
    /// Only numbers (optionally separated by commas) are allowed.
    pub number: bool,
    pub warn_if_no_issue: bool,
    pub logregex: String,
}

pub const DEFAULT_LABEL: &str = "Bug-ID / Issue-Nr:";

impl BugTraq {
    pub fn enabled(&self) -> bool {
        !self.message.is_empty()
    }

    /// Whether `id` is acceptable when `bugtraq.number` is set.
    pub fn valid_id(&self, id: &str) -> bool {
        !self.number
            || id
                .split(',')
                .all(|p| !p.trim().is_empty() && p.trim().chars().all(|c| c.is_ascii_digit()))
    }

    /// Adds the `bugtraq.message` line with `%BUGID%` replaced (upstream
    /// `CCommitDlg::OnOK`): appended after a blank line, or prepended.
    pub fn apply(&self, message: &str, id: &str) -> String {
        let id = id.trim();
        if !self.enabled() || id.is_empty() {
            return message.to_owned();
        }
        let line = self.message.replace("%BUGID%", id);
        if message.contains(&line) {
            return message.to_owned();
        }
        if self.append {
            let body = message.trim_end();
            if body.is_empty() {
                line
            } else {
                format!("{body}\n{line}\n")
            }
        } else {
            format!("{line}\n{message}")
        }
    }
}

fn truthy(v: &str, default: bool) -> bool {
    match v.trim().to_ascii_lowercase().as_str() {
        "" => default,
        "true" | "yes" | "on" | "1" => true,
        _ => false,
    }
}

impl Repo {
    /// Reads `bugtraq.*` (effective config, including `.tgitconfig`).
    pub fn bugtraq(&self) -> BugTraq {
        let git = self.git();
        let get = |k: &str| {
            let v = git
                .run(["config", "--get", k])
                .map(|v| v.trim().to_owned())
                .unwrap_or_default();
            if v.is_empty() {
                let project = self.workdir().join(".tgitconfig");
                if project.exists() {
                    return git
                        .run(["config", "--file", &project.to_string_lossy(), "--get", k])
                        .map(|v| v.trim().to_owned())
                        .unwrap_or_default();
                }
            }
            v
        };
        let label = get("bugtraq.label");
        BugTraq {
            url: get("bugtraq.url"),
            message: get("bugtraq.message"),
            label: if label.is_empty() {
                DEFAULT_LABEL.to_owned()
            } else {
                label
            },
            append: truthy(&get("bugtraq.append"), true),
            number: truthy(&get("bugtraq.number"), true),
            warn_if_no_issue: truthy(&get("bugtraq.warningifnoissue"), false),
            logregex: get("bugtraq.logregex"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_line_and_validation() {
        let b = BugTraq {
            message: "Issue: %BUGID%".into(),
            append: true,
            number: true,
            ..Default::default()
        };
        assert_eq!(b.apply("Fix crash\n", "12"), "Fix crash\nIssue: 12\n");
        assert_eq!(
            b.apply("Fix crash\nIssue: 12\n", "12"),
            "Fix crash\nIssue: 12\n"
        );
        assert_eq!(b.apply("x", " "), "x");
        assert!(b.valid_id("1, 23"));
        assert!(!b.valid_id("ABC-1"));
        let pre = BugTraq {
            append: false,
            number: false,
            ..b
        };
        assert_eq!(pre.apply("Fix", "ABC-1"), "Issue: ABC-1\nFix");
        assert!(pre.valid_id("ABC-1"));
    }

    #[test]
    fn reads_config() {
        let (_dir, repo) = crate::repo::tests::fixture();
        assert!(!repo.bugtraq().enabled());
        repo.git()
            .run(["config", "bugtraq.message", "Ref #%BUGID%"])
            .unwrap();
        repo.git()
            .run(["config", "bugtraq.append", "false"])
            .unwrap();
        let b = repo.bugtraq();
        assert!(b.enabled() && !b.append && b.number);
        assert_eq!(b.label, DEFAULT_LABEL);
    }
}
