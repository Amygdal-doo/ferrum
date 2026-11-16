//! Selection drags: extend the selection to the pointer, and while the
//! pointer is above or below the pane, scroll its scrollback on a timer
//! (speed from `ferrum_ui::autoscroll`), so a drag can select more than a
//! screen. Stops when the pointer comes back, the button is released, or
//! the scrollback's top / bottom is reached.

use super::TermView;
use crate::selection::line_of;
use ferrum_ui::autoscroll;
use ferrum_ui::gpui::Context;
use ferrumterm_protocol::{BlockId, Request};

impl TermView {
    /// Moves the selection head to the pointer (clamped to the pane, so past
    /// an edge it is the first / last visible line).
    pub(super) fn drag_to(&mut self, x: f32, y: f32) -> bool {
        let Some(block) = self.selection.as_ref().map(|s| s.block) else {
            return false;
        };
        if let Some((_, head)) = self.cell_under(x, y, Some(block))
            && let Some(selection) = self.selection.as_mut()
            && selection.head != head
        {
            selection.head = head;
            return true;
        }
        false
    }

    /// Lines to scroll the dragged pane by now: positive = into history
    /// (pointer above the pane), 0 = don't.
    fn drag_scroll_lines(&self) -> Option<(BlockId, i32)> {
        if !self.selecting {
            return None;
        }
        let (_, y) = self.drag_pointer?;
        let block = self.selection.as_ref()?.block;
        let &(_, _, top, _, height) = self.app.panes.iter().find(|p| p.0 == block)?;
        let frame = self.app.frames.get(&block)?;
        // Full-screen programs have no scrollback to move through.
        if frame.alt_screen {
            return None;
        }
        let cell_height = self.metrics.map_or(16.0, |m| f32::from(m.cell_height));
        let lines = scroll_step(
            y,
            (top, top + height),
            cell_height,
            frame.display_offset,
            frame.history,
        );
        (lines != 0).then_some((block, lines))
    }

    /// After a pointer move: starts the timer when the pointer left the pane
    /// mid-drag (it stops itself, see [`Self::drag_scroll_tick`]).
    pub(super) fn maybe_start_drag_scroll(&mut self, cx: &mut Context<Self>) {
        if self.drag_scrolling || self.drag_scroll_lines().is_none() {
            return;
        }
        self.drag_scrolling = true;
        self._drag_scroll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(autoscroll::INTERVAL).await;
                let more = this
                    .update(cx, |view, cx| view.drag_scroll_tick(cx))
                    .unwrap_or(false);
                if !more {
                    break;
                }
            }
        }));
    }

    /// One timer step; `false` ends the timer.
    fn drag_scroll_tick(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((block, lines)) = self.drag_scroll_lines() else {
            self.drag_scrolling = false;
            return false;
        };
        self.app.send(Request::Scroll {
            target: Some(block.to_string()),
            lines: Some(lines),
        });
        // The scrolled frame arrives shortly; `on_drag_frame` moves the head
        // onto its new edge line.
        cx.notify();
        true
    }

    /// A frame of `block` arrived: remember its lines for copying and keep
    /// the head under the pointer as the pane scrolls.
    pub(super) fn on_drag_frame(&mut self, block: BlockId) {
        if !self.selecting || self.selection.as_ref().map(|s| s.block) != Some(block) {
            return;
        }
        if let Some(frame) = self.app.frames.get(&block) {
            self.seen.record(frame);
        }
        if let Some((x, y)) = self.drag_pointer {
            self.drag_to(x, y);
        }
    }

    /// Converts a screen row of `block` to a line (see `selection`).
    pub(super) fn line_at(&self, block: BlockId, row: i64) -> i64 {
        match (self.app.frames.get(&block), u16::try_from(row)) {
            (Some(frame), Ok(row)) => line_of(frame, row),
            _ => row,
        }
    }
}

/// Lines to scroll a pane spanning `top..bottom` px for a pointer at `y`:
/// positive = into history (pointer above), negative = towards the live
/// screen, limited by the scrollback left (`offset` of `history` lines).
fn scroll_step(
    y: f32,
    (top, bottom): (f32, f32),
    cell_height: f32,
    offset: u32,
    history: u32,
) -> i32 {
    let lines = -autoscroll::lines_per_tick(autoscroll::overshoot(y, top, bottom), cell_height);
    let room = if lines > 0 {
        history.saturating_sub(offset)
    } else {
        offset
    };
    lines.signum() * lines.abs().min(i32::try_from(room).unwrap_or(i32::MAX))
}

#[cfg(test)]
mod tests {
    use super::scroll_step;

    #[test]
    fn scrolls_towards_the_edge_the_pointer_left_by() {
        let pane = (100.0, 500.0);
        // Inside: nothing.
        assert_eq!(scroll_step(300.0, pane, 20.0, 0, 1000), 0);
        // Above: into history, faster further out.
        assert_eq!(scroll_step(95.0, pane, 20.0, 0, 1000), 1);
        assert_eq!(scroll_step(50.0, pane, 20.0, 0, 1000), 3);
        // Below: back towards the live screen.
        assert_eq!(scroll_step(545.0, pane, 20.0, 10, 1000), -3);
    }

    #[test]
    fn stops_at_the_ends_of_the_scrollback() {
        let pane = (100.0, 500.0);
        assert_eq!(scroll_step(0.0, pane, 20.0, 1000, 1000), 0, "at the top");
        assert_eq!(scroll_step(0.0, pane, 20.0, 998, 1000), 2, "two lines left");
        assert_eq!(scroll_step(900.0, pane, 20.0, 0, 1000), 0, "at the bottom");
        assert_eq!(scroll_step(900.0, pane, 20.0, 1, 1000), -1);
    }
}
