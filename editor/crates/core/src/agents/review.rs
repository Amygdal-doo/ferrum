//! Reviewing what an agent changed: per-file diffs against the
//! [`Snapshot`], split into hunks (changes with 3 lines of context, like
//! `git diff`), and accepting or rejecting a hunk, a file or everything.
//!
//! Accepting moves the snapshot's baseline; rejecting writes the baseline
//! back to disk. Every action names the content it was computed from (a
//! hash of the file on disk): when the agent has written the file since,
//! the action is refused as [`ReviewError::Stale`] and the review must be
//! refreshed first.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use similar::{ChangeTag, TextDiff};

use super::snapshot::{Snapshot, read};

const CONTEXT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
}

impl FileStatus {
    pub fn letter(self) -> &'static str {
        match self {
            FileStatus::Added => "A",
            FileStatus::Modified => "M",
            FileStatus::Deleted => "D",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HunkLine {
    pub kind: LineKind,
    /// Without its line ending.
    pub text: String,
    /// 1-based line numbers.
    pub old_no: Option<usize>,
    pub new_no: Option<usize>,
}

/// A run of changes with context. Ranges are 0-based line indexes; the
/// context lines are inside them (equal on both sides).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// Stable for the same change at the same place.
    pub id: u64,
    pub old_start: usize,
    pub old_len: usize,
    pub new_start: usize,
    pub new_len: usize,
    pub lines: Vec<HunkLine>,
}

impl Hunk {
    /// `@@ -1,3 +1,4 @@`.
    pub fn header(&self) -> String {
        format!(
            "@@ -{},{} +{},{} @@",
            self.old_start + 1,
            self.old_len,
            self.new_start + 1,
            self.new_len
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// Relative to the project root.
    pub path: PathBuf,
    pub status: FileStatus,
    pub additions: usize,
    pub deletions: usize,
    /// Empty for binary files (they are accepted or rejected whole).
    pub hunks: Vec<Hunk>,
    pub binary: bool,
    /// Hash of the content on disk the hunks were computed from.
    pub current_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewError {
    /// The file changed since the review was computed.
    Stale(PathBuf),
    /// The hunk is no longer there.
    NoSuchHunk,
    Io(String),
}

impl std::fmt::Display for ReviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReviewError::Stale(path) => write!(
                f,
                "{} changed while you were reviewing; the review was refreshed",
                path.display()
            ),
            ReviewError::NoSuchHunk => write!(f, "that change is no longer there"),
            ReviewError::Io(error) => write!(f, "{error}"),
        }
    }
}

pub fn content_hash(content: Option<&[u8]>) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

fn as_text(bytes: Option<&[u8]>) -> Option<&str> {
    match bytes {
        None => Some(""),
        Some(bytes) if bytes[..bytes.len().min(8192)].contains(&0) => None,
        Some(bytes) => std::str::from_utf8(bytes).ok(),
    }
}

/// The change from `base` to `current` (`None` = missing), or `None` when
/// they are equal.
pub fn diff_file(path: &Path, base: Option<&[u8]>, current: Option<&[u8]>) -> Option<FileChange> {
    if base == current {
        return None;
    }
    let status = match (base, current) {
        (None, _) => FileStatus::Added,
        (_, None) => FileStatus::Deleted,
        _ => FileStatus::Modified,
    };
    let mut change = FileChange {
        path: path.to_path_buf(),
        status,
        additions: 0,
        deletions: 0,
        hunks: Vec::new(),
        binary: false,
        current_hash: content_hash(current),
    };
    let (Some(old), Some(new)) = (as_text(base), as_text(current)) else {
        change.binary = true;
        return Some(change);
    };
    change.hunks = hunks(old, new);
    for line in change.hunks.iter().flat_map(|h| &h.lines) {
        match line.kind {
            LineKind::Added => change.additions += 1,
            LineKind::Removed => change.deletions += 1,
            LineKind::Context => {}
        }
    }
    Some(change)
}

/// Hunks of a line diff, each with up to 3 lines of context.
pub fn hunks(old: &str, new: &str) -> Vec<Hunk> {
    let diff = TextDiff::from_lines(old, new);
    let mut out = Vec::new();
    for group in diff.grouped_ops(CONTEXT) {
        let (Some(first), Some(last)) = (group.first(), group.last()) else {
            continue;
        };
        let old_start = first.old_range().start;
        let new_start = first.new_range().start;
        let mut hunk = Hunk {
            id: 0,
            old_start,
            old_len: last.old_range().end - old_start,
            new_start,
            new_len: last.new_range().end - new_start,
            lines: Vec::new(),
        };
        for op in &group {
            for change in diff.iter_changes(op) {
                let kind = match change.tag() {
                    ChangeTag::Equal => LineKind::Context,
                    ChangeTag::Insert => LineKind::Added,
                    ChangeTag::Delete => LineKind::Removed,
                };
                hunk.lines.push(HunkLine {
                    kind,
                    text: change
                        .value()
                        .trim_end_matches('\n')
                        .trim_end_matches('\r')
                        .to_string(),
                    old_no: change.old_index().map(|i| i + 1),
                    new_no: change.new_index().map(|i| i + 1),
                });
            }
        }
        let mut hasher = DefaultHasher::new();
        hunk.old_start.hash(&mut hasher);
        for line in &hunk.lines {
            (line.kind as u8, &line.text).hash(&mut hasher);
        }
        hunk.id = hasher.finish();
        out.push(hunk);
    }
    out
}

/// `target` with lines `target_range` replaced by `source`'s
/// `source_range` (line endings kept as they are).
fn splice(
    target: &str,
    target_start: usize,
    target_len: usize,
    source: &str,
    source_start: usize,
    source_len: usize,
) -> String {
    let target: Vec<&str> = target.split_inclusive('\n').collect();
    let source: Vec<&str> = source.split_inclusive('\n').collect();
    let end = (target_start + target_len).min(target.len());
    let mut out = String::new();
    out.extend(target[..target_start.min(target.len())].iter().copied());
    out.extend(source.iter().skip(source_start).take(source_len).copied());
    out.extend(target[end..].iter().copied());
    out
}

/// `current` with `hunk` undone (its lines back as in `base`).
pub fn revert_hunk(base: &str, current: &str, hunk: &Hunk) -> String {
    splice(
        current,
        hunk.new_start,
        hunk.new_len,
        base,
        hunk.old_start,
        hunk.old_len,
    )
}

/// `base` with `hunk` applied: the baseline once the hunk is accepted.
pub fn accept_hunk(base: &str, current: &str, hunk: &Hunk) -> String {
    splice(
        base,
        hunk.old_start,
        hunk.old_len,
        current,
        hunk.new_start,
        hunk.new_len,
    )
}

/// Every changed file. Blocks (git, disk).
pub fn changes(snapshot: &Snapshot) -> Result<Vec<FileChange>, String> {
    Ok(snapshot
        .changed_paths()?
        .into_iter()
        .filter_map(|rel| file_change(snapshot, &rel))
        .collect())
}

/// The change of one file, if it differs from the baseline.
pub fn file_change(snapshot: &Snapshot, rel: &Path) -> Option<FileChange> {
    let base = snapshot.base(rel);
    let current = read(&snapshot.root.join(rel));
    diff_file(rel, base.as_deref(), current.as_deref())
}

/// The file's text on disk if it still hashes to `expected`.
fn current_checked(
    snapshot: &Snapshot,
    rel: &Path,
    expected: u64,
) -> Result<Option<Vec<u8>>, ReviewError> {
    let current = read(&snapshot.root.join(rel));
    if content_hash(current.as_deref()) != expected {
        return Err(ReviewError::Stale(rel.to_path_buf()));
    }
    Ok(current)
}

fn find_hunk(base: &str, current: &str, id: u64) -> Result<Hunk, ReviewError> {
    hunks(base, current)
        .into_iter()
        .find(|h| h.id == id)
        .ok_or(ReviewError::NoSuchHunk)
}

fn texts(base: Option<&[u8]>, current: Option<&[u8]>) -> Result<(String, String), ReviewError> {
    match (as_text(base), as_text(current)) {
        (Some(b), Some(c)) => Ok((b.to_string(), c.to_string())),
        _ => Err(ReviewError::Io("binary files are reviewed whole".into())),
    }
}

/// What a rejection did to the file on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Written {
    Content(Vec<u8>),
    Deleted,
}

