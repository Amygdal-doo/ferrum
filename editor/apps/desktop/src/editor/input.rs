//! Mouse, scroll wheel and platform text input (typing, IME composition,
//! dead keys) for the editor.

use std::ops::Range;

use ferrumeditor_core::history::EditKind;
use ferrumeditor_core::selection::Selection;
use ferrumeditor_core::text::word_range_at;
use gpui::{
    Bounds, Context, EntityInputHandler, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, ScrollEvent, UTF16Selection, Window, point, px, size,
};

use super::{DragMode, DragState, EditorEvent, EditorView};

/// Width of the gutter's breakpoint lane (left edge).
pub(super) const BREAKPOINT_LANE: Pixels = px(16.);

impl EditorView {
    pub(super) fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus(window, cx);
        self.hide_hover(cx);
        self.completion = None;
        let Some(layout) = self.last_layout.clone() else {
            return;
        };
        let offset = layout.offset_at(event.position, &self.state, &self.display_map);
        let in_gutter = event.position.x < layout.bounds.left() + layout.gutter_width;
        // The gutter's left lane toggles breakpoints.
        if in_gutter
            && event.position.x < layout.bounds.left() + BREAKPOINT_LANE
            && self.path.is_some()
        {
            let row = self.state.buffer().row_of(offset);
            cx.emit(EditorEvent::ToggleBreakpoint(row));
            return;
        }
        self.marked_range = None;
        let m = event.modifiers;
        if m.platform && !m.alt && !m.shift && !in_gutter && self.has_language_server() {
            // Cmd+click: go to definition (Cmd+Alt+click adds a cursor).
            self.state.click(offset);
            self.drag = None;
            self.go_to_definition(offset, cx);
        } else if m.platform && m.alt && !in_gutter {
            self.state.toggle_cursor_at(offset);
            self.drag = None;
        } else if event.modifiers.alt && !in_gutter {
            let origin = layout.display_point_at(event.position);
            self.state.column_select(origin, origin, &self.display_map);
            self.drag = Some(DragState {
                mode: DragMode::Column,
                origin: offset..offset,
                column_origin: Some(origin),
            });
        } else if in_gutter || event.click_count >= 3 {
            let row = self.state.buffer().row_of(offset);
            self.state.select_row(row);
            let range = self.state.primary().range();
            self.drag = Some(DragState {
                mode: DragMode::Line,
                origin: range,
                column_origin: None,
            });
        } else if event.click_count == 2 {
            self.state.select_word_at(offset);
            let range = self.state.primary().range();
            self.drag = Some(DragState {
                mode: DragMode::Word,
                origin: range,
                column_origin: None,
            });
        } else if event.modifiers.platform {
            self.state.toggle_cursor_at(offset);
            self.drag = None;
        } else if event.modifiers.shift {
            self.state.extend_primary_to(offset);
            let anchor = self.state.primary().anchor;
            self.drag = Some(DragState {
                mode: DragMode::Char,
                origin: anchor..anchor,
                column_origin: None,
            });
        } else {
            self.state.click(offset);
            self.drag = Some(DragState {
                mode: DragMode::Char,
                origin: offset..offset,
                column_origin: None,
            });
        }
        cx.emit(EditorEvent::SelectionsChanged);
        cx.notify();
    }

    pub(super) fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.drag.is_none() || !event.dragging() {
            if event.pressed_button.is_none() {
                let offset = self.text_offset_under(event.position);
                self.hover_moved(offset, cx);
            }
            return;
        }
        self.drag_pointer = Some(event.position);
        self.drag_select(event.position, cx);
        self.maybe_start_drag_scroll(cx);
    }

    /// Lines to scroll while the dragging pointer is above (negative) or
    /// below the text, from the shared autoscroll speed.
    fn drag_scroll_lines(&self) -> i32 {
        let (Some(_), Some(pointer), Some(layout)) =
            (&self.drag, self.drag_pointer, self.last_layout.as_ref())
        else {
            return 0;
        };
        ferrum_ui::autoscroll::lines_per_tick(
            ferrum_ui::autoscroll::overshoot(
                f32::from(pointer.y),
                f32::from(layout.bounds.top()),
                f32::from(layout.bounds.bottom()),
            ),
            f32::from(layout.line_height),
        )
    }

    /// Starts the edge autoscroll timer when the drag left the text
    /// vertically; it stops when the pointer is back, the button is up, or
    /// the scroll position stops moving (top / bottom reached).
    fn maybe_start_drag_scroll(&mut self, cx: &mut Context<Self>) {
        if self.drag_scroll.is_some() || self.drag_scroll_lines() == 0 {
            return;
        }
        self.drag_scroll = Some(cx.spawn(async move |this, cx| {
            let mut last = None;
            loop {
                cx.background_executor()
                    .timer(ferrum_ui::autoscroll::INTERVAL)
                    .await;
                match this.update(cx, |view, cx| view.drag_scroll_tick(last, cx)) {
                    Ok(Some(at)) => last = Some(at),
                    _ => break,
                }
            }
        }));
    }

    /// One timer step: scrolls and extends the selection. `last`: where the
    /// previous step left the (clamped) scroll position; `None` ends the timer.
    fn drag_scroll_tick(&mut self, last: Option<Pixels>, cx: &mut Context<Self>) -> Option<Pixels> {
        let lines = self.drag_scroll_lines();
        let (Some(pointer), Some(layout)) = (self.drag_pointer, self.last_layout.clone()) else {
            self.drag_scroll = None;
            return None;
        };
        // Prepaint clamps `scroll_y`: unchanged since the last step = at an end.
        if lines == 0 || last == Some(layout.scroll_y) {
            self.drag_scroll = None;
            return None;
        }
        self.scroll_y = layout.scroll_y + layout.line_height * lines as f32;
        self.drag_select(pointer, cx);
        Some(layout.scroll_y)
    }

    /// Extends the drag's selection to `position` (clamped into the text).
    fn drag_select(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let (Some(drag), Some(layout)) = (self.drag.clone(), self.last_layout.clone()) else {
            return;
        };
        let offset = layout.offset_at(position, &self.state, &self.display_map);
        let buffer = self.state.buffer();
        let selection = match drag.mode {
            DragMode::Column => {
                if let Some(origin) = drag.column_origin {
                    let head = layout.display_point_at(position);
                    self.state.column_select(origin, head, &self.display_map);
                }
                cx.notify();
                return;
            }
            DragMode::Char => Selection::new(drag.origin.start, offset),
            DragMode::Word => {
                let word = word_range_at(buffer, offset);
                extend_unit(&drag.origin, word)
            }
            DragMode::Line => {
                let row = buffer.row_of(offset);
                let start = buffer.line_start(row);
                let end = if row + 1 < buffer.line_count() {
                    buffer.line_start(row + 1)
                } else {
                    buffer.len()
                };
                extend_unit(&drag.origin, start..end)
            }
        };
        let mut selections = self.state.selections().to_vec();
        let primary = self.state.primary_index();
        selections[primary] = selection;
        let last = selections.len() - 1;
        selections.swap(primary, last);
        self.state.set_selections(selections);
        cx.emit(EditorEvent::SelectionsChanged);
        cx.notify();
    }

    pub(super) fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.drag = None;
        self.drag_pointer = None;
        self.drag_scroll = None;
    }

    pub(super) fn on_scroll(
        &mut self,
        event: &ScrollEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let line_height = self.last_layout.as_ref().map_or(px(20.), |l| l.line_height);
        let delta = event.delta.pixel_delta(line_height);
        if event.modifiers.shift && delta.x == px(0.) {
            self.scroll_x -= delta.y;
        } else {
            self.scroll_y -= delta.y;
            self.scroll_x -= delta.x;
        }
        self.hide_hover(cx);
        self.completion = None;
        // Clamping happens in prepaint, which knows the viewport.
        cx.stop_propagation();
        cx.notify();
    }

    /// Buffer offset of the character under `position`, if it is over text.
    fn text_offset_under(&self, position: Point<Pixels>) -> Option<usize> {
        let layout = self.last_layout.as_ref()?;
        if position.x < layout.text_left || !layout.bounds.contains(&position) {
            return None;
        }
        let row = layout.display_row_at(position.y);
        let row = layout.rows.iter().find(|r| r.display_row as isize == row)?;
        let x = position.x - layout.text_left + layout.scroll_x;
        if x > row.shaped.width + layout.char_width / 2. {
            return None;
        }
        // The character the mouse is on, not the caret slot nearest to it.
        let offset = row.offset_for_x(x - layout.char_width / 2.);
        let end = row.line_start + row.segment.end;
        (offset < end).then_some(offset)
    }

    fn utf16_to_offset(&self, utf16: usize) -> usize {
        let rope = self.state.buffer().rope();
        let utf16 = utf16.min(rope.len_utf16_cu());
        rope.char_to_byte(rope.utf16_cu_to_char(utf16))
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let rope = self.state.buffer().rope();
        rope.char_to_utf16_cu(rope.byte_to_char(offset.min(rope.len_bytes())))
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.utf16_to_offset(range.start)..self.utf16_to_offset(range.end)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }
}

