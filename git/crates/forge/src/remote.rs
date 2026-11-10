//! Remote URL → forge, host and project path.
//!
//! Handles scp-like SSH (`git@host:o/r.git`), `ssh://` (with ports), HTTPS
//! (with credentials or ports), `git://`, nested GitLab groups, and the
//! SSH-over-443 hosts (`ssh.github.com`, `altssh.gitlab.com`).

use crate::error::{ForgeError, ForgeResult};
use ferrumgit_domain::{ForgeKind, RepoForge};

/// Host and project path parsed from a remote URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteUrl {
    /// Web host, including a port for HTTP(S) remotes (`git.example.com:8443`).
    pub host: String,
    /// `owner/repo` or `group/subgroup/project`, without `.git`.
    pub path: String,
    /// `http` for plain-HTTP remotes, otherwise `https`.
    pub scheme: &'static str,
}

/// Parses a remote URL. Local paths and `file://` URLs give `None`.
#[must_use]
pub fn parse_remote_url(url: &str) -> Option<RemoteUrl> {
    let u = url.trim();
    let (scheme, rest, web_port) = if let Some((scheme, rest)) = u.split_once("://") {
        match scheme {
            "https" => ("https", rest, true),
            "http" => ("http", rest, true),
            "ssh" | "git+ssh" | "ssh+git" | "git" => ("https", rest, false),
            _ => return None,
        }
    } else {
        // scp-like: [user@]host:path (no slash before the colon).
        let colon = u.find(':')?;
        // A single letter before the colon is a Windows drive (`C:/repo`).
        if colon <= 1 || u[..colon].contains('/') {
            return None;
        }
        let (hostpart, path) = (&u[..colon], &u[colon + 1..]);
        let host = hostpart.rsplit('@').next()?;
        return build(host, path, "https");
    };
    let (authority, path) = rest.split_once('/')?;
    let hostport = authority.rsplit('@').next()?;
    let host = if web_port {
        hostport
    } else {
        hostport.split(':').next()?
    };
    build(host, path, scheme)
}

fn build(host: &str, path: &str, scheme: &'static str) -> Option<RemoteUrl> {
    let host = host.trim().to_lowercase();
    let host = match host.as_str() {
        "ssh.github.com" => "github.com".to_owned(),
        "altssh.gitlab.com" => "gitlab.com".to_owned(),
        _ => host,
    };
    let path = path
        .trim()
        .trim_matches('/')
        .trim_end_matches(".git")
        .trim_end_matches('/');
    // `ssh://git@host:2222/~user/repo` style paths are not forge projects.
    if host.is_empty() || path.is_empty() || path.starts_with('~') || !path.contains('/') {
        return None;
    }
    if path.split('/').any(|seg| seg.is_empty() || seg == "..") {
        return None;
    }
    Some(RemoteUrl {
        host,
        path: path.to_owned(),
        scheme,
    })
}

/// Guesses the forge from the host name.
#[must_use]
pub fn detect_kind(host: &str) -> Option<ForgeKind> {
    let name = host.split(':').next().unwrap_or(host);
    match name {
        "github.com" => Some(ForgeKind::GitHub),
        "gitlab.com" => Some(ForgeKind::GitLab),
        n if n.split(['.', '-']).any(|p| p == "gitlab") => Some(ForgeKind::GitLab),
        n if n.split(['.', '-']).any(|p| p == "github" || p == "ghe") => Some(ForgeKind::GitHub),
        _ => None,
    }
}

/// Default REST base URL: `api.github.com`, GitHub Enterprise `/api/v3`, GitLab `/api/v4`.
#[must_use]
pub fn default_api_url(kind: ForgeKind, host: &str, scheme: &str) -> String {
    match kind {
        ForgeKind::GitHub if host == "github.com" => "https://api.github.com".to_owned(),
        ForgeKind::GitHub => format!("{scheme}://{host}/api/v3"),
        ForgeKind::GitLab => format!("{scheme}://{host}/api/v4"),
    }
}

/// A project on a forge, resolved from a remote plus the repository's overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeRepo {
    pub kind: ForgeKind,
    /// Host used as the token's keychain account (`github.com`).
    pub host: String,
    /// `owner` (GitHub) or the full namespace (GitLab `group/subgroup`).
    pub owner: String,
    pub name: String,
    /// `https://github.com/owner/repo`.
    pub web_url: String,
    /// REST base without a trailing slash.
    pub api_url: String,
    /// Git remote the project was read from (fetches PR refs from it).
    pub remote: String,
}