fn write(snapshot: &Snapshot, rel: &Path, content: Option<&[u8]>) -> Result<Written, ReviewError> {
    let path = snapshot.root.join(rel);
    match content {
        Some(bytes) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| ReviewError::Io(e.to_string()))?;
            }
            crate::files::write_atomic(&path, bytes)
                .map_err(|e| ReviewError::Io(format!("{}: {e}", rel.display())))?;
            Ok(Written::Content(bytes.to_vec()))
        }
        None => {
            if path.exists() {
                std::fs::remove_file(&path)
                    .map_err(|e| ReviewError::Io(format!("{}: {e}", rel.display())))?;
            }
            Ok(Written::Deleted)
        }
    }
}

/// Keeps one hunk: the baseline takes it.
pub fn accept_hunk_in(
    snapshot: &mut Snapshot,
    rel: &Path,
    hunk_id: u64,
    expected: u64,
) -> Result<(), ReviewError> {
    let current = current_checked(snapshot, rel, expected)?;
    let base = snapshot.base(rel);
    if base.is_none() || current.is_none() {
        // Adding or deleting a file is one change.
        snapshot.accept(rel, current);
        return Ok(());
    }
    let (base_text, current_text) = texts(base.as_deref(), current.as_deref())?;
    let hunk = find_hunk(&base_text, &current_text, hunk_id)?;
    let accepted = accept_hunk(&base_text, &current_text, &hunk);
    snapshot.accept(rel, Some(accepted.into_bytes()));
    Ok(())
}

