//! `git log --follow --name-status`, `git blame --porcelain` and the HEAD reflog.

use super::{FIELD, log::commit_from};
use ferrumgit_domain::{Blame, BlameCommit, BlameLine, ChangeKind, FileCommit, Oid};

/// Record separator put before each commit (`%x1e`) so the NUL-separated
/// name-status fields can be told apart from the next commit.
pub const RECORD: char = '\u{1e}';
/// `%x1e` + [`super::COMMIT_FORMAT`].
pub const FILE_LOG_FORMAT: &str = "%x1e%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%s";
/// Fields: new oid, reflog subject.
pub const REFLOG_FORMAT: &str = "%H%x1f%gs";

fn change_kind(status: &str) -> ChangeKind {
    match status.chars().next() {
        Some('A') => ChangeKind::Added,
        Some('D') => ChangeKind::Deleted,
        Some('R') => ChangeKind::Renamed,
        Some('C') => ChangeKind::Copied,
        Some('T') => ChangeKind::TypeChanged,
        Some('M') => ChangeKind::Modified,
        _ => ChangeKind::Unmodified,
    }
}

/// `git log -z --follow --name-status --format=FILE_LOG_FORMAT -- <path>`.
/// `path` is the file's current name; merges list no change, so they keep the
/// path of the commit after them (older commits come later).
#[must_use]
pub fn parse_file_log(raw: &[u8], path: &str) -> Vec<FileCommit> {
    let text = String::from_utf8_lossy(raw);
    let mut current = path.to_owned();
    let mut out = Vec::new();
    for rec in text.split(RECORD).filter(|r| !r.trim().is_empty()) {
        let mut parts = rec.split('\0');
        let header = parts.next().unwrap_or_default();
        let f: Vec<&str> = header.splitn(6, FIELD).collect();
        let Some(commit) = commit_from(&f) else {
            continue;
        };
        let rest: Vec<&str> = parts
            .map(|p| p.trim_start_matches('\n'))
            .filter(|p| !p.is_empty())
            .collect();
        let (change, old_path, new_path) = match rest.as_slice() {
            [status, from, to, ..] if status.starts_with(['R', 'C']) => (
                change_kind(status),
                Some((*from).to_owned()),
                (*to).to_owned(),
            ),
            [status, p, ..] => (change_kind(status), None, (*p).to_owned()),
            _ => (ChangeKind::Unmodified, None, current.clone()),
        };
        // Older commits know the file by its name before this rename.
        current = old_path.clone().unwrap_or_else(|| new_path.clone());
        out.push(FileCommit {
            commit,
            path: new_path,
            old_path,
            change,
        });
    }
    out
}

/// `git blame --porcelain [<rev>] -- <path>`.
#[must_use]
pub fn parse_blame(raw: &[u8], path: &str) -> Blame {
    let text = String::from_utf8_lossy(raw);
    let mut blame = Blame {
        path: path.to_owned(),
        ..Default::default()
    };
    let mut oid: Option<(Oid, u32)> = None;
    for line in text.lines() {
        if let Some(content) = line.strip_prefix('\t') {
            if let Some((o, n)) = oid.take() {
                blame.lines.push(BlameLine {
                    oid: o,
                    line_no: n,
                    content: content.to_owned(),
                });
            }
            continue;
        }
        let mut words = line.splitn(2, ' ');
        let key = words.next().unwrap_or_default();
        let value = words.next().unwrap_or_default();
        // Group header: `<sha> <orig line> <final line> [<lines in group>]`.
        if key.len() >= 40 && key.bytes().all(|b| b.is_ascii_hexdigit()) {
            let final_line = value
                .split(' ')
                .nth(1)
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            let o = Oid::new(key);
            blame
                .commits
                .entry(o.clone())
                .or_insert_with(|| BlameCommit {
                    path: path.to_owned(),
                    ..Default::default()
                });
            oid = Some((o, final_line));
            continue;
        }
        let Some(c) = oid.as_ref().and_then(|(o, _)| blame.commits.get_mut(o)) else {
            continue;
        };
        match key {
            "author" => c.author_name = value.to_owned(),
            "author-mail" => {
                c.author_email = value
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .to_owned();
            }
            "author-time" => c.author_time = value.parse().unwrap_or(0),
            "summary" => c.summary = value.to_owned(),
            "filename" => c.path = value.to_owned(),
            _ => {}
        }
    }
    blame
}

