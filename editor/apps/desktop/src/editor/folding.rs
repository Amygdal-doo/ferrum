//! Code folding in the editor: fold / unfold at the caret, fold all /
//! unfold all, the gutter's fold toggles. Foldable regions come from the
//! background tree-sitter pass (`background`); the folded ones live in a
//! [`Folds`] that follows edits, and the display map hides their rows.

use ferrumeditor_core::folding::FoldRegion;
use gpui::{Context, Pixels, px};

use super::EditorView;

/// Width of the gutter's fold lane (right edge, next to the text).
pub(super) const FOLD_LANE: Pixels = px(14.);

impl EditorView {
    /// `editor:fold`, `editor:unfold`, `editor:toggle-fold`,
    /// `editor:fold-all`, `editor:unfold-all`. False for other names.
    pub(super) fn fold_command(&mut self, name: &str, cx: &mut Context<Self>) -> bool {
        let row = self.state.buffer().row_of(self.state.primary().head);
        match name {
            "editor:fold" => {
                if let Some(region) = self.region_to_fold(row) {
                    self.fold_region(region);
                }
            }
            "editor:unfold" => {
                if !self.folds.unfold_row(self.state.buffer(), row) {
                    let end = self.state.buffer().line_end(row);
                    self.folds.unfold_containing(self.state.buffer(), end + 1);
                }
            }
            "editor:toggle-fold" => self.toggle_fold_at(row),
            "editor:fold-all" => {
                let regions = self.fold_regions.clone();
                for region in regions.iter() {
                    self.fold_region(*region);
                }
            }
            "editor:unfold-all" => self.folds.clear(),
            _ => return false,
        }
        self.sync_display_map(self.display_map.wrap_width());
        self.request_autoscroll();
        cx.notify();
        true
    }

    /// The innermost foldable region around `row` that is not folded yet.
    fn region_to_fold(&self, row: usize) -> Option<FoldRegion> {
        let buffer = self.state.buffer();
        self.fold_regions
            .iter()
            .filter(|r| r.header <= row && row <= r.last + 1)
            .filter(|r| !self.folds.is_folded(buffer, r.header))
            .max_by_key(|r| r.header)
            .copied()
    }

    /// Folds `region`, moving carets out of it to the end of its header.
    fn fold_region(&mut self, region: FoldRegion) {
        let buffer = self.state.buffer();
        let start = buffer.line_end(region.header);
        let end = buffer.line_end(region.last.min(buffer.line_count().saturating_sub(1)));
        let head = self.state.primary().head;
        if start < head && head <= end {
            self.state.set_cursor(start);
        }
        self.folds.fold(self.state.buffer(), region);
    }

    /// Gutter click / `editor:toggle-fold` on `row`.
    pub(super) fn toggle_fold_at(&mut self, row: usize) {
        if self.folds.unfold_row(self.state.buffer(), row) {
            return;
        }
        if let Some(region) = self.fold_regions.iter().find(|r| r.header == row).copied() {
            self.fold_region(region);
        }
    }

    /// Whether `row` starts a foldable region (a gutter toggle is drawn).
    pub(super) fn is_fold_header(&self, row: usize) -> bool {
        self.fold_regions
            .binary_search_by_key(&row, |r| r.header)
            .is_ok()
    }

    /// Brings the display map up to date with the folds (unfolding any
    /// fold a caret moved into) and the soft wrap width; prepaint calls it.
    pub(super) fn sync_display_map(&mut self, wrap: Option<usize>) {
        let buffer = self.state.buffer();
        self.folds.sync(buffer);
        for selection in self.state.selections() {
            self.folds.unfold_containing(buffer, selection.head);
        }
        let hidden = self.folds.hidden_rows(buffer);
        let tab_size = self.state.settings.tab_size;
        self.display_map
            .sync_with_folds(buffer, tab_size, wrap, &hidden);
    }
}

#[cfg(test)]
impl EditorView {
    /// Buffer rows hidden by folds.
    pub fn hidden_rows(&self) -> Vec<std::ops::Range<usize>> {
        self.folds.hidden_rows(self.state.buffer())
    }

    pub fn fold_regions_ready(&self) -> bool {
        !self.fold_regions.is_empty()
    }
}
