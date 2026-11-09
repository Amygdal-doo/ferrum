use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, HighlightStyle,
    KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad,
    Pixels, Point, ShapedLine, SharedString, Style, TextRun, UTF16Selection, UnderlineStyle,
    Window, actions, fill, point, prelude::*, px, relative, size,
};
use theme::ActiveTheme;
use ui::{StyledTypography, h_flex};
use unicode_segmentation::UnicodeSegmentation;

actions!(
    text_input,
    [
        Backspace,
        Delete,
        DeleteWordLeft,
        Left,
        Right,
        Up,
        Down,
        WordLeft,
        WordRight,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectWordLeft,
        SelectWordRight,
        SelectAll,
        Home,
        End,
        SelectHome,
        SelectEnd,
        Enter,
        Submit,
        Tab,
        Paste,
        Cut,
        Copy,
        ShowCharacterPalette,
    ]
);

const KEY_CONTEXT: &str = "FerrumTextInput";
const MASK_CHAR: char = '•';

pub(crate) fn bind_keys(cx: &mut App) {
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, context),
        KeyBinding::new("delete", Delete, context),
        KeyBinding::new("alt-backspace", DeleteWordLeft, context),
        KeyBinding::new("ctrl-backspace", DeleteWordLeft, context),
        KeyBinding::new("left", Left, context),
        KeyBinding::new("right", Right, context),
        KeyBinding::new("up", Up, context),
        KeyBinding::new("down", Down, context),
        KeyBinding::new("alt-left", WordLeft, context),
        KeyBinding::new("alt-right", WordRight, context),
        KeyBinding::new("ctrl-left", WordLeft, context),
        KeyBinding::new("ctrl-right", WordRight, context),
        KeyBinding::new("shift-left", SelectLeft, context),
        KeyBinding::new("shift-right", SelectRight, context),
        KeyBinding::new("shift-up", SelectUp, context),
        KeyBinding::new("shift-down", SelectDown, context),
        KeyBinding::new("alt-shift-left", SelectWordLeft, context),
        KeyBinding::new("alt-shift-right", SelectWordRight, context),
        KeyBinding::new("cmd-a", SelectAll, context),
        KeyBinding::new("ctrl-a", SelectAll, context),
        KeyBinding::new("home", Home, context),
        KeyBinding::new("end", End, context),
        KeyBinding::new("cmd-left", Home, context),
        KeyBinding::new("cmd-right", End, context),
        KeyBinding::new("shift-home", SelectHome, context),
        KeyBinding::new("shift-end", SelectEnd, context),
        KeyBinding::new("cmd-shift-left", SelectHome, context),
        KeyBinding::new("cmd-shift-right", SelectEnd, context),
        KeyBinding::new("enter", Enter, context),
        KeyBinding::new("cmd-enter", Submit, context),
        KeyBinding::new("ctrl-enter", Submit, context),
        KeyBinding::new("tab", Tab, Some("FerrumTextInput && multiline")),
        KeyBinding::new("cmd-v", Paste, context),
        KeyBinding::new("ctrl-v", Paste, context),
        KeyBinding::new("cmd-c", Copy, context),
        KeyBinding::new("ctrl-c", Copy, context),
        KeyBinding::new("cmd-x", Cut, context),
        KeyBinding::new("ctrl-x", Cut, context),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, context),
    ]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputEvent {
    /// The text changed through user input.
    Changed,
    /// Enter in a single-line input, or cmd/ctrl-enter in any input.
    Submit,
    Blurred,
}

/// An editable text field: single-line (default) or multi-line, optionally
/// masked for secrets or rendered in the monospace buffer font.
pub struct TextInput {
    focus_handle: FocusHandle,
    content: String,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    multiline: bool,
    masked: bool,
    monospace: bool,
    read_only: bool,
    bordered: bool,
    min_lines: usize,
    scroll_x: Pixels,
    /// Styled byte ranges (sorted, non-overlapping), e.g. syntax colours.
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    last_layout: Option<TextLayout>,
    is_selecting: bool,
    _blur: Option<gpui::Subscription>,
}

#[derive(Clone)]
struct TextLayout {
    lines: Vec<LayoutLine>,
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    scroll_x: Pixels,
}

#[derive(Clone)]
struct LayoutLine {
    shaped: ShapedLine,
    content_range: Range<usize>,
}