/// Undoes one hunk on disk. Returns what the file now holds.
pub fn reject_hunk_in(
    snapshot: &mut Snapshot,
    rel: &Path,
    hunk_id: u64,
    expected: u64,
) -> Result<Written, ReviewError> {
    let current = current_checked(snapshot, rel, expected)?;
    let base = snapshot.base(rel);
    if base.is_none() || current.is_none() {
        return write(snapshot, rel, base.as_deref());
    }
    let (base_text, current_text) = texts(base.as_deref(), current.as_deref())?;
    let hunk = find_hunk(&base_text, &current_text, hunk_id)?;
    let reverted = revert_hunk(&base_text, &current_text, &hunk);
    write(snapshot, rel, Some(reverted.as_bytes()))
}

/// Keeps the whole file as it is now.
pub fn accept_file_in(
    snapshot: &mut Snapshot,
    rel: &Path,
    expected: u64,
) -> Result<(), ReviewError> {
    let current = current_checked(snapshot, rel, expected)?;
    snapshot.accept(rel, current);
    Ok(())
}

/// Restores the whole file (deleting it if the agent created it).
pub fn reject_file_in(
    snapshot: &mut Snapshot,
    rel: &Path,
    expected: u64,
) -> Result<Written, ReviewError> {
    current_checked(snapshot, rel, expected)?;
    let base = snapshot.base(rel);
    write(snapshot, rel, base.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::snapshot::tests::{have_git, repo};

    const BASE: &str = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\n";

    #[test]
    fn hunks_have_context_and_line_numbers() {
        let new = "a\nB\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\n";
        let hunks = hunks(BASE, new);
        assert_eq!(hunks.len(), 2, "{hunks:#?}");
        let first = &hunks[0];
        assert_eq!(first.header(), "@@ -1,5 +1,5 @@");
        let kinds: Vec<LineKind> = first.lines.iter().map(|l| l.kind).collect();
        assert_eq!(
            kinds,
            [
                LineKind::Context,
                LineKind::Removed,
                LineKind::Added,
                LineKind::Context,
                LineKind::Context,
                LineKind::Context
            ]
        );
        assert_eq!(first.lines[2].new_no, Some(2));
        assert_eq!(first.lines[1].old_no, Some(2));
        assert_eq!(hunks[1].lines.last().map(|l| l.text.as_str()), Some("m"));
        assert_ne!(hunks[0].id, hunks[1].id);
        assert_eq!(super::hunks(BASE, new)[0].id, hunks[0].id, "ids are stable");
    }

    #[test]
    fn revert_and_accept_one_hunk_of_two() {
        let new = "a\nB\nc\nd\ne\nf\ng\nh\ni\nj\nK\nl\n";
        let hunks = hunks(BASE, new);
        assert_eq!(hunks.len(), 2);
        let reverted = revert_hunk(BASE, new, &hunks[0]);
        assert_eq!(reverted, "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nK\nl\n");
        let accepted = accept_hunk(BASE, new, &hunks[1]);
        assert_eq!(accepted, reverted, "the other hunk is all that is left");
        // Line endings and a missing final newline survive.
        let crlf = revert_hunk(
            "x\r\ny\r\nz",
            "x\r\nY\r\nz",
            &super::hunks("x\r\ny\r\nz", "x\r\nY\r\nz")[0],
        );
        assert_eq!(crlf, "x\r\ny\r\nz");
    }

    #[test]
    fn files_added_deleted_and_binary() {
        let added = diff_file(Path::new("n.rs"), None, Some(b"x\ny\n")).expect("change");
        assert_eq!(
            (added.status, added.additions, added.deletions),
            (FileStatus::Added, 2, 0)
        );
        let deleted = diff_file(Path::new("o.rs"), Some(b"x\n"), None).expect("change");
        assert_eq!(
            (deleted.status, deleted.deletions),
            (FileStatus::Deleted, 1)
        );
        let binary = diff_file(Path::new("b"), Some(&[0, 1]), Some(&[0, 2])).expect("change");
        assert!(binary.binary && binary.hunks.is_empty());
        assert!(diff_file(Path::new("same"), Some(b"x"), Some(b"x")).is_none());
    }

    /// The whole flow on a real repository: the "agent" edits two places
    /// in one file, adds a file and deletes one; one hunk is accepted, the
    /// other rejected, the new file rejected (deleted), the deletion
    /// accepted; a stale action is refused.
    #[test]
    fn accept_and_reject_against_a_git_snapshot() {
        if !have_git() {
            return;
        }
        let dir = repo();
        let root = dir.path().canonicalize().expect("canonical");
        std::fs::write(root.join("a.txt"), BASE).expect("write");
        let mut snap = Snapshot::take(&root).expect("snapshot");

        std::fs::write(root.join("a.txt"), "a\nB\nc\nd\ne\nf\ng\nh\ni\nj\nK\nl\n").expect("write");
        std::fs::write(root.join("new.rs"), "fn main() {}\n").expect("write");
        std::fs::remove_file(root.join("notes.md")).expect("rm");

        let all = changes(&snap).expect("changes");
        let summary: Vec<(String, FileStatus, usize, usize)> = all
            .iter()
            .map(|c| {
                (
                    c.path.to_string_lossy().to_string(),
                    c.status,
                    c.additions,
                    c.deletions,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("a.txt".into(), FileStatus::Modified, 2, 2),
                ("new.rs".into(), FileStatus::Added, 1, 0),
                ("notes.md".into(), FileStatus::Deleted, 0, 1),
            ]
        );
        let a = &all[0];
        assert_eq!(a.hunks.len(), 2);
        let rel = Path::new("a.txt");

        // Accept the first hunk: disk unchanged, only the second remains.
        accept_hunk_in(&mut snap, rel, a.hunks[0].id, a.current_hash).expect("accept");
        let a = file_change(&snap, rel).expect("still changed");
        assert_eq!(a.hunks.len(), 1);
        assert_eq!(
            a.hunks[0]
                .lines
                .iter()
                .filter(|l| l.kind == LineKind::Added)
                .count(),
            1
        );

        // A stale hash is refused, and nothing is written.
        assert_eq!(
            reject_hunk_in(&mut snap, rel, a.hunks[0].id, 42),
            Err(ReviewError::Stale(rel.to_path_buf()))
        );
        // Reject the second: the file keeps the accepted B, loses K.
        let written =
            reject_hunk_in(&mut snap, rel, a.hunks[0].id, a.current_hash).expect("reject");
        let expected = "a\nB\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\n";
        assert_eq!(written, Written::Content(expected.as_bytes().to_vec()));
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).expect("read"),
            expected
        );
        assert!(file_change(&snap, rel).is_none());

        // Reject the added file (deleted), accept the deletion.
        let new = file_change(&snap, Path::new("new.rs")).expect("added");
        assert_eq!(
            reject_file_in(&mut snap, Path::new("new.rs"), new.current_hash),
            Ok(Written::Deleted)
        );
        assert!(!root.join("new.rs").exists());
        let gone = file_change(&snap, Path::new("notes.md")).expect("deleted");
        accept_file_in(&mut snap, Path::new("notes.md"), gone.current_hash).expect("accept");
        assert!(changes(&snap).expect("changes").is_empty());

        // Rejecting a deletion brings the file back.
        let mut snap = Snapshot::take(&root).expect("snapshot");
        std::fs::remove_file(root.join("a.txt")).expect("rm");
        let change = file_change(&snap, rel).expect("deleted");
        reject_file_in(&mut snap, rel, change.current_hash).expect("restore");
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).expect("read"),
            expected
        );
    }
}
