//! Pure helpers for the less common commands: git-svn, Send Mail, proxy,
//! daemon and drag-and-drop operations.

use std::path::{Path, PathBuf};

use crate::{Repo, Result};

fn s(v: &str) -> String {
    v.to_owned()
}

// ----- git-svn -------------------------------------------------------------

/// The SVN tracking ref from `svn-remote.svn.fetch` (`trunk:refs/remotes/trunk`
/// → `remotes/trunk`), as `SVNRebaseCommand` / `SVNFetchCommand` derive it.
pub fn svn_upstream(fetch_value: &str) -> Option<String> {
    let first = fetch_value.lines().next()?.trim();
    let after = &first[first.find(':')?..];
    let r = after
        .strip_prefix(":refs/")
        .unwrap_or(after.trim_start_matches(':'));
    (!r.is_empty()).then(|| r.to_owned())
}

/// `git svn dcommit [--rmdir]`.
pub fn svn_dcommit(rmdir: bool) -> Vec<String> {
    let mut a = vec![s("svn"), s("dcommit")];
    if rmdir {
        a.push(s("--rmdir"));
    }
    a
}

impl Repo {
    pub fn svn_upstream(&self) -> Option<String> {
        let v = self.git().run(["config", "svn-remote.svn.fetch"]).ok()?;
        svn_upstream(&v)
    }

    /// Whether the working tree has no changes to tracked files
    /// (`CGit::CheckCleanWorkTree`).
    pub fn is_clean(&self) -> bool {
        let git = self.git();
        git.output(["diff", "--quiet"]).is_ok_and(|o| o.success())
            && git
                .output(["diff", "--cached", "--quiet"])
                .is_ok_and(|o| o.success())
    }

    /// Branch and tag names containing `commit` (`GetRefsCommitIsOn`).
    pub fn refs_containing(&self, commit: &str) -> Result<Vec<String>> {
        let out = self.git().run([
            "for-each-ref",
            "--format=%(refname)",
            "--contains",
            commit,
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ])?;
        Ok(out
            .lines()
            .filter(|l| !l.ends_with("/HEAD"))
            .map(|l| {
                l.strip_prefix("refs/heads/")
                    .or_else(|| l.strip_prefix("refs/"))
                    .unwrap_or(l)
                    .to_owned()
            })
            .collect())
    }
}

// ----- Send Mail -----------------------------------------------------------

/// A patch file produced by `git format-patch` (`CSerialPatch::Parse`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailPatch {
    pub subject: String,
    pub body: String,
}

/// Reads the `Subject:` header (with RFC 2822 continuation lines) and the body.
pub fn parse_patch(text: &str) -> MailPatch {
    let mut subject = String::new();
    let mut in_subject = false;
    let mut body_start = text.len();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let l = line.trim_end_matches(['\r', '\n']);
        if in_subject && (l.starts_with(' ') || l.starts_with('\t')) {
            subject.push(' ');
            subject.push_str(l.trim());
        } else if let Some(rest) = l.strip_prefix("Subject: ") {
            subject = rest.trim().to_owned();
            in_subject = true;
        } else {
            in_subject = false;
            if l.is_empty() && !subject.is_empty() {
                body_start = offset + line.len();
                break;
            }
        }
        offset += line.len();
    }
    MailPatch {
        subject,
        body: text.get(body_start..).unwrap_or_default().to_owned(),
    }
}