impl EventEmitter<TextInputEvent> for TextInput {}

impl TextInput {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let blur = cx.on_blur(&focus_handle, window, |_, _, cx| {
            cx.emit(TextInputEvent::Blurred);
        });
        Self {
            focus_handle,
            content: String::new(),
            placeholder: SharedString::default(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            multiline: false,
            masked: false,
            monospace: false,
            read_only: false,
            bordered: true,
            min_lines: 1,
            scroll_x: px(0.),
            highlights: Vec::new(),
            last_layout: None,
            is_selecting: false,
            _blur: Some(blur),
        }
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn multiline(mut self, min_lines: usize) -> Self {
        self.multiline = true;
        self.min_lines = min_lines.max(1);
        self
    }

    pub fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }

    pub fn monospace(mut self) -> Self {
        self.monospace = true;
        self
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub fn borderless(mut self) -> Self {
        self.bordered = false;
        self
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.content = self.sanitize(text.into());
        let end = self.content.len();
        self.selected_range = end..end;
        self
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Byte range of the current selection (empty when it is just a caret).
    pub fn selection(&self) -> Range<usize> {
        self.selected_range.clone()
    }

    pub fn cursor(&self) -> usize {
        self.cursor_offset()
    }

    /// The selected text, or `None` when nothing is selected.
    pub fn selected_text(&self) -> Option<&str> {
        (!self.selected_range.is_empty()).then(|| &self.content[self.selected_range.clone()])
    }

    /// Replaces the whole text without emitting [`TextInputEvent::Changed`].
    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        let text = self.sanitize(text.into());
        if text == self.content {
            return;
        }
        self.content = text;
        let end = self.content.len();
        self.selected_range = end..end;
        self.selection_reversed = false;
        self.marked_range = None;
        self.highlights.clear();
        cx.notify();
    }

    pub fn set_masked(&mut self, masked: bool, cx: &mut Context<Self>) {
        self.masked = masked;
        cx.notify();
    }

    pub fn set_read_only(&mut self, read_only: bool, cx: &mut Context<Self>) {
        self.read_only = read_only;
        cx.notify();
    }

    pub fn set_placeholder(&mut self, placeholder: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.placeholder = placeholder.into();
        cx.notify();
    }

    /// Styles byte ranges of the text (sorted, non-overlapping, on char
    /// boundaries). Ignored while masked. Cleared by [`Self::set_text`].
    pub fn set_highlights(
        &mut self,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
        cx: &mut Context<Self>,
    ) {
        self.highlights = highlights;
        cx.notify();
    }

    /// Height of one text row as last painted (`None` before the first paint).
    pub fn line_height(&self) -> Option<Pixels> {
        self.last_layout.as_ref().map(|layout| layout.line_height)
    }

    /// Selects `range` (byte offsets, clamped to char boundaries).
    pub fn select_range(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        let start = self.clamp_offset(range.start.min(range.end));
        let end = self.clamp_offset(range.start.max(range.end));
        self.selected_range = start..end;
        self.selection_reversed = false;
        cx.notify();
    }

    /// Inserts `text` at the caret (replacing the selection) as if typed.
    pub fn insert(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_text_in_range(None, text, window, cx);
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus_handle, cx);
    }

    fn sanitize(&self, text: String) -> String {
        if self.multiline {
            text.replace("\r\n", "\n")
        } else {
            text.replace(['\r', '\n'], " ")
        }
    }