/// `git reflog -z --format=REFLOG_FORMAT HEAD`, newest first.
#[must_use]
pub fn parse_reflog(raw: &[u8]) -> Vec<(Oid, String)> {
    String::from_utf8_lossy(raw)
        .split('\0')
        .filter_map(|rec| {
            let (oid, subject) = rec.trim_start_matches('\n').split_once(FIELD)?;
            (oid.len() >= 40).then(|| (Oid::new(oid), subject.to_owned()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "cccccccccccccccccccccccccccccccccccccccc";
    const D: &str = "dddddddddddddddddddddddddddddddddddddddd";

    fn rec(oid: &str, parents: &str, summary: &str, changes: &str) -> String {
        format!("\x1e{oid}\x1f{parents}\x1fAda\x1fa@x\x1f100\x1f{summary}\0{changes}")
    }

    #[test]
    fn file_log_follows_renames_and_keeps_merges() {
        let raw = [
            rec(A, &format!("{B} {D}"), "Merge", ""),
            rec(B, C, "Edit", "\nM\0new.rs\0"),
            rec(C, D, "Move", "\nR087\0old.rs\0new.rs\0"),
            rec(D, "", "Create", "\nA\0old.rs\0"),
        ]
        .concat();
        let log = parse_file_log(raw.as_bytes(), "new.rs");
        assert_eq!(log.len(), 4);
        assert_eq!(log[0].change, ChangeKind::Unmodified);
        assert_eq!(log[0].path, "new.rs", "merge keeps the newer path");
        assert_eq!(log[1].change, ChangeKind::Modified);
        assert_eq!(log[2].change, ChangeKind::Renamed);
        assert_eq!(log[2].old_path.as_deref(), Some("old.rs"));
        assert_eq!(log[2].path, "new.rs");
        assert_eq!(log[3].path, "old.rs");
        assert_eq!(log[3].change, ChangeKind::Added);
        assert_eq!(log[3].commit.summary, "Create");
        assert!(log[3].commit.parents.is_empty());
    }

    #[test]
    fn blame_porcelain_groups_and_uncommitted_lines() {
        let z = "0".repeat(40);
        let raw = format!(
            "{A} 1 1 2\nauthor Ada\nauthor-mail <ada@x>\nauthor-time 100\nsummary First\nboundary\nfilename old.rs\n\tfn a() {{}}\n\
             {A} 2 2\n\t\n\
             {B} 1 3 1\nauthor Bob\nauthor-mail <bob@x>\nauthor-time 200\nsummary Second\nprevious {A} old.rs\nfilename new.rs\n\t// tab\there\n\
             {z} 4 4 1\nauthor Not Committed Yet\nauthor-mail <not.committed.yet>\nauthor-time 300\nsummary Version of new.rs from new.rs\nfilename new.rs\n\tlocal\n"
        );
        let b = parse_blame(raw.as_bytes(), "new.rs");
        assert_eq!(b.lines.len(), 4);
        assert_eq!(b.lines[0].content, "fn a() {}");
        assert_eq!(b.lines[1].content, "");
        assert_eq!(b.lines[1].line_no, 2);
        assert_eq!(b.lines[2].content, "// tab\there");
        assert_eq!(b.lines[2].line_no, 3);
        let first = b.commit_of(&b.lines[0]).expect("commit");
        assert_eq!(first.author_name, "Ada");
        assert_eq!(first.author_email, "ada@x");
        assert_eq!(first.author_time, 100);
        assert_eq!(first.path, "old.rs");
        assert_eq!(b.commit_of(&b.lines[2]).expect("c").summary, "Second");
        assert!(b.lines[3].oid.is_zero());
        assert!(b.starts_group(0));
        assert!(!b.starts_group(1));
        assert!(b.starts_group(2));
        assert!(!b.starts_group(9));
    }

    #[test]
    fn reflog_entries() {
        let raw = format!("{B}\x1fcommit: Two\0{A}\x1fcheckout: moving from a to b\0");
        let r = parse_reflog(raw.as_bytes());
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].1, "commit: Two");
        assert_eq!(r[1].0.as_str(), A);
    }
}
