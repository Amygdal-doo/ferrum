//! Parsed unified diffs.

use crate::status::ChangeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
    /// `\ No newline at end of file`
    NoNewline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    /// Without the leading `+`/`-`/` ` marker.
    pub content: String,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Hunk {
    /// The full `@@ -a,b +c,d @@ context` line.
    pub header: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileDiff {
    /// New path (old path for deletions).
    pub path: String,
    /// Previous path of a rename/copy.
    pub old_path: Option<String>,
    pub change: ChangeKind,
    pub binary: bool,
    /// Raw header lines (`diff --git`, `index`, `---`, `+++` …), needed to
    /// rebuild a patch for one hunk.
    pub header: Vec<String>,
    pub hunks: Vec<Hunk>,
    pub additions: u32,
    pub deletions: u32,
}

/// Which side of the repository a diff was taken from; decides the hunk actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffSource {
    /// Working tree vs index: stage / discard hunks.
    Unstaged,
    /// Index vs HEAD: unstage hunks.
    Staged,
    /// An untracked file shown as all-added.
    Untracked,
    /// A historical commit or stash (read-only).
    Commit,
}

/// One row of a side-by-side diff: indices into [`Hunk::lines`] for the old
/// (left) and new (right) side. Context lines appear on both sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitRow {
    pub left: Option<usize>,
    pub right: Option<usize>,
}

impl Hunk {
    /// Pairs the lines for a side-by-side view: within each run of changes,
    /// the n-th removed line sits next to the n-th added one; the longer side
    /// continues alone. `\ No newline` markers stay on the side of the line
    /// they belong to.
    #[must_use]
    pub fn split_rows(&self) -> Vec<SplitRow> {
        let mut rows = Vec::with_capacity(self.lines.len());
        let (mut removed, mut added): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
        let flush = |rows: &mut Vec<SplitRow>, removed: &mut Vec<usize>, added: &mut Vec<usize>| {
            for k in 0..removed.len().max(added.len()) {
                rows.push(SplitRow {
                    left: removed.get(k).copied(),
                    right: added.get(k).copied(),
                });
            }
            removed.clear();
            added.clear();
        };
        let mut last = LineKind::Context;
        for (i, line) in self.lines.iter().enumerate() {
            let kind = if line.kind == LineKind::NoNewline {
                last
            } else {
                line.kind
            };
            match kind {
                LineKind::Removed => removed.push(i),
                LineKind::Added => added.push(i),
                LineKind::Context | LineKind::NoNewline => {
                    flush(&mut rows, &mut removed, &mut added);
                    rows.push(SplitRow {
                        left: Some(i),
                        right: Some(i),
                    });
                }
            }
            last = kind;
        }
        flush(&mut rows, &mut removed, &mut added);
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hunk(spec: &str) -> Hunk {
        let lines = spec
            .chars()
            .map(|c| DiffLine {
                kind: match c {
                    '+' => LineKind::Added,
                    '-' => LineKind::Removed,
                    '\\' => LineKind::NoNewline,
                    _ => LineKind::Context,
                },
                content: String::new(),
                old_no: None,
                new_no: None,
            })
            .collect();
        Hunk {
            lines,
            ..Default::default()
        }
    }

    fn pairs(h: &Hunk) -> Vec<(Option<usize>, Option<usize>)> {
        h.split_rows().iter().map(|r| (r.left, r.right)).collect()
    }

    #[test]
    fn context_is_on_both_sides_and_changes_pair_up() {
        // ` - - + ` → context, two removed next to one added, context.
        let h = hunk(" --+ ");
        assert_eq!(
            pairs(&h),
            vec![
                (Some(0), Some(0)),
                (Some(1), Some(3)),
                (Some(2), None),
                (Some(4), Some(4)),
            ]
        );
    }

    #[test]
    fn additions_longer_than_removals_and_interleaved_runs() {
        assert_eq!(
            pairs(&hunk("-++")),
            vec![(Some(0), Some(1)), (None, Some(2))]
        );
        // `-+-+` is one run of changes: removed [0, 2] next to added [1, 3].
        assert_eq!(
            pairs(&hunk("-+-+")),
            vec![(Some(0), Some(1)), (Some(2), Some(3))]
        );
        assert_eq!(pairs(&hunk("+")), vec![(None, Some(0))]);
        assert!(pairs(&hunk("")).is_empty());
    }

    #[test]
    fn no_newline_marker_follows_its_line() {
        // Old last line lacked a newline, the new one has it.
        assert_eq!(
            pairs(&hunk("-\\+")),
            vec![(Some(0), Some(2)), (Some(1), None)]
        );
        // Unchanged last line without newline.
        assert_eq!(
            pairs(&hunk(" \\")),
            vec![(Some(0), Some(0)), (Some(1), Some(1))]
        );
    }
}
