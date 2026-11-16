//! `git status --porcelain=v2 --branch -z --untracked-files=all`

use ferrumgit_domain::{BranchInfo, ChangeKind, FileStatus, Oid, WorkingStatus};

fn kind(c: u8) -> ChangeKind {
    match c {
        b'M' => ChangeKind::Modified,
        b'A' => ChangeKind::Added,
        b'D' => ChangeKind::Deleted,
        b'R' => ChangeKind::Renamed,
        b'C' => ChangeKind::Copied,
        b'T' => ChangeKind::TypeChanged,
        _ => ChangeKind::Unmodified,
    }
}

/// Parses records; `operation` is filled in by the caller (from the git dir).
#[must_use]
pub fn parse_status(raw: &[u8]) -> WorkingStatus {
    let text = String::from_utf8_lossy(raw);
    let mut records = text.split('\0').filter(|r| !r.is_empty());
    let mut branch = BranchInfo::default();
    let mut files = Vec::new();
    while let Some(rec) = records.next() {
        if let Some(h) = rec.strip_prefix("# ") {
            let (key, value) = h.split_once(' ').unwrap_or((h, ""));
            match key {
                "branch.oid" if value != "(initial)" => branch.oid = Some(Oid::new(value)),
                "branch.head" if value != "(detached)" => branch.head = Some(value.to_owned()),
                "branch.upstream" => branch.upstream = Some(value.to_owned()),
                "branch.ab" => {
                    for part in value.split(' ') {
                        if let Some(a) = part.strip_prefix('+') {
                            branch.ahead = a.parse().unwrap_or(0);
                        } else if let Some(b) = part.strip_prefix('-') {
                            branch.behind = b.parse().unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        let tag = rec.as_bytes()[0];
        match tag {
            b'1' | b'2' | b'u' => {
                // Fixed fields before the path: 1 → 8, 2 → 9, u → 10.
                let fields = match tag {
                    b'1' => 8,
                    b'2' => 9,
                    _ => 10,
                };
                let mut parts = rec.splitn(fields + 1, ' ');
                let xy = parts.nth(1).unwrap_or("..").as_bytes();
                let Some(path) = parts.nth(fields - 2) else {
                    continue;
                };
                let (x, y) = (
                    xy.first().copied().unwrap_or(b'.'),
                    xy.get(1).copied().unwrap_or(b'.'),
                );
                let mut f = FileStatus {
                    path: path.to_owned(),
                    ..FileStatus::default()
                };
                if tag == b'u' {
                    f.index = ChangeKind::Conflicted;
                    f.worktree = ChangeKind::Conflicted;
                } else {
                    f.index = kind(x);
                    f.worktree = kind(y);
                }
                if tag == b'2' {
                    f.orig_path = records.next().map(str::to_owned);
                }
                files.push(f);
            }
            b'?' => files.push(FileStatus {
                path: rec[2..].to_owned(),
                orig_path: None,
                index: ChangeKind::Untracked,
                worktree: ChangeKind::Untracked,
            }),
            _ => {}
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    WorkingStatus {
        branch,
        files,
        ..WorkingStatus::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_branch_and_entries() {
        let raw = b"# branch.oid 1111111111111111111111111111111111111111\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0\
1 .M N... 100644 100644 100644 aaaa bbbb src/lib.rs\0\
1 A. N... 000000 100644 100644 0000 cccc new file.txt\0\
2 R. N... 100644 100644 100644 dddd eeee R100 renamed.rs\0old.rs\0\
u UU N... 100644 100644 100644 100644 a1 b2 c3 conflict.txt\0\
? untracked/dir/x.md\0";
        let s = parse_status(raw);
        assert_eq!(s.branch.head.as_deref(), Some("main"));
        assert_eq!(s.branch.upstream.as_deref(), Some("origin/main"));
        assert_eq!((s.branch.ahead, s.branch.behind), (2, 1));
        assert_eq!(s.files.len(), 5);
        let by = |p: &str| s.files.iter().find(|f| f.path == p).expect(p);
        assert_eq!(by("src/lib.rs").worktree, ChangeKind::Modified);
        assert!(!by("src/lib.rs").is_staged());
        assert_eq!(by("new file.txt").index, ChangeKind::Added);
        assert_eq!(by("renamed.rs").orig_path.as_deref(), Some("old.rs"));
        assert!(by("conflict.txt").is_conflicted());
        assert!(by("untracked/dir/x.md").is_unstaged());
        assert_eq!(s.staged().len(), 2);
        assert_eq!(s.unstaged().len(), 3);
    }

    #[test]
    fn initial_and_detached() {
        let s = parse_status(b"# branch.oid (initial)\0# branch.head (detached)\0");
        assert!(s.branch.oid.is_none());
        assert!(s.branch.head.is_none());
        assert!(s.is_clean());
    }
}
