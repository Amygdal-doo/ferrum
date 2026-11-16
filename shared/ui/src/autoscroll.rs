//! Drag-selection autoscroll: while a selection drag holds the pointer above
//! or below a scrollable area, the area scrolls on a timer, faster the
//! further past the edge the pointer is. Views keep the timer; this module
//! only does the arithmetic, so every view scrolls the same way.

use std::time::Duration;

/// How often a view scrolls while the pointer stays past an edge.
pub const INTERVAL: Duration = Duration::from_millis(50);

/// Most lines scrolled per tick (≈ 200 lines a second).
pub const MAX_LINES: u32 = 10;

/// How far `y` is past the edges of `top..bottom`: negative above the top,
/// positive below the bottom, 0 inside.
#[must_use]
pub fn overshoot(y: f32, top: f32, bottom: f32) -> f32 {
    if y < top {
        y - top
    } else if y > bottom {
        y - bottom
    } else {
        0.0
    }
}

/// Lines to scroll this tick for an [`overshoot`] of `distance` px with lines
/// `line_height` px tall: negative = up (towards the start), positive = down,
/// 0 inside. One line just past the edge, one more per line-height further
/// out, at most [`MAX_LINES`].
#[must_use]
pub fn lines_per_tick(distance: f32, line_height: f32) -> i32 {
    if distance == 0.0 || !distance.is_finite() {
        return 0;
    }
    let line_height = if line_height > 0.0 { line_height } else { 16.0 };
    let lines = (1.0 + (distance.abs() / line_height).floor()).min(MAX_LINES as f32) as i32;
    lines * distance.signum() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overshoot_is_signed_distance_outside() {
        assert_eq!(overshoot(50.0, 10.0, 100.0), 0.0);
        assert_eq!(overshoot(10.0, 10.0, 100.0), 0.0);
        assert_eq!(overshoot(4.0, 10.0, 100.0), -6.0);
        assert_eq!(overshoot(130.0, 10.0, 100.0), 30.0);
    }

    #[test]
    fn speed_grows_with_distance_and_is_capped() {
        assert_eq!(lines_per_tick(0.0, 20.0), 0);
        assert_eq!(lines_per_tick(1.0, 20.0), 1);
        assert_eq!(lines_per_tick(19.0, 20.0), 1);
        assert_eq!(lines_per_tick(20.0, 20.0), 2);
        assert_eq!(lines_per_tick(-45.0, 20.0), -3);
        assert_eq!(lines_per_tick(5000.0, 20.0), MAX_LINES as i32);
        assert_eq!(lines_per_tick(-5000.0, 20.0), -(MAX_LINES as i32));
    }

    #[test]
    fn degenerate_inputs_stay_sane() {
        assert_eq!(lines_per_tick(f32::NAN, 20.0), 0);
        assert_eq!(lines_per_tick(10.0, 0.0), 1);
    }
}