fn percent_encode(v: &str) -> String {
    let mut out = String::new();
    for b in v.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'@' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// A `mailto:` URL for the default mail client (attachments are not possible
/// through `mailto:`).
pub fn mailto(to: &[String], cc: &[String], subject: &str, body: &str) -> String {
    let mut url = format!(
        "mailto:{}",
        to.iter()
            .map(|t| percent_encode(t.trim()))
            .collect::<Vec<_>>()
            .join(",")
    );
    let mut q = Vec::new();
    if !cc.is_empty() {
        q.push(format!(
            "cc={}",
            cc.iter()
                .map(|c| percent_encode(c.trim()))
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    q.push(format!("subject={}", percent_encode(subject)));
    q.push(format!("body={}", percent_encode(body)));
    url.push('?');
    url.push_str(&q.join("&"));
    url
}

/// `git send-email` arguments for the selected patches.
pub fn send_email_args(
    to: &[String],
    cc: &[String],
    subject: Option<&str>,
    patches: &[PathBuf],
) -> Vec<String> {
    let mut a = vec![s("send-email"), s("--confirm=never")];
    for t in to.iter().filter(|t| !t.trim().is_empty()) {
        a.push(format!("--to={}", t.trim()));
    }
    for c in cc.iter().filter(|c| !c.trim().is_empty()) {
        a.push(format!("--cc={}", c.trim()));
    }
    if let Some(sub) = subject {
        a.push("--compose".to_owned());
        a.push(format!("--subject={sub}"));
    }
    a.push(s("--"));
    a.extend(patches.iter().map(|p| p.to_string_lossy().into_owned()));
    a
}

/// Splits a `;`-separated address field.
pub fn split_addresses(v: &str) -> Vec<String> {
    v.split([';', ','])
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(str::to_owned)
        .collect()
}

// ----- Network -------------------------------------------------------------

fn escape_userinfo(v: &str) -> String {
    let mut out = String::new();
    for c in v.chars() {
        match c {
            ':' | '@' | '/' | '%' => out.push_str(&format!("%{:02X}", c as u32)),
            _ => out.push(c),
        }
    }
    out
}

/// The global `http.proxy` value (`SetProxyPage.cpp:224-266`):
/// `http://[user[:pass]@]host[:port]`, or `None` to unset.
pub fn proxy_url(enabled: bool, host: &str, port: &str, user: &str, pass: &str) -> Option<String> {
    if !enabled || host.trim().is_empty() {
        return None;
    }
    let host = host.trim();
    let mut url = String::new();
    if !host.contains("://") {
        url.push_str("http://");
    }
    if !user.is_empty() {
        url.push_str(&escape_userinfo(user));
        if !pass.is_empty() {
            url.push(':');
            url.push_str(&escape_userinfo(pass));
        }
        url.push('@');
    }
    url.push_str(host);
    if let Ok(p) = port.trim().parse::<u16>()
        && p != 0
    {
        url.push_str(&format!(":{p}"));
    }
    Some(url)
}

// ----- Add / Clean -----------------------------------------------------------

/// "Add as Executable (+x)": `git update-index --add --chmod=+x -- <paths>`.
pub fn add_executable_args(paths: &[String]) -> Vec<String> {
    let mut a = vec![s("update-index"), s("--add"), s("--chmod=+x"), s("--")];
    a.extend(paths.iter().cloned());
    a
}

/// "Add as Symlink": index mode 120000 for a file whose content is the link
/// target (`git update-index --add --cacheinfo 120000,<blob>,<path>`).
pub fn add_symlink_args(blob: &str, path: &str) -> Vec<String> {
    vec![
        s("update-index"),
        s("--add"),
        s("--cacheinfo"),
        format!("120000,{blob},{path}"),
    ]
}

/// Paths from `git clean -n` output ("Would remove x"), resolved against
/// `git submodule foreach` "Entering 'sub'" lines. Run with
/// `-c core.quotepath=false` so names are not escaped.
pub fn parse_clean_dry_run(out: &str) -> Vec<String> {
    let mut prefix = String::new();
    let mut paths = Vec::new();
    for line in out.lines() {
        if let Some(rest) = line.strip_prefix("Entering '") {
            prefix = format!("{}/", rest.trim_end_matches('\''));
        } else if let Some(p) = line.strip_prefix("Would remove ") {
            paths.push(format!("{prefix}{}", p.trim_end_matches('/')));
        }
    }
    paths
}

// ----- daemon / drag and drop ----------------------------------------------

/// `git daemon --verbose --export-all --base-path=<repo>`.
pub fn daemon_args(base: &Path) -> Vec<String> {
    vec![
        s("daemon"),
        s("--verbose"),
        s("--export-all"),
        format!("--base-path={}", base.to_string_lossy()),
    ]
}

/// `git mv -- <src> <dst>` (paths relative to the work tree).
pub fn mv_args(src: &str, dst: &str) -> Vec<String> {
    vec![s("mv"), s("--"), s(src), s(dst)]
}

/// Copies a file or directory tree (directories skip nested `.git`).
pub fn copy_recursive(from: &Path, to: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut copied = Vec::new();
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for e in std::fs::read_dir(from)? {
            let e = e?;
            if e.file_name() == ".git" {
                continue;
            }
            copied.extend(copy_recursive(&e.path(), &to.join(e.file_name()))?);
        }
    } else {
        std::fs::copy(from, to)?;
        copied.push(to.to_owned());
    }
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svn_ref_and_args() {
        assert_eq!(
            svn_upstream("trunk:refs/remotes/origin/trunk\n").as_deref(),
            Some("remotes/origin/trunk")
        );
        assert_eq!(svn_upstream("nothing"), None);
        assert_eq!(svn_dcommit(true).join(" "), "svn dcommit --rmdir");
    }

    #[test]
    fn patch_subject_and_mailto() {
        let p = parse_patch(
            "From abc Mon Sep 17 00:00:00 2001\nFrom: A <a@b>\nSubject: [PATCH 1/2] Fix the\n thing\n\nbody line\n---\n",
        );
        assert_eq!(p.subject, "[PATCH 1/2] Fix the thing");
        assert_eq!(p.body, "body line\n---\n");
        let m = mailto(&["a@b.c".into()], &["d@e.f".into()], "Hi there", "x&y");
        assert_eq!(m, "mailto:a@b.c?cc=d@e.f&subject=Hi%20there&body=x%26y");
        assert_eq!(split_addresses("a@b; c@d,"), ["a@b", "c@d"]);
        let a = send_email_args(
            &["a@b".into()],
            &[],
            None,
            &[PathBuf::from("/p/0001.patch")],
        );
        assert_eq!(
            a.join(" "),
            "send-email --confirm=never --to=a@b -- /p/0001.patch"
        );
    }

    #[test]
    fn proxy() {
        assert_eq!(proxy_url(false, "h", "", "", ""), None);
        assert_eq!(
            proxy_url(true, "proxy.local", "3128", "u@x", "p:w").as_deref(),
            Some("http://u%40x:p%3Aw@proxy.local:3128")
        );
        assert_eq!(
            proxy_url(true, "socks5://h", "", "", "").as_deref(),
            Some("socks5://h")
        );
    }

    #[test]
    fn refs_containing_and_copy() {
        let (dir, repo) = crate::repo::tests::fixture();
        repo.git().run(["tag", "v1"]).unwrap();
        repo.git().run(["branch", "side"]).unwrap();
        let head = repo.head_id().unwrap().unwrap();
        let mut refs = repo.refs_containing(&head).unwrap();
        refs.sort();
        assert_eq!(refs, ["main", "side", "tags/v1"]);
        assert!(repo.is_clean());
        assert!(repo.is_ancestor(&head, "HEAD"));
        let src = dir.path().join("d");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("sub/f"), "x").unwrap();
        let copied = copy_recursive(&src, &dir.path().join("e")).unwrap();
        assert_eq!(copied.len(), 1);
        assert_eq!(daemon_args(Path::new("/r"))[3], "--base-path=/r");
    }

    #[test]
    fn clean_dry_run_and_add_modes() {
        let out = "Would remove a.log\nWould remove dir/\nEntering 'sub'\nWould remove x\n";
        assert_eq!(parse_clean_dry_run(out), ["a.log", "dir", "sub/x"]);
        assert_eq!(
            add_executable_args(&["run.sh".into()]).join(" "),
            "update-index --add --chmod=+x -- run.sh"
        );
        assert_eq!(add_symlink_args("abc", "l")[3], "120000,abc,l");
    }
}
