# Security policy

## Supported versions

KoopaGit hasn't had a release yet. Security fixes go into `main`.

## Reporting a vulnerability

Please **don't** open a public issue. Report the vulnerability privately with
GitHub's [private vulnerability reporting](https://github.com/jonassaa/koopagit/security/advisories/new).

Please include:

- what an attacker can do, and on which platform,
- steps to reproduce it, or a proof of concept,
- the commit you tested.

You should get an acknowledgement within a week. Once a fix is ready, it is
published together with a GitHub security advisory that credits you, unless
you'd rather not be named.

KoopaGit runs `git` on the repositories you open. Vulnerabilities in Git
itself, such as malicious repository configuration or hooks, should be reported
to the [Git project](https://git-scm.com/community), unless KoopaGit makes them
worse.