    fn clamp_offset(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.content.len());
        while !self.content.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    fn left(&mut self, _: &Left, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), window, cx);
        } else {
            self.move_to(self.selected_range.start, window, cx)
        }
    }

    fn right(&mut self, _: &Right, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), window, cx);
        } else {
            self.move_to(self.selected_range.end, window, cx)
        }
    }

    fn up(&mut self, _: &Up, window: &mut Window, cx: &mut Context<Self>) {
        if !self.multiline {
            cx.propagate();
            return;
        }
        let target = self.vertical_offset(self.cursor_offset(), -1);
        self.move_to(target, window, cx);
    }

    fn down(&mut self, _: &Down, window: &mut Window, cx: &mut Context<Self>) {
        if !self.multiline {
            cx.propagate();
            return;
        }
        let target = self.vertical_offset(self.cursor_offset(), 1);
        self.move_to(target, window, cx);
    }

    fn word_left(&mut self, _: &WordLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.previous_word_boundary(self.cursor_offset()), window, cx);
    }

    fn word_right(&mut self, _: &WordRight, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.next_word_boundary(self.cursor_offset()), window, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), window, cx);
    }

    fn select_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), window, cx);
    }

    fn select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        if !self.multiline {
            cx.propagate();
            return;
        }
        self.select_to(self.vertical_offset(self.cursor_offset(), -1), window, cx);
    }

    fn select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        if !self.multiline {
            cx.propagate();
            return;
        }
        self.select_to(self.vertical_offset(self.cursor_offset(), 1), window, cx);
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_word_boundary(self.cursor_offset()), window, cx);
    }

    fn select_word_right(
        &mut self,
        _: &SelectWordRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_to(self.next_word_boundary(self.cursor_offset()), window, cx);
    }

    fn select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, window, cx);
        self.select_to(self.content.len(), window, cx)
    }

    fn home(&mut self, _: &Home, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.line_start(self.cursor_offset()), window, cx);
    }

    fn end(&mut self, _: &End, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.line_end(self.cursor_offset()), window, cx);
    }

    fn select_home(&mut self, _: &SelectHome, window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.line_start(self.cursor_offset()), window, cx);
    }

    fn select_end(&mut self, _: &SelectEnd, window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.line_end(self.cursor_offset()), window, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        if self.selected_range.is_empty() {
            let previous = self.previous_boundary(self.cursor_offset());
            if self.cursor_offset() == previous {
                return;
            }
            self.select_to(previous, window, cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            if self.cursor_offset() == next {
                return;
            }
            self.select_to(next, window, cx)
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn delete_word_left(&mut self, _: &DeleteWordLeft, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        if self.selected_range.is_empty() {
            self.select_to(self.previous_word_boundary(self.cursor_offset()), window, cx);
        }
        self.replace_text_in_range(None, "", window, cx)
    }

    fn enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        if self.multiline && !self.read_only {
            let indent = self.current_indent();
            self.replace_text_in_range(None, &format!("\n{indent}"), window, cx);
        } else if self.multiline {
            cx.propagate();
        } else {
            cx.emit(TextInputEvent::Submit);
        }
    }

    fn submit(&mut self, _: &Submit, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(TextInputEvent::Submit);
    }

    fn tab(&mut self, _: &Tab, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            cx.propagate();
            return;
        }
        self.replace_text_in_range(None, "  ", window, cx);
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.is_selecting = true;
        let offset = self.index_for_mouse_position(event.position);
        if event.click_count >= 3 {
            let start = self.line_start(offset);
            let end = self.line_end(offset);
            self.move_to(start, window, cx);
            self.select_to(end, window, cx);
        } else if event.click_count == 2 {
            let start = self.previous_word_boundary(self.next_boundary(offset));
            let end = self.next_word_boundary(start);
            self.move_to(start, window, cx);
            self.select_to(end, window, cx);
        } else if event.modifiers.shift {
            self.select_to(offset, window, cx);
        } else {
            self.move_to(offset, window, cx)
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), window, cx);
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let text = self.sanitize(text);
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() && !self.masked {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() && !self.masked && !self.read_only {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx)
        }
    }

    fn move_to(&mut self, offset: usize, window: &Window, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify()
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn select_to(&mut self, offset: usize, window: &Window, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify()
    }

    fn line_start(&self, offset: usize) -> usize {
        self.content[..offset].rfind('\n').map_or(0, |index| index + 1)
    }

    fn line_end(&self, offset: usize) -> usize {
        self.content[offset..]
            .find('\n')
            .map_or(self.content.len(), |index| offset + index)
    }

    fn current_indent(&self) -> String {
        let start = self.line_start(self.cursor_offset());
        self.content[start..self.cursor_offset()]
            .chars()
            .take_while(|character| *character == ' ' || *character == '\t')
            .collect()
    }

    /// Offset one line above (`delta < 0`) or below, keeping the x position.
    fn vertical_offset(&self, offset: usize, delta: isize) -> usize {
        let line_starts = self.line_starts();
        let line_index = line_starts
            .iter()
            .rposition(|start| *start <= offset)
            .unwrap_or(0);
        let target_index = line_index as isize + delta;
        if target_index < 0 {
            return 0;
        }
        let target_index = target_index as usize;
        if target_index >= line_starts.len() {
            return self.content.len();
        }

        let target_start = line_starts[target_index];
        let target_end = self.line_end(target_start);
        let layout = self.last_layout.as_ref().filter(|layout| {
            layout.lines.len() == line_starts.len()
        });
        if let Some(layout) = layout
            && let (Some(current), Some(target)) =
                (layout.lines.get(line_index), layout.lines.get(target_index))
        {
            let x = current
                .shaped
                .x_for_index(self.display_offset(line_starts[line_index], offset));
            let display_index = target.shaped.closest_index_for_x(x);
            return self.content_offset(target_start, target_end, display_index);
        }

        let column = self.content[line_starts[line_index]..offset].chars().count();
        self.content[target_start..target_end]
            .char_indices()
            .nth(column)
            .map_or(target_end, |(index, _)| target_start + index)
    }

    fn line_starts(&self) -> Vec<usize> {
        let mut starts = vec![0];
        starts.extend(self.content.match_indices('\n').map(|(index, _)| index + 1));
        starts
    }

    /// Byte offset within the displayed (possibly masked) line for a content offset.
    fn display_offset(&self, line_start: usize, offset: usize) -> usize {
        if self.masked {
            self.content[line_start..offset].chars().count() * MASK_CHAR.len_utf8()
        } else {
            offset - line_start
        }
    }

    /// Content offset for a byte index into the displayed line `line_start..line_end`.
    fn content_offset(&self, line_start: usize, line_end: usize, display_index: usize) -> usize {
        let line = &self.content[line_start..line_end];
        if self.masked {
            let chars = display_index / MASK_CHAR.len_utf8();
            line.char_indices()
                .nth(chars)
                .map_or(line_end, |(index, _)| line_start + index)
        } else {
            line_start + display_index.min(line.len())
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        let Some(layout) = self.last_layout.as_ref() else {
            return 0;
        };
        if self.content.is_empty() || layout.lines.is_empty() {
            return 0;
        }
        if position.y < layout.bounds.top() {
            return 0;
        }
        let row = ((position.y - layout.bounds.top()) / layout.line_height).floor() as usize;
        let Some(line) = layout.lines.get(row) else {
            return self.content.len();
        };
        let x = position.x - layout.bounds.left() + layout.scroll_x;
        let display_index = line.shaped.closest_index_for_x(x);
        self.content_offset(
            line.content_range.start,
            line.content_range.end,
            display_index,
        )
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for character in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += character.len_utf16();
            utf8_offset += character.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for character in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += character.len_utf8();
            utf16_offset += character.len_utf16();
        }
        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.content.len())
    }

    fn previous_word_boundary(&self, offset: usize) -> usize {
        if self.masked {
            return self.line_start(offset);
        }
        self.content
            .split_word_bound_indices()
            .rev()
            .find(|(index, word)| *index < offset && !word.trim().is_empty())
            .map_or(0, |(index, _)| index)
    }

    fn next_word_boundary(&self, offset: usize) -> usize {
        if self.masked {
            return self.line_end(offset);
        }
        self.content
            .split_word_bound_indices()
            .find(|(index, word)| index + word.len() > offset && !word.trim().is_empty())
            .map_or(self.content.len(), |(index, word)| index + word.len())
    }

    fn cursor_bounds(&self, offset: usize) -> Option<Bounds<Pixels>> {
        let layout = self.last_layout.as_ref()?;
        let row = layout
            .lines
            .iter()
            .rposition(|line| line.content_range.start <= offset)?;
        let line = layout.lines.get(row)?;
        let x = line
            .shaped
            .x_for_index(self.display_offset(line.content_range.start, offset));
        let top = layout.bounds.top() + layout.line_height * row as f32;
        Some(Bounds::new(
            point(layout.bounds.left() + x - layout.scroll_x, top),
            size(px(1.), layout.line_height),
        ))
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        self.content.get(range).map(str::to_string)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_range = self.range_from_utf16(&range_utf16);
        self.selection_reversed = false;
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify();
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.content.encode_utf16().count())
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.marked_range = None;
        window.notify_text_input_changed(&self.focus_handle);
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let new_text = self.sanitize(new_text.to_string());
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content.replace_range(range.clone(), &new_text);
        let caret = range.start + new_text.len();
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.marked_range.take();
        window.notify_text_input_changed(&self.focus_handle);
        cx.emit(TextInputEvent::Changed);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content.replace_range(range.clone(), new_text);
        self.marked_range =
            (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
        let insertion_start_utf16 = self.offset_to_utf16(range.start);
        let inserted_length_utf16 = new_text.encode_utf16().count();
        self.selected_range = new_selected_range_utf16
            .map(|selected| {
                self.range_from_utf16(
                    &(insertion_start_utf16 + selected.start.min(inserted_length_utf16)
                        ..insertion_start_utf16 + selected.end.min(inserted_length_utf16)),
                )
            })
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        self.selection_reversed = false;
        window.notify_text_input_changed(&self.focus_handle);
        cx.emit(TextInputEvent::Changed);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let start = self.cursor_bounds(range.start)?;
        let end = self.cursor_bounds(range.end)?;
        Some(Bounds::from_corners(start.origin, end.bottom_right()))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        self.last_layout.as_ref()?;
        Some(self.offset_to_utf16(self.index_for_mouse_position(point)))
    }
}

