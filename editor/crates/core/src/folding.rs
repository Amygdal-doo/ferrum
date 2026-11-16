//! Code folding: the regions that can fold, found in the tree-sitter syntax
//! tree, and the regions folded in a buffer, kept in step with its edits.
//!
//! A region hangs from its first row (which stays visible) and hides the
//! rows after it. When the region ends with a closer that starts its own
//! row (`}`, `]`, `)`, an HTML end tag) that row stays visible too, so a
//! folded function reads `fn main() { ⋯` followed by `}`.

use std::ops::Range;

use crate::buffer::Buffer;
use crate::highlight;
use crate::language::Language;

/// A foldable region: `header` stays visible, `first..=last` hide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FoldRegion {
    pub header: usize,
    pub last: usize,
}

/// Nodes without bracket delimiters that still fold as a whole: indented
/// blocks (Python), YAML mappings, Markdown sections, TOML tables, HTML
/// elements, fenced code.
const BLOCK_KINDS: &[&str] = &[
    "block",
    "block_mapping_pair",
    "block_sequence_item",
    "section",
    "table",
    "table_array_element",
    "element",
    "script_element",
    "style_element",
    "fenced_code_block",
];

/// Foldable regions of `source`, one per header row (the outermost), sorted
/// by header row. Empty for plain text or when parsing fails.
pub fn fold_regions(language: Language, source: &str) -> Vec<FoldRegion> {
    let Some(grammar) = highlight::grammar(language) else {
        return Vec::new();
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&grammar).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let lines: Vec<&str> = source.split('\n').collect();
    let mut by_header: std::collections::BTreeMap<usize, usize> = Default::default();
    let mut cursor = tree.walk();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if let Some(region) = region_of(node, &lines)
            && region.last > region.header
        {
            let last = by_header.entry(region.header).or_insert(region.last);
            *last = (*last).max(region.last);
        }
        stack.extend(node.children(&mut cursor));
    }
    by_header
        .into_iter()
        .map(|(header, last)| FoldRegion { header, last })
        .collect()
}

/// Whether `row` starts (after indentation) with the text of `node`.
fn starts_row(node: tree_sitter::Node, lines: &[&str]) -> bool {
    let start = node.start_position();
    lines
        .get(start.row)
        .and_then(|line| line.get(..start.column))
        .is_some_and(|before| before.trim().is_empty())
}

fn region_of(node: tree_sitter::Node, lines: &[&str]) -> Option<FoldRegion> {
    let start = node.start_position().row;
    let mut end = node.end_position().row;
    // A node ending at column 0 really ends on the row before.
    if end > start && node.end_position().column == 0 {
        end -= 1;
    }
    // One-row nodes only fold as an indented block under their statement.
    if end <= start && node.kind() != "block" {
        return None;
    }
    let count = node.child_count();
    let first = node.child(0);
    let last = count.checked_sub(1).and_then(|i| node.child(i));
    let opens = first.is_some_and(|c| !c.is_named() && matches!(c.kind(), "{" | "[" | "("));
    let closer = last.filter(|c| {
        (!c.is_named() && matches!(c.kind(), "}" | "]" | ")")) || c.kind() == "end_tag"
    });
    if opens || closer.is_some_and(|c| c.kind() == "end_tag") {
        let header = if opens {
            first.map_or(start, |c| c.start_position().row)
        } else {
            start
        };
        let last_row = match closer {
            Some(closer) if starts_row(closer, lines) => closer.start_position().row - 1,
            _ => end,
        };
        return Some(FoldRegion {
            header,
            last: last_row,
        });
    }
    if node.kind().contains("comment") {
        return Some(FoldRegion {
            header: start,
            last: end,
        });
    }
    if BLOCK_KINDS.contains(&node.kind()) {
        // An indented block (Python) hangs from its parent's first row.
        let header = node
            .parent()
            .map(|p| p.start_position().row)
            .filter(|row| *row < start && node.kind() == "block")
            .unwrap_or(start);
        return Some(FoldRegion { header, last: end });
    }
    None
}

/// The folded regions of one buffer, as byte ranges from the newline that
/// ends the header row to the end of the last hidden row. Edits move them;
/// an edit inside (or across) a folded region unfolds it.
#[derive(Debug, Clone, Default)]
pub struct Folds {
    ranges: Vec<Range<usize>>,
    version: u64,
}

impl Folds {
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// Follows the buffer's edits since the last call. Returns whether any
    /// fold was dropped.
    pub fn sync(&mut self, buffer: &Buffer) -> bool {
        if self.version == buffer.version() {
            return false;
        }
        let before = self.ranges.len();
        match buffer.edits_since(self.version) {
            Some(edits) => {
                for (edit, new_len) in edits {
                    self.ranges.retain_mut(|fold| {
                        if edit.end <= fold.start {
                            let shift = |o: usize| o + new_len - edit.len();
                            *fold = shift(fold.start)..shift(fold.end);
                            true
                        } else {
                            edit.start >= fold.end
                        }
                    });
                }
            }
            None => self.ranges.clear(),
        }
        self.version = buffer.version();
        self.ranges.len() != before
    }

