//! What changed in a folder since its baseline snapshot, and a file's
//! diff against it in FerrumGIT's diff model (so the same hunk view
//! renders it).

use super::scan::Snapshot;
use ferrumgit_domain::{ChangeKind, DiffLine, FileDiff, Hunk, LineKind};
use similar::{ChangeTag, TextDiff};
use std::time::Duration;

/// Lines of context around each change.
const CONTEXT: usize = 3;

/// A file that differs from the baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// `/`-separated, relative to the folder.
    pub path: String,
    /// `Added`, `Modified` or `Deleted`.
    pub kind: ChangeKind,
}

/// The files added, modified or deleted between `baseline` and `current`,
/// by path.
#[must_use]
pub fn changes(baseline: &Snapshot, current: &Snapshot) -> Vec<Change> {
    let mut out: Vec<Change> = current
        .files
        .iter()
        .filter_map(|(path, entry)| {
            let kind = match baseline.files.get(path) {
                None => ChangeKind::Added,
                Some(old) if old.text != entry.text => ChangeKind::Modified,
                Some(_) => return None,
            };
            Some(Change {
                path: path.clone(),
                kind,
            })
        })
        .collect();
    out.extend(
        baseline
            .files
            .keys()
            .filter(|path| !current.files.contains_key(*path))
            .map(|path| Change {
                path: path.clone(),
                kind: ChangeKind::Deleted,
            }),
    );
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// `path`'s diff from its baseline text to its current text (an added file
/// has no baseline, a deleted one no current text).
#[must_use]
pub fn file_diff(path: &str, baseline: Option<&str>, current: Option<&str>) -> FileDiff {
    let change = match (baseline, current) {
        (None, _) => ChangeKind::Added,
        (_, None) => ChangeKind::Deleted,
        _ => ChangeKind::Modified,
    };
    let old = baseline.unwrap_or_default();
    let new = current.unwrap_or_default();
    let diff = TextDiff::configure()
        .timeout(Duration::from_millis(500))
        .diff_lines(old, new);
    let mut file = FileDiff {
        path: path.to_owned(),
        change,
        ..FileDiff::default()
    };
    for group in diff.grouped_ops(CONTEXT) {
        let (Some(first), Some(last)) = (group.first(), group.last()) else {
            continue;
        };
        let old_start = first.old_range().start;
        let new_start = first.new_range().start;
        let old_lines = last.old_range().end - old_start;
        let new_lines = last.new_range().end - new_start;
        let mut hunk = Hunk {
            header: format!(
                "@@ -{},{old_lines} +{},{new_lines} @@",
                old_start + 1,
                new_start + 1
            ),
            old_start: line_no(old_start + 1),
            old_lines: line_no(old_lines),
            new_start: line_no(new_start + 1),
            new_lines: line_no(new_lines),
            lines: Vec::new(),
        };
        for op in &group {
            for change in diff.iter_changes(op) {
                let kind = match change.tag() {
                    ChangeTag::Equal => LineKind::Context,
                    ChangeTag::Insert => {
                        file.additions += 1;
                        LineKind::Added
                    }
                    ChangeTag::Delete => {
                        file.deletions += 1;
                        LineKind::Removed
                    }
                };
                hunk.lines.push(DiffLine {
                    kind,
                    content: change
                        .value()
                        .trim_end_matches('\n')
                        .trim_end_matches('\r')
                        .to_owned(),
                    old_no: change.old_index().map(|i| line_no(i + 1)),
                    new_no: change.new_index().map(|i| line_no(i + 1)),
                });
            }
        }
        file.hunks.push(hunk);
    }
    file
}

fn line_no(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}
