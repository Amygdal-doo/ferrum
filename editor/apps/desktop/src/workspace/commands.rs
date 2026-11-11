//! Workspace-level command handlers, registered by name. Editor commands
//! (`editor:*`) are handled by the focused editor, or forwarded to the
//! active one by `Workspace::run_command`.

use ferrum_ui::prelude::*;
use ferrumeditor_core::session::SplitAxis;

use super::Workspace;
use crate::picker::PickerKind;
use crate::registry::CommandRegistry;
use crate::settings::Settings;

pub(super) fn builtin_registry() -> CommandRegistry {
    let mut r = CommandRegistry::default();
    r.register("application:new-file", |w, window, cx| {
        w.new_file(window, cx)
    });
    r.register("application:open-file", |w, window, cx| {
        w.open_file_dialog(window, cx)
    });
    r.register("application:open-folder", |w, window, cx| {
        w.open_folder_dialog(window, cx)
    });
    r.register("application:open-config", |w, window, cx| {
        w.open_settings_file(SettingsFile::Config, window, cx)
    });
    r.register("application:open-keymap", |w, window, cx| {
        w.open_settings_file(SettingsFile::Keymap, window, cx)
    });
    r.register("application:open-snippets", |w, window, cx| {
        w.open_settings_file(SettingsFile::Snippets, window, cx)
    });
    r.register("application:open-theme", |w, window, cx| {
        w.open_settings_file(SettingsFile::Theme, window, cx)
    });
    r.register("application:about", |w, window, cx| {
        w.show_about(window, cx)
    });
    r.register("application:quit", |w, window, cx| w.quit(window, cx));
    r.register("core:save", |w, window, cx| {
        if let Some(editor) = w.active_editor() {
            w.save_editor(editor, false, None, window, cx);
        }
    });
    r.register("core:save-as", |w, window, cx| {
        if let Some(editor) = w.active_editor() {
            w.save_editor(editor, true, None, window, cx);
        }
    });
    r.register("core:save-all", |w, window, cx| w.save_all(window, cx));
    r.register("core:close-tab", |w, window, cx| {
        w.close_active_tab(window, cx)
    });
    r.register("core:reopen-closed-tab", |w, window, cx| {
        w.reopen_closed_tab(window, cx)
    });
    r.register("core:reload-file", |w, _, cx| {
        if let Some(editor) = w.active_editor()
            && let Err(error) = editor.update(cx, |e, cx| e.reload_from_disk(cx))
        {
            w.error(format!("Reload failed: {error}"), cx);
        }
    });
    r.register("pane:next-tab", |w, window, cx| w.cycle_tab(1, window, cx));
    r.register("pane:previous-tab", |w, window, cx| {
        w.cycle_tab(-1, window, cx)
    });
    r.register("pane:split-right", |w, window, cx| {
        w.split(SplitAxis::Horizontal, window, cx)
    });
    r.register("pane:split-down", |w, window, cx| {
        w.split(SplitAxis::Vertical, window, cx)
    });
    r.register("pane:close", |w, window, cx| {
        let pane = w.active_pane;
        w.close_pane(pane, window, cx);
    });
    r.register("pane:focus-next", |w, window, cx| {
        w.focus_pane(1, window, cx)
    });
    r.register("pane:focus-previous", |w, window, cx| {
        w.focus_pane(-1, window, cx)
    });
    r.register("tree-view:toggle", |w, window, cx| {
        w.toggle_tree(window, cx)
    });
    r.register("tree-view:reveal-active-file", |w, window, cx| {
        w.reveal_active_file(window, cx)
    });
    r.register("tree-view:focus", |w, window, cx| w.focus_tree(window, cx));
    r.register("fuzzy-finder:toggle-file-finder", |w, window, cx| {
        w.toggle_picker(PickerKind::Files, window, cx)
    });
    r.register("command-palette:toggle", |w, window, cx| {
        w.toggle_picker(PickerKind::Commands, window, cx)
    });
    r.register("go-to-line:toggle", |w, window, cx| {
        w.toggle_picker(PickerKind::GoToLine, window, cx)
    });
    r.register("find-and-replace:show", |w, window, cx| {
        w.show_find_bar(false, window, cx)
    });
    r.register("find-and-replace:show-replace", |w, window, cx| {
        w.show_find_bar(true, window, cx)
    });
    r.register("find-and-replace:find-next", |w, window, cx| {
        w.find_in_active(true, window, cx)
    });
    r.register("find-and-replace:find-previous", |w, window, cx| {
        w.find_in_active(false, window, cx)
    });
    r.register("find-and-replace:use-selection-as-find", |w, window, cx| {
        w.use_selection_as_find(window, cx)
    });
    r.register("find-in-files:show", |w, window, cx| {
        w.show_project_find(window, cx)
    });
    r.register("markdown-preview:toggle", |w, window, cx| {
        w.toggle_markdown_preview(window, cx)
    });
    r.register("window:increase-font-size", |_, _, cx| {
        adjust_font(cx, Some(1.0))
    });
    r.register("window:decrease-font-size", |_, _, cx| {
        adjust_font(cx, Some(-1.0))
    });
    r.register("window:reset-font-size", |_, _, cx| adjust_font(cx, None));
    r.register("theme:use-dark", |w, _, cx| w.use_theme("ferrum-dark", cx));
    r.register("theme:use-light", |w, _, cx| {
        w.use_theme("ferrum-light", cx)
    });
    r
}

