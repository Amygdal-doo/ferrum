//! Closing a window and quitting with several windows open: quitting asks
//! about unsaved changes in every window; closing one of several windows
//! asks about its own and leaves the others running.

use ferrum_ui::prelude::*;
use gpui::PromptLevel;

use super::Workspace;
use crate::editor::EditorView;

impl Workspace {
    fn dirty_editors(&self, cx: &App) -> Vec<Entity<EditorView>> {
        self.all_editors()
            .into_iter()
            .filter(|e| e.read(cx).is_dirty())
            .collect()
    }

    /// The window's close button: the last window quits; another closes
    /// after asking about its unsaved changes. Returns false (GPUI closes
    /// nothing by itself; the window is removed here when it may go).
    pub fn close_window(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let me = cx.entity_id();
        let others = crate::windows::all(cx)
            .into_iter()
            .filter(|(_, w)| w.entity_id() != me)
            .count();
        if others == 0 {
            self.quit(window, cx);
            return false;
        }
        let dirty = self.dirty_editors(cx);
        if dirty.is_empty() {
            self.save_session(cx);
            window.remove_window();
            return false;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!(
                "You have unsaved changes in {} file{}.",
                dirty.len(),
                if dirty.len() == 1 { "" } else { "s" }
            ),
            Some("Save them before closing the window?"),
            &["Save All", "Don't Save", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let Ok(answer) = answer.await else { return };
            this.update_in(cx, |this, window, cx| {
                match answer {
                    0 => {
                        if dirty.iter().any(|e| e.read(cx).path.is_none()) {
                            this.error("Save or close untitled files first", cx);
                            return;
                        }
                        this.save_all(window, cx);
                    }
                    1 => {}
                    _ => return,
                }
                this.save_session(cx);
                window.remove_window();
            })
            .ok();
        })
        .detach();
        false
    }

    /// Quits after saving the session, asking about unsaved changes in
    /// every window.
    pub fn quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let me = cx.entity_id();
        let mut dirty = self.dirty_editors(cx);
        let elsewhere: Vec<Entity<EditorView>> = crate::windows::all(cx)
            .into_iter()
            .filter(|(_, w)| w.entity_id() != me)
            .flat_map(|(_, w)| w.read(cx).dirty_editors(cx))
            .collect();
        let own = dirty.len();
        dirty.extend(elsewhere.iter().cloned());
        if dirty.is_empty() {
            self.save_session(cx);
            cx.quit();
            return;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!(
                "You have unsaved changes in {} file{}.",
                dirty.len(),
                if dirty.len() == 1 { "" } else { "s" }
            ),
            Some(if own == dirty.len() {
                "Save them before quitting?"
            } else {
                "Some are in other windows. Save them before quitting?"
            }),
            &["Save All", "Don't Save", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let Ok(answer) = answer.await else { return };
            this.update_in(cx, |this, window, cx| match answer {
                0 => {
                    if dirty.iter().any(|e| e.read(cx).path.is_none()) {
                        this.error("Save or close untitled files first", cx);
                        return;
                    }
                    this.save_all(window, cx);
                    for editor in &elsewhere {
                        if let Err(error) = editor.update(cx, |e, cx| e.save(None, cx)) {
                            this.error(format!("Could not save: {error}"), cx);
                            return;
                        }
                    }
                    this.save_session(cx);
                    cx.quit();
                }
                1 => {
                    this.save_session(cx);
                    cx.quit();
                }
                _ => {}
            })
            .ok();
        })
        .detach();
    }
}
