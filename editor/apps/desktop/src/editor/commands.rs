//! `editor:*` commands, dispatched by name from key bindings, menus and
//! the command palette.

use ferrumeditor_core::editor::matching_bracket;
use gpui::{ClipboardItem, Context, Window};

use super::{EditorEvent, EditorView};
use crate::actions::RunCommand;
use crate::settings::Settings;

/// Clipboard metadata marking whole-line copies (pasted above the caret).
const LINE_MODE: &str = "ferrum-editor-line";

impl EditorView {
    pub(super) fn on_run_command(
        &mut self,
        action: &RunCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.run_command(&action.name, window, cx) {
            cx.propagate();
        }
    }

    fn page_rows(&self) -> isize {
        self.last_layout.as_ref().map_or(20, |l| {
            ((l.bounds.size.height / l.line_height).floor() as isize - 1).max(1)
        })
    }

    /// Runs an editor command; false if `name` is not one.
    pub fn run_command(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let version = self.state.buffer().version();
        let selections = self.state.selections().to_vec();
        let page = self.page_rows();
        let s = &mut self.state;
        let map = &self.display_map;
        match name {
            "editor:undo" => {
                s.undo();
            }
            "editor:redo" => {
                s.redo();
            }
            "editor:copy" => write_clipboard(s.copy(), cx),
            "editor:cut" => {
                let copied = s.cut();
                write_clipboard(copied, cx);
            }
            "editor:paste" => {
                if let Some(item) = cx.read_from_clipboard()
                    && let Some(text) = item.text()
                {
                    let line_mode = item.metadata().is_some_and(|m| m == LINE_MODE);
                    s.paste(&text, line_mode);
                }
            }
            "editor:select-all" => s.select_all(),
            "editor:select-line" => s.select_line(),
            "editor:newline" => s.newline(),
            "editor:newline-below" => s.newline_below(),
            "editor:newline-above" => s.newline_above(),
            "editor:backspace" => s.backspace(),
            "editor:delete" => s.delete(),
            "editor:delete-word-left" => s.delete_word_left(),
            "editor:delete-word-right" => s.delete_word_right(),
            "editor:delete-to-line-start" => s.delete_to_line_start(),
            "editor:delete-to-line-end" => s.delete_to_line_end(),
            "editor:delete-line" => s.delete_lines(),
            "editor:tab" => {
                let snippets = cx.global::<Settings>().snippets.for_language(s.language);
                s.tab(&snippets);
            }
            "editor:shift-tab" => s.shift_tab(),
            "editor:indent" => s.indent_lines(),
            "editor:outdent" => s.outdent_lines(),
            "editor:toggle-comment" => s.toggle_comment(),
            "editor:duplicate-lines" => s.duplicate_lines(),
            "editor:move-line-up" => s.move_lines_up(),
            "editor:move-line-down" => s.move_lines_down(),
            "editor:join-lines" => s.join_lines(),
            "editor:upper-case" => s.upper_case(),
            "editor:lower-case" => s.lower_case(),
            "editor:select-next" => s.select_next_occurrence(),
            "editor:select-all-occurrences" => s.select_all_occurrences(),
            "editor:split-selection-into-lines" => s.split_selection_into_lines(),
            "editor:add-cursor-above" => s.add_cursor_above(map),
            "editor:add-cursor-below" => s.add_cursor_below(map),
            "editor:go-to-matching-bracket" => {
                if let Some((_, partner)) = matching_bracket(s.buffer(), s.primary().head) {
                    s.set_cursor(partner.start);
                }
            }
            "editor:cancel" => {
                if !s.escape() {
                    return false;
                }
            }
            "editor:show-character-palette" => window.show_character_palette(),
            "editor:move-left" => s.move_left(false),
            "editor:move-right" => s.move_right(false),
            "editor:move-up" => s.move_vertical(-1, false, map),
            "editor:move-down" => s.move_vertical(1, false, map),
            "editor:select-left" => s.move_left(true),
            "editor:select-right" => s.move_right(true),
            "editor:select-up" => s.move_vertical(-1, true, map),
            "editor:select-down" => s.move_vertical(1, true, map),
            "editor:move-to-previous-word" => s.move_word_left(false),
            "editor:move-to-next-word" => s.move_word_right(false),
            "editor:select-to-previous-word" => s.move_word_left(true),
            "editor:select-to-next-word" => s.move_word_right(true),
            "editor:move-to-line-start" => s.move_line_start(false),
            "editor:move-to-line-end" => s.move_line_end(false),
            "editor:select-to-line-start" => s.move_line_start(true),
            "editor:select-to-line-end" => s.move_line_end(true),
            "editor:move-to-top" => s.move_doc_start(false),
            "editor:move-to-bottom" => s.move_doc_end(false),
            "editor:select-to-top" => s.move_doc_start(true),
            "editor:select-to-bottom" => s.move_doc_end(true),
            "editor:page-up" => s.move_vertical(-page, false, map),
            "editor:page-down" => s.move_vertical(page, false, map),
            "editor:select-page-up" => s.move_vertical(-page, true, map),
            "editor:select-page-down" => s.move_vertical(page, true, map),
            "editor:toggle-soft-wrap" => self.soft_wrap = !self.soft_wrap,
            "editor:toggle-line-numbers" => self.show_line_numbers = !self.show_line_numbers,
            "editor:toggle-indent-guides" => self.show_indent_guides = !self.show_indent_guides,
            _ => return false,
        }
        self.marked_range = None;
        if self.state.buffer().version() != version {
            self.after_edit(cx);
        }
        if self.state.selections() != selections.as_slice() {
            cx.emit(EditorEvent::SelectionsChanged);
        }
        self.request_autoscroll();
        cx.notify();
        true
    }
}

fn write_clipboard(copied: ferrumeditor_core::editor::ClipboardText, cx: &mut Context<EditorView>) {
    let item = if copied.line_mode {
        ClipboardItem::new_string_with_metadata(copied.text, LINE_MODE.to_string())
    } else {
        ClipboardItem::new_string(copied.text)
    };
    cx.write_to_clipboard(item);
}
