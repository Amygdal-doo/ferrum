//! `git for-each-ref` and `git remote -v`.

use ferrumgit_domain::{GitRef, Oid, RefKind, Remote};

/// Fields (NUL-separated): refname, objectname, peeled objectname, upstream, track, HEAD marker.
pub const REFS_FORMAT: &str =
    "%(refname)%00%(objectname)%00%(*objectname)%00%(upstream:short)%00%(upstream:track)%00%(HEAD)";

fn track(s: &str) -> (u32, u32, bool) {
    let inner = s.trim().trim_start_matches('[').trim_end_matches(']');
    if inner == "gone" {
        return (0, 0, true);
    }
    let (mut ahead, mut behind) = (0, 0);
    for part in inner.split(", ") {
        if let Some(n) = part.strip_prefix("ahead ") {
            ahead = n.parse().unwrap_or(0);
        } else if let Some(n) = part.strip_prefix("behind ") {
            behind = n.parse().unwrap_or(0);
        }
    }
    (ahead, behind, false)
}

#[must_use]
pub fn parse_refs(raw: &[u8]) -> Vec<GitRef> {
    let text = String::from_utf8_lossy(raw);
    let mut refs: Vec<GitRef> = text
        .lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\0').collect();
            if f.len() < 6 {
                return None;
            }
            let full = f[0];
            let (kind, name) = if let Some(n) = full.strip_prefix("refs/heads/") {
                (RefKind::LocalBranch, n)
            } else if let Some(n) = full.strip_prefix("refs/remotes/") {
                // `origin/HEAD` is a symbolic pointer, not a branch.
                if n.ends_with("/HEAD") {
                    return None;
                }
                (RefKind::RemoteBranch, n)
            } else {
                (RefKind::Tag, full.strip_prefix("refs/tags/")?)
            };
            let target = if f[2].is_empty() { f[1] } else { f[2] };
            let (ahead, behind, gone) = track(f[4]);
            Some(GitRef {
                name: name.to_owned(),
                full_name: full.to_owned(),
                target: Oid::new(target),
                kind: Some(kind),
                upstream: (!f[3].is_empty()).then(|| f[3].to_owned()),
                ahead,
                behind,
                upstream_gone: gone,
                is_head: f[5].trim() == "*",
                merged: false,
            })
        })
        .collect();
    refs.sort_by_key(|r| (r.kind, r.name.to_lowercase()));
    refs
}

/// `git remote -v`: `origin\tgit@host:o/r.git (fetch)`.
#[must_use]
pub fn parse_remotes(raw: &[u8]) -> Vec<Remote> {
    let mut out: Vec<Remote> = Vec::new();
    for line in String::from_utf8_lossy(raw).lines() {
        let Some((name, rest)) = line.split_once('\t') else {
            continue;
        };
        let (url, kind) = rest.rsplit_once(' ').unwrap_or((rest, "(fetch)"));
        let idx = match out.iter().position(|r| r.name == name) {
            Some(i) => i,
            None => {
                out.push(Remote {
                    name: name.to_owned(),
                    ..Remote::default()
                });
                out.len() - 1
            }
        };
        if kind == "(push)" {
            out[idx].push_url = url.to_owned();
        } else {
            out[idx].fetch_url = url.to_owned();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_branches_tags_and_remotes() {
        let raw = "refs/heads/main\0aaa\0\0origin/main\0[ahead 1, behind 3]\0*\n\
refs/heads/old\0bbb\0\0origin/old\0[gone]\0 \n\
refs/remotes/origin/HEAD\0aaa\0\0\0\0 \n\
refs/remotes/origin/main\0ccc\0\0\0\0 \n\
refs/tags/v1.0\0ttt\0ddd\0\0\0 \n";
        let refs = parse_refs(raw.as_bytes());
        assert_eq!(refs.len(), 4);
        let main = &refs[0];
        assert!(main.is_head && main.is(RefKind::LocalBranch));
        assert_eq!((main.ahead, main.behind), (1, 3));
        assert!(refs[1].upstream_gone);
        assert_eq!(refs[2].remote_parts(), Some(("origin", "main")));
        assert_eq!(refs[3].target.as_str(), "ddd", "annotated tags are peeled");
    }

    #[test]
    fn parses_remote_v() {
        let raw = b"origin\tgit@github.com:o/r.git (fetch)\norigin\tgit@github.com:o/r.git (push)\nup\thttps://x/y (fetch)\nup\tno_push (push)\n";
        let r = parse_remotes(raw);
        assert_eq!(r.len(), 2);
        assert!(r[0].is_ssh());
        assert_eq!(r[1].push_url, "no_push");
    }
}
