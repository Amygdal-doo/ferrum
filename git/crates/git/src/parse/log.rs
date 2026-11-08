//! `git log` / `git show -s` / `git stash list` with unit-separated fields.

use super::FIELD;
use ferrumgit_domain::{Commit, CommitDetails, Oid, Stash};

/// Fields: oid, parents, author name, author email, author time, summary.
pub const COMMIT_FORMAT: &str = "%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%s";
/// [`COMMIT_FORMAT`] plus committer name, email, time and body.
pub const DETAILS_FORMAT: &str = "%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%s%x1f%cn%x1f%ce%x1f%ct%x1f%b";
/// Fields: oid, time, reflog subject.
pub const STASH_FORMAT: &str = "%H%x1f%at%x1f%gs";

fn commit_from(f: &[&str]) -> Option<Commit> {
    if f.len() < 6 || f[0].len() < 7 {
        return None;
    }
    Some(Commit {
        oid: Oid::new(f[0].trim()),
        parents: f[1].split_whitespace().map(Oid::new).collect(),
        author_name: f[2].to_owned(),
        author_email: f[3].to_owned(),
        author_time: f[4].trim().parse().unwrap_or(0),
        summary: f[5].to_owned(),
    })
}

/// `git log -z --format=COMMIT_FORMAT` (records separated by NUL).
#[must_use]
pub fn parse_log(raw: &[u8]) -> Vec<Commit> {
    String::from_utf8_lossy(raw)
        .split('\0')
        .filter_map(|rec| {
            let rec = rec.trim_start_matches('\n');
            let f: Vec<&str> = rec.splitn(6, FIELD).collect();
            commit_from(&f)
        })
        .collect()
}

/// `git show -s --format=DETAILS_FORMAT <oid>`; files are filled in later.
#[must_use]
pub fn parse_details_header(raw: &[u8]) -> Option<CommitDetails> {
    let text = String::from_utf8_lossy(raw);
    let f: Vec<&str> = text.splitn(10, FIELD).collect();
    if f.len() < 10 {
        return None;
    }
    Some(CommitDetails {
        commit: commit_from(&f[..6])?,
        committer_name: f[6].to_owned(),
        committer_email: f[7].to_owned(),
        commit_time: f[8].trim().parse().unwrap_or(0),
        body: f[9].trim_end().to_owned(),
        files: Vec::new(),
        truncated: false,
    })
}

/// `git stash list -z --format=STASH_FORMAT`; index = position.
#[must_use]
pub fn parse_stashes(raw: &[u8]) -> Vec<Stash> {
    String::from_utf8_lossy(raw)
        .split('\0')
        .filter(|r| !r.trim().is_empty())
        .enumerate()
        .filter_map(|(index, rec)| {
            let f: Vec<&str> = rec.trim_start_matches('\n').splitn(3, FIELD).collect();
            (f.len() == 3).then(|| Stash {
                index,
                oid: Oid::new(f[0]),
                time: f[1].parse().unwrap_or(0),
                message: f[2].to_owned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "cccccccccccccccccccccccccccccccccccccccc";

    #[test]
    fn parses_log_records() {
        let raw = format!(
            "{A}\x1f{B} {C}\x1fAda\x1fada@x.io\x1f1700000000\x1fMerge branch 'x'\0{B}\x1f\x1fBob\x1fb@x\x1f1600000000\x1fRoot: with \x1f? no\0"
        );
        let log = parse_log(raw.as_bytes());
        assert_eq!(log.len(), 2);
        assert!(log[0].is_merge());
        assert_eq!(log[0].parents[1].as_str(), C);
        assert!(log[1].parents.is_empty());
        assert_eq!(log[1].summary, "Root: with \x1f? no");
    }

    #[test]
    fn parses_details_with_multiline_body() {
        let raw = format!(
            "{A}\x1f{B}\x1fAda\x1fa@x\x1f1\x1fSummary\x1fCo\x1fc@x\x1f2\x1fLine 1\n\nLine 3\n"
        );
        let d = parse_details_header(raw.as_bytes()).expect("details");
        assert_eq!(d.commit.summary, "Summary");
        assert_eq!(d.committer_name, "Co");
        assert_eq!(d.commit_time, 2);
        assert_eq!(d.body, "Line 1\n\nLine 3");
    }

    #[test]
    fn parses_stash_list() {
        let raw = format!("{A}\x1f10\x1fOn main: wip\0{B}\x1f5\x1fWIP on dev: abc msg\0");
        let s = parse_stashes(raw.as_bytes());
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].index, 1);
        assert_eq!(s[1].refname(), "stash@{1}");
        assert_eq!(s[0].message, "On main: wip");
    }
}