fn adjust_font(cx: &mut Context<Workspace>, delta: Option<f32>) {
    let settings = cx.global_mut::<Settings>();
    settings.font_size_delta = match delta {
        Some(d) => (settings.font_size_delta + d).clamp(-6.0, 40.0),
        None => 0.0,
    };
    cx.refresh_windows();
}

#[derive(Clone, Copy)]
pub(super) enum SettingsFile {
    Config,
    Keymap,
    Snippets,
    Theme,
}

impl Workspace {
    fn open_settings_file(
        &mut self,
        which: SettingsFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(files) = cx.global::<Settings>().files.clone() else {
            self.error("No home folder: settings live in memory only", cx);
            return;
        };
        let path = match which {
            SettingsFile::Config => files.config,
            SettingsFile::Keymap => files.keymap,
            SettingsFile::Snippets => files.snippets,
            SettingsFile::Theme => files.theme,
        };
        self.open_file(&path, window, cx);
    }

    fn use_theme(&mut self, name: &str, cx: &mut Context<Self>) {
        let settings = cx.global_mut::<Settings>();
        settings.set_theme(name);
        let theme = settings.theme.clone();
        crate::theme_apply::apply(&theme, cx);
    }

    fn toggle_tree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tree.is_none() {
            self.info("Open a folder to show the tree view (⌘⇧O)", cx);
            return;
        }
        self.tree_visible = !self.tree_visible;
        if !self.tree_visible {
            self.focus_active(window, cx);
        }
        cx.notify();
    }

    fn focus_tree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.tree.clone() else {
            return;
        };
        if tree.read(cx).is_focused(window) {
            self.focus_active(window, cx);
        } else {
            self.tree_visible = true;
            tree.update(cx, |tree, cx| tree.focus(window, cx));
        }
        cx.notify();
    }

    fn reveal_active_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(tree), Some(editor)) = (self.tree.clone(), self.active_editor()) else {
            return;
        };
        let Some(path) = editor.read(cx).path.clone() else {
            return;
        };
        self.tree_visible = true;
        let found = tree.update(cx, |tree, cx| tree.reveal(&path, cx));
        if found {
            tree.update(cx, |tree, cx| tree.focus(window, cx));
        } else {
            self.info("The active file is outside the open folder", cx);
        }
        cx.notify();
    }

    /// Marks the active editor's file in the tree.
    pub(super) fn sync_tree_selection(&mut self, cx: &mut Context<Self>) {
        let path = self.active_editor().and_then(|e| e.read(cx).path.clone());
        if let Some(tree) = self.tree.clone() {
            tree.update(cx, |tree, cx| tree.set_active_file(path, cx));
        }
    }
}