/// Runs for the line `line` (content byte range) with `highlights` applied
/// over `base`.
fn highlight_runs(
    base: &TextRun,
    line: Range<usize>,
    highlights: &[(Range<usize>, HighlightStyle)],
) -> Vec<TextRun> {
    let first = highlights.partition_point(|(range, _)| range.end <= line.start);
    let mut runs = Vec::new();
    let mut pos = line.start;
    for (range, style) in &highlights[first..] {
        if range.start >= line.end {
            break;
        }
        let start = range.start.max(line.start);
        let end = range.end.min(line.end);
        if start > pos {
            runs.push(TextRun {
                len: start - pos,
                ..base.clone()
            });
        }
        if end > start {
            runs.push(TextRun {
                len: end - start,
                color: style.color.unwrap_or(base.color),
                background_color: style.background_color,
                ..base.clone()
            });
        }
        pos = pos.max(end);
    }
    if pos < line.end || runs.is_empty() {
        runs.push(TextRun {
            len: line.end - pos,
            ..base.clone()
        });
    }
    runs
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    lines: Vec<LayoutLine>,
    cursor: Option<PaintQuad>,
    selections: Vec<PaintQuad>,
    scroll_x: Pixels,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let input = self.input.read(cx);
        let rows = if input.multiline {
            (input.content.matches('\n').count() + 1).max(input.min_lines)
        } else {
            1
        };
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = (window.line_height() * rows as f32).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let colors = cx.theme().colors();
        let player = cx.theme().players().local();

        let mut lines = Vec::new();
        if input.content.is_empty() {
            let placeholder = input.placeholder.clone();
            let run = TextRun {
                len: placeholder.len(),
                font: style.font(),
                color: colors.text_placeholder,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window
                .text_system()
                .shape_line(placeholder, font_size, &[run], None);
            lines.push(LayoutLine {
                shaped,
                content_range: 0..0,
            });
        } else {
            let mut line_start = 0;
            for line_text in input.content.split('\n') {
                let line_end = line_start + line_text.len();
                let display: SharedString = if input.masked {
                    MASK_CHAR.to_string().repeat(line_text.chars().count()).into()
                } else {
                    line_text.to_string().into()
                };
                let base = TextRun {
                    len: display.len(),
                    font: style.font(),
                    color: style.color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let runs = match input.marked_range.as_ref() {
                    Some(marked)
                        if !input.masked
                            && marked.start >= line_start
                            && marked.end <= line_end =>
                    {
                        let marked_start = marked.start - line_start;
                        let marked_end = marked.end - line_start;
                        [
                            TextRun {
                                len: marked_start,
                                ..base.clone()
                            },
                            TextRun {
                                len: marked_end - marked_start,
                                underline: Some(UnderlineStyle {
                                    color: Some(base.color),
                                    thickness: px(1.),
                                    wavy: false,
                                }),
                                ..base.clone()
                            },
                            TextRun {
                                len: display.len() - marked_end,
                                ..base
                            },
                        ]
                        .into_iter()
                        .filter(|run| run.len > 0)
                        .collect()
                    }
                    _ if !input.masked && !input.highlights.is_empty() => {
                        highlight_runs(&base, line_start..line_end, &input.highlights)
                    }
                    _ => vec![base],
                };
                let shaped = window
                    .text_system()
                    .shape_line(display, font_size, &runs, None);
                lines.push(LayoutLine {
                    shaped,
                    content_range: line_start..line_end,
                });
                line_start = line_end + 1;
            }
        }

        let cursor_offset = input.cursor_offset();
        let cursor_row = lines
            .iter()
            .rposition(|line| line.content_range.start <= cursor_offset)
            .unwrap_or(0);
        let cursor_x = lines.get(cursor_row).map_or(px(0.), |line| {
            line.shaped
                .x_for_index(input.display_offset(line.content_range.start, cursor_offset))
        });

        let width = bounds.size.width;
        let mut scroll_x = input.scroll_x;
        let margin = px(4.);
        if cursor_x - scroll_x > width - margin {
            scroll_x = cursor_x - width + margin;
        } else if cursor_x < scroll_x {
            scroll_x = cursor_x;
        }
        let widest = lines
            .iter()
            .map(|line| line.shaped.width)
            .fold(px(0.), |a, b| a.max(b));
        scroll_x = scroll_x.min((widest - width + margin).max(px(0.))).max(px(0.));

        let row_top = |row: usize| bounds.top() + line_height * row as f32;
        let selected = input.selected_range.clone();
        let mut selections = Vec::new();
        let mut cursor = None;
        if selected.is_empty() {
            cursor = Some(fill(
                Bounds::new(
                    point(bounds.left() + cursor_x - scroll_x, row_top(cursor_row)),
                    size(px(2.), line_height),
                ),
                player.cursor,
            ));
        } else if !input.content.is_empty() {
            for (row, line) in lines.iter().enumerate() {
                let range = &line.content_range;
                if range.end < selected.start || range.start > selected.end {
                    continue;
                }
                let start = selected.start.max(range.start);
                let end = selected.end.min(range.end);
                let x_start = line.shaped.x_for_index(input.display_offset(range.start, start));
                let mut x_end = line.shaped.x_for_index(input.display_offset(range.start, end));
                if selected.end > range.end {
                    x_end += px(6.);
                }
                selections.push(fill(
                    Bounds::from_corners(
                        point(bounds.left() + x_start - scroll_x, row_top(row)),
                        point(bounds.left() + x_end - scroll_x, row_top(row) + line_height),
                    ),
                    player.selection,
                ));
            }
        }

        PrepaintState {
            lines,
            cursor,
            selections,
            scroll_x,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );

        let line_height = window.line_height();
        let scroll_x = prepaint.scroll_x;
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            for selection in prepaint.selections.drain(..) {
                window.paint_quad(selection);
            }
            for (row, line) in prepaint.lines.iter().enumerate() {
                let origin = point(
                    bounds.left() - scroll_x,
                    bounds.top() + line_height * row as f32,
                );
                if let Err(error) = line.shaped.paint(
                    origin,
                    line_height,
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                ) {
                    eprintln!("text input: failed to paint line: {error:#}");
                }
            }
            if focus_handle.is_focused(window)
                && let Some(cursor) = prepaint.cursor.take()
            {
                window.paint_quad(cursor);
            }
        });

        let layout = TextLayout {
            lines: std::mem::take(&mut prepaint.lines),
            bounds,
            line_height,
            scroll_x,
        };
        self.input.update(cx, |input, _| {
            input.scroll_x = scroll_x;
            input.last_layout = Some(layout);
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let focused = self.focus_handle.is_focused(window);
        let mut key_context = gpui::KeyContext::new_with_defaults();
        key_context.add(KEY_CONTEXT);
        if self.multiline {
            key_context.add("multiline");
        }

        h_flex()
            .key_context(key_context)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::delete_word_left))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::tab))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .w_full()
            .min_w_0()
            .items_start()
            .text_color(colors.text)
            .when(self.monospace, |this| this.font_buffer(cx).text_buffer(cx))
            .when(!self.monospace, |this| this.text_ui(cx))
            .when(self.bordered, |this| {
                this.px_2()
                    .py_1()
                    .rounded_sm()
                    .border_1()
                    .bg(colors.editor_background)
                    .border_color(if focused {
                        colors.border_focused
                    } else {
                        colors.border
                    })
            })
            .when(self.read_only, |this| this.text_color(colors.text_muted))
            .child(TextElement {
                input: cx.entity(),
            })
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
