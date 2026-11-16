//! Editor commands by FerrumIDE name (`editor:undo`, `core:save`…), bound
//! to FerrumIDE's default keys in the editor's own key contexts.

use ferrumeditor_core::commands::{Scope, builtin_commands};
use ferrumeditor_core::editor::{ClipboardText, matching_bracket};
use ferrumeditor_core::keymap::Keymap;
use gpui::{App, ClipboardItem, Context, KeyBinding, SharedString, Window};

use super::CodeEditor;

/// Key context of the text surface: editing and motion keys.
pub const EDITOR_CONTEXT: &str = "FerrumGitEditor";
/// Key context of the whole editor (text and find bar): save and find.
pub const PANE_CONTEXT: &str = "FerrumGitEditorPane";

/// Clipboard metadata marking whole-line copies (pasted above the caret),
/// shared with FerrumIDE.
const LINE_MODE: &str = "ferrum-editor-line";

/// Commands that work while the find bar has focus too.
const PANE_COMMANDS: [&str; 5] = [
    "core:save",
    "find-and-replace:show",
    "find-and-replace:find-next",
    "find-and-replace:find-previous",
    "find-and-replace:use-selection-as-find",
];

#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(namespace = ferrumgit, no_json)]
pub struct EditorCommand {
    pub name: SharedString,
}

/// Binds FerrumIDE's default keys for the commands this editor runs.
pub fn bind_keys(cx: &mut App) {
    let keymap = Keymap::defaults(&builtin_commands(), cfg!(target_os = "macos"));
    let bindings: Vec<KeyBinding> = keymap
        .bindings
        .iter()
        .filter_map(|binding| {
            let command = binding.command.as_str();
            let context = if PANE_COMMANDS.contains(&command) {
                PANE_CONTEXT
            } else if binding.scope == Scope::Editor && EDITING_COMMANDS.contains(&command) {
                EDITOR_CONTEXT
            } else {
                return None;
            };
            let action = EditorCommand {
                name: SharedString::from(command.to_owned()),
            };
            Some(KeyBinding::new(&binding.keys, action, Some(context)))
        })
        .collect();
    cx.bind_keys(bindings);
}

/// The `editor:*` commands [`CodeEditor::run_command`] knows.
const EDITING_COMMANDS: [&str; 60] = [
    "editor:undo",
    "editor:redo",
    "editor:copy",
    "editor:cut",
    "editor:paste",
    "editor:select-all",
    "editor:select-line",
    "editor:newline",
    "editor:newline-below",
    "editor:newline-above",
    "editor:backspace",
    "editor:delete",
    "editor:delete-word-left",
    "editor:delete-word-right",
    "editor:delete-to-line-start",
    "editor:delete-to-line-end",
    "editor:delete-line",
    "editor:tab",
    "editor:shift-tab",
    "editor:indent",
    "editor:outdent",
    "editor:toggle-comment",
    "editor:duplicate-lines",
    "editor:move-line-up",
    "editor:move-line-down",
    "editor:join-lines",
    "editor:upper-case",
    "editor:lower-case",
    "editor:select-next",
    "editor:select-all-occurrences",
    "editor:split-selection-into-lines",
    "editor:add-cursor-above",
    "editor:add-cursor-below",
    "editor:go-to-matching-bracket",
    "editor:cancel",
    "editor:show-character-palette",
    "editor:move-left",
    "editor:move-right",
    "editor:move-up",
    "editor:move-down",
    "editor:select-left",
    "editor:select-right",
    "editor:select-up",
    "editor:select-down",
    "editor:move-to-previous-word",
    "editor:move-to-next-word",
    "editor:select-to-previous-word",
    "editor:select-to-next-word",
    "editor:move-to-line-start",
    "editor:move-to-line-end",
    "editor:select-to-line-start",
    "editor:select-to-line-end",
    "editor:move-to-top",
    "editor:move-to-bottom",
    "editor:select-to-top",
    "editor:select-to-bottom",
    "editor:page-up",
    "editor:page-down",
    "editor:select-page-up",
    "editor:select-page-down",
];

impl CodeEditor {
    pub(super) fn on_command(
        &mut self,
        action: &EditorCommand,
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

    /// Runs a command by name; false if it is not one this editor knows
    /// (or Escape had nothing to cancel).
    pub fn run_command(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match name {
            "core:save" => {
                // A failure is reported through `CodeEditorEvent::Error`.
                self.save(cx).ok();
                return true;
            }
            "find-and-replace:show" => {
                self.open_find(window, cx);
                return true;
            }
            "find-and-replace:use-selection-as-find" => {
                self.use_selection_as_find(window, cx);
                return true;
            }
            "find-and-replace:find-next" | "find-and-replace:find-previous" => {
                self.find_next(name.ends_with("next"), cx);
                return true;
            }
            _ => {}
        }
        let tab_size = self.state.settings.tab_size;
        self.display_map.sync(self.state.buffer(), tab_size, None);
        let version = self.state.buffer().version();
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
            "editor:tab" => s.tab(&[]),
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
            _ => return false,
        }
        self.marked_range = None;
        if self.state.buffer().version() != version {
            self.after_edit(cx);
        }
        self.autoscroll = true;
        cx.notify();
        true
    }
}

fn write_clipboard(copied: ClipboardText, cx: &mut Context<CodeEditor>) {
    let item = if copied.line_mode {
        ClipboardItem::new_string_with_metadata(copied.text, LINE_MODE.to_string())
    } else {
        ClipboardItem::new_string(copied.text)
    };
    cx.write_to_clipboard(item);
}
