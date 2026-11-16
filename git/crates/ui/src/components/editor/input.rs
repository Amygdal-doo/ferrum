//! Mouse, scroll wheel and platform text input (typing, IME composition,
//! dead keys) for the editor.

use std::ops::Range;

use ferrumeditor_core::history::EditKind;
use ferrumeditor_core::selection::Selection;
use ferrumeditor_core::text::word_range_at;
use gpui::{
    Bounds, Context, EntityInputHandler, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, ScrollEvent, UTF16Selection, Window, px, size,
};

use super::{CodeEditor, DragMode, DragState};

impl CodeEditor {
    pub(super) fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus(window, cx);
        let Some(layout) = self.last_layout.clone() else {
            return;
        };
        let tab_size = self.state.settings.tab_size;
        let offset = layout.offset_at(event.position, self.state.buffer(), tab_size);
        let in_gutter = event.position.x < layout.bounds.left() + layout.gutter_width;
        self.marked_range = None;
        let m = event.modifiers;
        let (mode, origin) = if (m.platform || m.alt) && !in_gutter {
            // Cmd+click (Alt+click) adds or removes a cursor.
            self.state.toggle_cursor_at(offset);
            self.drag = None;
            cx.notify();
            return;
        } else if in_gutter || event.click_count >= 3 {
            self.state.select_row(self.state.buffer().row_of(offset));
            (DragMode::Line, self.state.primary().range())
        } else if event.click_count == 2 {
            self.state.select_word_at(offset);
            (DragMode::Word, self.state.primary().range())
        } else if m.shift {
            self.state.extend_primary_to(offset);
            let anchor = self.state.primary().anchor;
            (DragMode::Char, anchor..anchor)
        } else {
            self.state.click(offset);
            (DragMode::Char, offset..offset)
        };
        self.drag = Some(DragState { mode, origin });
        cx.notify();
    }

    pub(super) fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.dragging() {
            self.drag_select(event.position, cx);
        }
    }

    /// Extends the drag's selection to `position`.
    fn drag_select(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let (Some(drag), Some(layout)) = (self.drag.clone(), self.last_layout.as_ref()) else {
            return;
        };
        let buffer = self.state.buffer();
        let offset = layout.offset_at(position, buffer, self.state.settings.tab_size);
        let selection = match drag.mode {
            DragMode::Char => Selection::new(drag.origin.start, offset),
            DragMode::Word => extend_unit(&drag.origin, word_range_at(buffer, offset)),
            DragMode::Line => {
                let row = buffer.row_of(offset);
                let end = if row + 1 < buffer.line_count() {
                    buffer.line_start(row + 1)
                } else {
                    buffer.len()
                };
                extend_unit(&drag.origin, buffer.line_start(row)..end)
            }
        };
        let mut selections = self.state.selections().to_vec();
        let primary = self.state.primary_index();
        selections[primary] = selection;
        let last = selections.len() - 1;
        selections.swap(primary, last);
        self.state.set_selections(selections);
        self.autoscroll = true;
        cx.notify();
    }

    pub(super) fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.drag = None;
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
        // Clamping happens in prepaint, which knows the viewport.
        cx.stop_propagation();
        cx.notify();
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

impl EntityInputHandler for CodeEditor {
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
        match explicit {
            // IME commit or a platform replacement: one range, one caret.
            Some(range) => self
                .state
                .replace_and_select(range, text, None, EditKind::Insert),
            // Plain typing: every cursor, with auto-pairs and auto-indent.
            None => self.state.insert_text(text),
        }
        self.marked_range = None;
        self.autoscroll = true;
        self.after_edit(cx);
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
        self.autoscroll = true;
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
        let origin = layout.point_for_offset(start)?;
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
        let offset = layout.offset_at(position, self.state.buffer(), self.state.settings.tab_size);
        Some(self.offset_to_utf16(offset))
    }
}
