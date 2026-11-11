//! A project folder → its git remotes → the GitHub repository, read from
//! `.git/config` directly (no git process, works for worktrees too).

use ferrumgit_domain::{ForgeKind, RepoForge};
use ferrumgit_forge::{ForgeRepo, resolve};
use std::path::{Path, PathBuf};

/// The top of the git repository containing `path`.
#[must_use]
pub fn git_root(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

/// The directory holding `config`: `.git`, or a worktree's common dir.
fn config_dir(root: &Path) -> Option<PathBuf> {
    let dot_git = root.join(".git");
    let git_dir = if dot_git.is_file() {
        let text = std::fs::read_to_string(&dot_git).ok()?;
        let dir = PathBuf::from(text.trim().strip_prefix("gitdir:")?.trim());
        if dir.is_absolute() {
            dir
        } else {
            root.join(dir)
        }
    } else {
        dot_git
    };
    match std::fs::read_to_string(git_dir.join("commondir")) {
        Ok(common) => {
            let common = PathBuf::from(common.trim());
            Some(if common.is_absolute() {
                common
            } else {
                git_dir.join(common)
            })
        }
        Err(_) => Some(git_dir),
    }
}

/// `(name, url)` for every `[remote "name"]` with a `url`.
#[must_use]
pub fn parse_remotes(config: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut current: Option<String> = None;
    for line in config.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            current = line
                .strip_prefix("[remote")
                .and_then(|r| r.trim().strip_prefix('"'))
                .and_then(|r| r.strip_suffix("\"]"))
                .map(str::to_owned);
            continue;
        }
        let Some(name) = &current else { continue };
        if let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("url")
            && !out.iter().any(|(n, _)| n == name)
        {
            out.push((name.clone(), value.trim().trim_matches('"').to_owned()));
        }
    }
    out
}

/// Remotes of the repository containing `path`.
#[must_use]
pub fn remotes(path: &Path) -> Vec<(String, String)> {
    git_root(path)
        .and_then(|root| config_dir(&root))
        .and_then(|dir| std::fs::read_to_string(dir.join("config")).ok())
        .map(|text| parse_remotes(&text))
        .unwrap_or_default()
}

/// What the CI page can do with a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoStatus {
    GitHub(ForgeRepo),
    /// GitLab pipelines are not supported.
    GitLab(ForgeRepo),
    NotGit,
    NoForge,
}

#[must_use]
pub fn detect(path: &Path) -> RepoStatus {
    if git_root(path).is_none() {
        return RepoStatus::NotGit;
    }
    from_remotes(&remotes(path))
}

#[must_use]
pub fn from_remotes(remotes: &[(String, String)]) -> RepoStatus {
    match resolve(remotes, &RepoForge::default()) {
        Ok(repo) if repo.kind == ForgeKind::GitHub => RepoStatus::GitHub(repo),
        Ok(repo) => RepoStatus::GitLab(repo),
        Err(_) => RepoStatus::NoForge,
    }
}

/// Folders (deduplicated by repository root) that have a GitHub remote.
#[must_use]
pub fn github_projects(paths: &[PathBuf]) -> Vec<(PathBuf, ForgeRepo)> {
    let mut out: Vec<(PathBuf, ForgeRepo)> = Vec::new();
    for path in paths {
        let Some(root) = git_root(path) else { continue };
        if out.iter().any(|(p, _)| *p == root) {
            continue;
        }
        if let RepoStatus::GitHub(repo) = detect(&root) {
            out.push((root, repo));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"[core]
	bare = false
[remote "upstream"]
	url = git@gitlab.com:g/p.git
	fetch = +refs/heads/*:refs/remotes/upstream/*
[remote "origin"]
	url = https://github.com/octo/app.git
[branch "main"]
	url = not-a-remote
"#;

    #[test]
    fn parses_config_remotes() {
        assert_eq!(
            parse_remotes(CONFIG),
            vec![
                ("upstream".into(), "git@gitlab.com:g/p.git".into()),
                ("origin".into(), "https://github.com/octo/app.git".into()),
            ]
        );
        match from_remotes(&parse_remotes(CONFIG)) {
            RepoStatus::GitHub(r) => assert_eq!(r.path(), "octo/app"),
            other => panic!("{other:?}"),
        }
        let gl = parse_remotes("[remote \"origin\"]\nurl = git@gitlab.com:g/p.git");
        assert!(matches!(from_remotes(&gl), RepoStatus::GitLab(_)));
        assert_eq!(from_remotes(&[]), RepoStatus::NoForge);
    }

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ferrumci-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    #[test]
    fn reads_repos_and_worktrees_from_disk() {
        let base = tmp("repo");
        let repo = base.join("app");
        std::fs::create_dir_all(repo.join(".git")).expect("git");
        std::fs::create_dir_all(repo.join("src")).expect("src");
        std::fs::write(repo.join(".git/config"), CONFIG).expect("config");
        assert!(matches!(detect(&repo.join("src")), RepoStatus::GitHub(_)));

        let wt = base.join("wt");
        let wt_git = repo.join(".git/worktrees/wt");
        std::fs::create_dir_all(&wt_git).expect("wt git");
        std::fs::create_dir_all(&wt).expect("wt");
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", wt_git.display())).expect("file");
        std::fs::write(wt_git.join("commondir"), "../..\n").expect("commondir");
        assert_eq!(remotes(&wt).len(), 2);

        let plain = base.join("plain");
        std::fs::create_dir_all(&plain).expect("plain");
        assert_eq!(detect(&plain), RepoStatus::NotGit);

        let found = github_projects(&[repo.join("src"), repo.clone(), plain, wt]);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0, repo);
        let _ = std::fs::remove_dir_all(&base);
    }
}