    /// Folds `region` (rows of the current text).
    pub fn fold(&mut self, buffer: &Buffer, region: FoldRegion) {
        self.sync(buffer);
        if region.last <= region.header || region.last >= buffer.line_count() {
            return;
        }
        let range = buffer.line_end(region.header)..buffer.line_end(region.last);
        if !self.ranges.contains(&range) {
            self.ranges.push(range);
            self.ranges
                .sort_by_key(|r| (r.start, std::cmp::Reverse(r.end)));
        }
    }

    /// Unfolds the region hanging from `row`. Returns whether there was one.
    pub fn unfold_row(&mut self, buffer: &Buffer, row: usize) -> bool {
        self.sync(buffer);
        let before = self.ranges.len();
        self.ranges.retain(|r| buffer.row_of(r.start) != row);
        self.ranges.len() != before
    }

    /// Unfolds every region hiding `offset` (a caret that moved into it).
    pub fn unfold_containing(&mut self, buffer: &Buffer, offset: usize) -> bool {
        self.sync(buffer);
        let before = self.ranges.len();
        self.ranges
            .retain(|r| !(r.start < offset && offset <= r.end));
        self.ranges.len() != before
    }

    pub fn clear(&mut self) {
        self.ranges.clear();
    }

    /// Whether the region hanging from `row` is folded.
    pub fn is_folded(&self, buffer: &Buffer, row: usize) -> bool {
        self.ranges.iter().any(|r| buffer.row_of(r.start) == row)
    }

    /// Hidden buffer rows, as sorted, disjoint row ranges (for the
    /// [`DisplayMap`](crate::display::DisplayMap)).
    pub fn hidden_rows(&self, buffer: &Buffer) -> Vec<Range<usize>> {
        let mut out: Vec<Range<usize>> = Vec::new();
        for range in &self.ranges {
            let rows = buffer.row_of(range.start) + 1..buffer.row_of(range.end) + 1;
            match out.last_mut() {
                Some(last) if rows.start <= last.end => last.end = last.end.max(rows.end),
                _ => out.push(rows),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regions(language: Language, source: &str) -> Vec<(usize, usize)> {
        fold_regions(language, source)
            .into_iter()
            .map(|r| (r.header, r.last))
            .collect()
    }

    #[test]
    fn rust_blocks_keep_the_closing_brace_visible() {
        let source =
            "fn main() {\n    let v = [\n        1,\n    ];\n}\n\n/// a\n/// b\nfn x() {}\n";
        let found = regions(Language::Rust, source);
        assert!(found.contains(&(0, 3)), "{found:?}");
        assert!(found.contains(&(1, 2)), "{found:?}");
        assert!(!found.iter().any(|(h, _)| *h == 8), "{found:?}");
    }

    #[test]
    fn python_blocks_hang_from_their_statement() {
        let source = "def f(x):\n    if x:\n        return 1\n    return 2\n\ny = 3\n";
        let found = regions(Language::Python, source);
        assert!(found.contains(&(0, 3)), "{found:?}");
        assert!(found.contains(&(1, 2)), "{found:?}");
    }

    #[test]
    fn json_and_markdown_regions() {
        let json = regions(Language::Json, "{\n  \"a\": [\n    1\n  ]\n}\n");
        assert_eq!(json, vec![(0, 3), (1, 2)]);
        let md = regions(Language::Markdown, "# A\ntext\n\n## B\nmore\n");
        assert!(md.contains(&(0, 4)), "{md:?}");
        assert!(regions(Language::PlainText, "a\nb").is_empty());
    }

    #[test]
    fn folds_follow_edits_and_unfold_when_touched() {
        let mut buffer = Buffer::new("a {\n  b\n  c\n}\nd {\n  e\n}\n");
        let mut folds = Folds::default();
        folds.fold(&buffer, FoldRegion { header: 0, last: 2 });
        folds.fold(&buffer, FoldRegion { header: 4, last: 5 });
        assert_eq!(folds.hidden_rows(&buffer), vec![1..3, 5..6]);
        assert!(folds.is_folded(&buffer, 0) && !folds.is_folded(&buffer, 1));
        // A line inserted above moves both folds down.
        buffer.edit(0..0, "top\n");
        folds.sync(&buffer);
        assert_eq!(folds.hidden_rows(&buffer), vec![2..4, 6..7]);
        // Typing at the end of a header keeps its fold.
        let end = buffer.line_end(1);
        buffer.edit(end..end, " ");
        assert!(!folds.sync(&buffer));
        // An edit inside the second fold drops it.
        let inside = buffer.line_start(6) + 1;
        buffer.edit(inside..inside, "x");
        assert!(folds.sync(&buffer));
        assert_eq!(folds.hidden_rows(&buffer), vec![2..4]);
        assert!(folds.unfold_containing(&buffer, buffer.line_start(3)));
        assert!(folds.is_empty());
    }

    #[test]
    fn nested_folds_merge_and_replaced_text_unfolds() {
        let mut buffer = Buffer::new("a {\n  b {\n    c\n  }\n}\n");
        let mut folds = Folds::default();
        folds.fold(&buffer, FoldRegion { header: 1, last: 2 });
        folds.fold(&buffer, FoldRegion { header: 0, last: 3 });
        assert_eq!(folds.hidden_rows(&buffer), vec![1..4]);
        assert!(folds.unfold_row(&buffer, 0));
        assert_eq!(folds.hidden_rows(&buffer), vec![2..3]);
        buffer.set_text("other");
        folds.sync(&buffer);
        assert!(folds.is_empty());
    }
}