impl ForgeRepo {
    /// `owner/repo` or `group/sub/project`.
    #[must_use]
    pub fn path(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    /// Ref holding a pull request's head on the forge remote.
    #[must_use]
    pub fn head_ref(&self, number: u64) -> String {
        match self.kind {
            ForgeKind::GitHub => format!("refs/pull/{number}/head"),
            ForgeKind::GitLab => format!("refs/merge-requests/{number}/head"),
        }
    }

    #[must_use]
    pub fn pr_web_url(&self, number: u64) -> String {
        match self.kind {
            ForgeKind::GitHub => format!("{}/pull/{number}", self.web_url),
            ForgeKind::GitLab => format!("{}/-/merge_requests/{number}", self.web_url),
        }
    }

    /// The forge's list page, for "open in browser".
    #[must_use]
    pub fn prs_web_url(&self) -> String {
        match self.kind {
            ForgeKind::GitHub => format!("{}/pulls", self.web_url),
            ForgeKind::GitLab => format!("{}/-/merge_requests", self.web_url),
        }
    }
}

/// Picks the remote to use: the override, `origin`, else the first one.
#[must_use]
pub fn pick_remote<'a>(
    remotes: &'a [(String, String)],
    wanted: &str,
) -> Option<&'a (String, String)> {
    let wanted = wanted.trim();
    if !wanted.is_empty() {
        return remotes.iter().find(|(name, _)| name == wanted);
    }
    remotes
        .iter()
        .find(|(name, _)| name == "origin")
        .or_else(|| remotes.first())
}