/// Selection spanning the unit clicked first and the unit under the mouse.
fn extend_unit(origin: &Range<usize>, unit: Range<usize>) -> Selection {
    if unit.start < origin.start {
        Selection::new(origin.end, unit.start)
    } else {
        Selection::new(origin.start, unit.end.max(origin.end))
    }
}

impl EntityInputHandler for EditorView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual.replace(self.range_to_utf16(&range));
        Some(self.state.buffer().slice(range))
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let primary = self.state.primary();
        Some(UTF16Selection {
            range: self.range_to_utf16(&primary.range()),
            reversed: primary.reversed(),
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked_range = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let explicit = range_utf16
            .map(|r| self.range_from_utf16(&r))
            .or(self.marked_range.take());
        let typed = explicit.is_none();
        match explicit {
            // IME commit or a platform replacement: one range, one caret.
            Some(range) => self
                .state
                .replace_and_select(range, text, None, EditKind::Insert),
            // Plain typing: every cursor, with auto-pairs and auto-indent.
            None => self.state.insert_text(text),
        }
        self.marked_range = None;
        self.request_autoscroll();
        self.after_edit(cx);
        if typed {
            self.after_typing(text, cx);
        } else {
            self.completion = None;
        }
        cx.emit(EditorEvent::SelectionsChanged);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .map(|r| self.range_from_utf16(&r))
            .or(self.marked_range.clone())
            .unwrap_or_else(|| self.state.primary().range());
        // The selection inside the composition arrives in UTF-16 units of `new_text`.
        let select = new_selected_range_utf16.map(|r| {
            let byte = |units: usize| {
                let mut count = 0;
                new_text
                    .char_indices()
                    .find(|(_, c)| {
                        let found = count >= units;
                        count += c.len_utf16();
                        found
                    })
                    .map_or(new_text.len(), |(i, _)| i)
            };
            byte(r.start)..byte(r.end)
        });
        self.state
            .replace_and_select(range.clone(), new_text, select, EditKind::Insert);
        self.marked_range =
            (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
        self.request_autoscroll();
        self.after_edit(cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.last_layout.as_ref()?;
        let start = self.utf16_to_offset(range_utf16.start);
        let origin: Point<Pixels> = layout.point_for_offset(start)?;
        Some(Bounds::new(
            origin,
            size(layout.char_width, layout.line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let layout = self.last_layout.as_ref()?;
        let offset = layout.offset_at(
            point(position.x, position.y),
            &self.state,
            &self.display_map,
        );
        Some(self.offset_to_utf16(offset))
    }
}
