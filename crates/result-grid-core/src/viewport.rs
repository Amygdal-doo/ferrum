use std::ops::Range;

/// Fixed-row-height virtual viewport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub scroll_top: f64,
    pub height: f64,
    pub row_height: f64,
    pub overscan: usize,
}

impl Viewport {
    pub const DEFAULT_ROW_HEIGHT: f64 = 28.0;
    pub const DEFAULT_OVERSCAN: usize = 8;

    #[must_use]
    pub fn new(scroll_top: f64, height: f64) -> Self {
        Self {
            scroll_top,
            height,
            row_height: Self::DEFAULT_ROW_HEIGHT,
            overscan: Self::DEFAULT_OVERSCAN,
        }
    }

    /// Total scrollable height for `total_rows` rows.
    #[must_use]
    pub fn content_height(&self, total_rows: usize) -> f64 {
        total_rows as f64 * self.row_height
    }
}

/// Rows to materialise for the current scroll position. The result length is
/// bounded by `height / row_height + 2 * overscan + 1`, independent of `total_rows`.
#[must_use]
pub fn visible_range(vp: &Viewport, total_rows: usize) -> Range<usize> {
    if total_rows == 0 || vp.row_height <= 0.0 {
        return 0..0;
    }
    let first = (vp.scroll_top.max(0.0) / vp.row_height).floor() as usize;
    let visible = (vp.height.max(0.0) / vp.row_height).ceil() as usize + 1;
    let start = first.saturating_sub(vp.overscan).min(total_rows);
    let end = first
        .saturating_add(visible)
        .saturating_add(vp.overscan)
        .min(total_rows);
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_result_renders_nothing() {
        assert_eq!(visible_range(&Viewport::new(0.0, 500.0), 0), 0..0);
    }

    #[test]
    fn top_of_list() {
        let vp = Viewport {
            scroll_top: 0.0,
            height: 100.0,
            row_height: 10.0,
            overscan: 2,
        };
        assert_eq!(visible_range(&vp, 1000), 0..13);
    }

    #[test]
    fn middle_of_list_includes_overscan_both_sides() {
        let vp = Viewport {
            scroll_top: 500.0,
            height: 100.0,
            row_height: 10.0,
            overscan: 2,
        };
        assert_eq!(visible_range(&vp, 1000), 48..63);
    }

    #[test]
    fn end_is_clamped() {
        let vp = Viewport {
            scroll_top: 1e9,
            height: 100.0,
            row_height: 10.0,
            overscan: 2,
        };
        assert_eq!(visible_range(&vp, 1000), 1000..1000);
    }

    #[test]
    fn rendered_rows_are_bounded_for_100k_rows() {
        let vp = Viewport::new(1_234_567.0, 800.0);
        let r = visible_range(&vp, 100_000);
        let bound = (800.0 / vp.row_height).ceil() as usize + 1 + 2 * vp.overscan;
        assert!(r.len() <= bound, "{} > {bound}", r.len());
    }
}