/// Resolves the forge project for a repository. `remotes` are `(name, url)` pairs.
pub fn resolve(remotes: &[(String, String)], settings: &RepoForge) -> ForgeResult<ForgeRepo> {
    let (remote, url) = pick_remote(remotes, &settings.remote).ok_or(ForgeError::NotDetected)?;
    let parsed = parse_remote_url(url).ok_or(ForgeError::NotDetected)?;
    let kind = settings
        .kind
        .or_else(|| detect_kind(&parsed.host))
        .ok_or(ForgeError::NotDetected)?;
    let api_url = match settings.api_url.trim().trim_end_matches('/') {
        "" => default_api_url(kind, &parsed.host, parsed.scheme),
        custom => custom.to_owned(),
    };
    let (owner, name) = parsed
        .path
        .rsplit_once('/')
        .ok_or(ForgeError::NotDetected)?;
    if kind == ForgeKind::GitHub && owner.contains('/') {
        return Err(ForgeError::Invalid(format!(
            "“{}” is not a GitHub owner/repo path. If this is a GitLab server, set the forge type in Repository settings.",
            parsed.path
        )));
    }
    Ok(ForgeRepo {
        kind,
        web_url: format!("{}://{}/{}", parsed.scheme, parsed.host, parsed.path),
        host: parsed.host,
        owner: owner.to_owned(),
        name: name.to_owned(),
        api_url,
        remote: remote.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(url: &str) -> Option<(String, String)> {
        parse_remote_url(url).map(|r| (r.host, r.path))
    }

    fn pair(h: &str, path: &str) -> Option<(String, String)> {
        Some((h.to_owned(), path.to_owned()))
    }

    #[test]
    fn parses_ssh_https_and_self_hosted_urls() {
        assert_eq!(
            p("git@github.com:rust-lang/rust.git"),
            pair("github.com", "rust-lang/rust")
        );
        assert_eq!(p("github.com:o/r"), pair("github.com", "o/r"));
        assert_eq!(p("https://github.com/o/r"), pair("github.com", "o/r"));
        assert_eq!(p("https://github.com/o/r.git/"), pair("github.com", "o/r"));
        assert_eq!(
            p("https://me:ghp_secret@github.com/o/r.git"),
            pair("github.com", "o/r")
        );
        assert_eq!(
            p("ssh://git@ssh.github.com:443/o/r.git"),
            pair("github.com", "o/r")
        );
        assert_eq!(
            p("ssh://git@gitlab.example.com:2222/group/sub/project.git"),
            pair("gitlab.example.com", "group/sub/project")
        );
        assert_eq!(
            p("https://git.example.com:8443/team/app.git"),
            pair("git.example.com:8443", "team/app")
        );
        assert_eq!(
            p("git@GitLab.com:Group/Proj.git"),
            pair("gitlab.com", "Group/Proj")
        );
        assert_eq!(p("git://github.com/o/r.git"), pair("github.com", "o/r"));
        assert_eq!(
            parse_remote_url("http://ghe.local/o/r").map(|r| r.scheme),
            Some("http")
        );
    }

    #[test]
    fn rejects_non_forge_urls() {
        assert_eq!(p("/srv/git/repo.git"), None);
        assert_eq!(p("../other"), None);
        assert_eq!(p("file:///srv/git/r.git"), None);
        assert_eq!(p("https://github.com/onlyowner"), None);
        assert_eq!(p("ssh://git@host/~me/r.git"), None);
        assert_eq!(p("C:/path/repo"), None);
    }

    #[test]
    fn detects_forge_from_host() {
        assert_eq!(detect_kind("github.com"), Some(ForgeKind::GitHub));
        assert_eq!(detect_kind("gitlab.com"), Some(ForgeKind::GitLab));
        assert_eq!(detect_kind("gitlab.example.org"), Some(ForgeKind::GitLab));
        assert_eq!(detect_kind("my-gitlab.corp:8443"), Some(ForgeKind::GitLab));
        assert_eq!(detect_kind("github.acme.com"), Some(ForgeKind::GitHub));
        assert_eq!(detect_kind("git.example.com"), None);
        assert_eq!(detect_kind("bitbucket.org"), None);
    }

    fn remotes(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect()
    }

    #[test]
    fn resolves_github_and_gitlab_projects() {
        let r = resolve(
            &remotes(&[
                ("upstream", "git@x.org:a/b"),
                ("origin", "git@github.com:o/r.git"),
            ]),
            &RepoForge::default(),
        )
        .expect("github");
        assert_eq!(r.kind, ForgeKind::GitHub);
        assert_eq!((r.owner.as_str(), r.name.as_str()), ("o", "r"));
        assert_eq!(r.api_url, "https://api.github.com");
        assert_eq!(r.web_url, "https://github.com/o/r");
        assert_eq!(r.remote, "origin");
        assert_eq!(r.head_ref(7), "refs/pull/7/head");
        assert_eq!(r.pr_web_url(7), "https://github.com/o/r/pull/7");

        let r = resolve(
            &remotes(&[("origin", "https://gitlab.com/g/sub/p.git")]),
            &RepoForge::default(),
        )
        .expect("gitlab");
        assert_eq!(r.kind, ForgeKind::GitLab);
        assert_eq!((r.owner.as_str(), r.name.as_str()), ("g/sub", "p"));
        assert_eq!(r.api_url, "https://gitlab.com/api/v4");
        assert_eq!(r.head_ref(3), "refs/merge-requests/3/head");
        assert_eq!(
            r.pr_web_url(3),
            "https://gitlab.com/g/sub/p/-/merge_requests/3"
        );
    }

    #[test]
    fn self_hosted_needs_or_accepts_overrides() {
        let list = remotes(&[("origin", "git@git.example.com:team/app.git")]);
        assert_eq!(
            resolve(&list, &RepoForge::default()),
            Err(ForgeError::NotDetected)
        );
        let gl = resolve(
            &list,
            &RepoForge {
                kind: Some(ForgeKind::GitLab),
                ..RepoForge::default()
            },
        )
        .expect("gitlab");
        assert_eq!(gl.api_url, "https://git.example.com/api/v4");
        let ghe = resolve(
            &list,
            &RepoForge {
                kind: Some(ForgeKind::GitHub),
                api_url: "https://git.example.com/custom/api/".into(),
                remote: String::new(),
            },
        )
        .expect("ghe");
        assert_eq!(ghe.api_url, "https://git.example.com/custom/api");
        // Detected GitHub Enterprise host uses /api/v3.
        let ghe = resolve(
            &remotes(&[("origin", "https://github.acme.com/o/r")]),
            &RepoForge::default(),
        )
        .expect("ghe");
        assert_eq!(ghe.api_url, "https://github.acme.com/api/v3");
    }

    #[test]
    fn picks_requested_remote_and_rejects_nested_github_paths() {
        let list = remotes(&[
            ("origin", "git@github.com:me/r.git"),
            ("upstream", "git@github.com:org/r.git"),
        ]);
        let r = resolve(
            &list,
            &RepoForge {
                remote: "upstream".into(),
                ..RepoForge::default()
            },
        )
        .expect("upstream");
        assert_eq!(r.owner, "org");
        assert_eq!(
            resolve(
                &list,
                &RepoForge {
                    remote: "missing".into(),
                    ..RepoForge::default()
                }
            ),
            Err(ForgeError::NotDetected)
        );
        assert!(matches!(
            resolve(
                &remotes(&[("origin", "git@github.com:a/b/c.git")]),
                &RepoForge::default()
            ),
            Err(ForgeError::Invalid(_))
        ));
        assert_eq!(
            resolve(&[], &RepoForge::default()),
            Err(ForgeError::NotDetected)
        );
    }
}
