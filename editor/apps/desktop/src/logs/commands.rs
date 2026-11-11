//! What the user can do in the window: manage sources, filter, follow,
//! select, copy and export.

use crate::logs::{LogsView, Selection};
use ferrum_ui::gpui::{ClipboardItem, ScrollStrategy};
use ferrum_ui::prelude::*;
use ferrum_ui::project;
use ferrumlogs_core::{Filter, Levels, SourceConfig};

impl LogsView {
    /// Adds or replaces (same id) a source of the current project.
    pub fn save_source(&mut self, config: SourceConfig, start: bool, cx: &mut Context<Self>) {
        let launcher = self.launcher.clone();
        let Some(session) = self.session_mut() else {
            return;
        };
        let mut configs = session.configs();
        let id = config.id.clone();
        let changed = match configs.iter_mut().find(|c| c.id == id) {
            Some(existing) => {
                let changed = existing.kind != config.kind;
                *existing = config;
                changed
            }
            None => {
                configs.push(config);
                false
            }
        };
        session.set_sources(configs);
        let running = session.source(&id).is_some_and(|s| s.is_running());
        if start || (changed && running) {
            session.start(&id, &launcher);
        }
        self.persist(cx);
        cx.notify();
    }

    pub fn remove_source(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(session) = self.session_mut() else {
            return;
        };
        let configs = session
            .configs()
            .into_iter()
            .filter(|c| c.id != id)
            .collect();
        session.set_sources(configs);
        self.persist(cx);
        cx.notify();
    }

    pub fn start_source(&mut self, id: &str, cx: &mut Context<Self>) {
        let launcher = self.launcher.clone();
        if let Some(session) = self.session_mut() {
            session.start(id, &launcher);
        }
        self.after_start(cx);
    }

    pub fn stop_source(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(session) = self.session_mut() {
            session.stop(id);
        }
        cx.notify();
    }

    pub fn start_all(&mut self, cx: &mut Context<Self>) {
        let launcher = self.launcher.clone();
        if let Some(session) = self.session_mut() {
            session.start_all(&launcher);
        }
        self.after_start(cx);
    }

    pub fn stop_all(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.session_mut() {
            session.stop_all();
        }
        cx.notify();
    }

    /// A start writes a `$ command` line: keep it in view.
    fn after_start(&mut self, cx: &mut Context<Self>) {
        if self.follow {
            self.scroll_to_end();
        }
        cx.notify();
    }

    pub fn update_filter(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Filter)) {
        if let Some(session) = self.session_mut() {
            session.update_filter(f);
        }
        self.unseen = 0;
        if self.follow {
            self.scroll_to_end();
        }
        cx.notify();
    }

    pub fn toggle_errors_only(&mut self, cx: &mut Context<Self>) {
        let Some(levels) = self.session().map(|s| s.filter().levels) else {
            return;
        };
        let next = if levels == Levels::ERRORS_ONLY {
            self.saved_levels.take().unwrap_or_default()
        } else {
            self.saved_levels = Some(levels);
            Levels::ERRORS_ONLY
        };
        self.update_filter(cx, |f| f.levels = next);
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.session_mut() {
            session.clear();
        }
        self.selection = None;
        self.unseen = 0;
        cx.notify();
    }

    pub(crate) fn scroll_to_end(&self) {
        let count = self.session().map_or(0, |s| s.view.len());
        if count > 0 {
            self.scroll
                .scroll_to_item_strict(count - 1, ScrollStrategy::Bottom);
        }
    }

    pub fn set_follow(&mut self, follow: bool, cx: &mut Context<Self>) {
        self.follow = follow;
        if follow {
            self.unseen = 0;
            self.scroll_to_end();
        }
        cx.notify();
    }

    /// Mouse wheel over the list: scrolling up pauses following; scrolling
    /// back down to the end resumes it (checked after layout, in render).
    pub(crate) fn on_wheel(&mut self, up: bool, down: bool, cx: &mut Context<Self>) {
        if up && self.follow {
            self.follow = false;
            cx.notify();
        } else if down && !self.follow {
            self.resume_check = true;
            cx.notify();
        }
    }

    /// Called while rendering: resumes following once scrolled to the end.
    pub(crate) fn check_resume(&mut self) {
        if self.resume_check {
            self.resume_check = false;
            if self.scroll.is_scrolled_to_end() == Some(true) {
                self.follow = true;
                self.unseen = 0;
            }
        }
    }

    /// Click selects a line; shift-click extends the selection.
    pub fn select_row(&mut self, row: usize, extend: bool, cx: &mut Context<Self>) {
        let Some(seq) = self.session().and_then(|s| s.view.seq(row)) else {
            return;
        };
        self.selection = match self.selection {
            Some(sel) if extend => Some(Selection {
                anchor: sel.anchor,
                head: seq,
            }),
            Some(sel) if sel.anchor == seq && sel.head == seq => None,
            _ => Some(Selection {
                anchor: seq,
                head: seq,
            }),
        };
        cx.notify();
    }

    /// The selected lines that are still visible, as copied text.
    pub fn selection_text(&self) -> Option<String> {
        let (session, selection) = (self.session()?, self.selection?);
        let range = selection.range();
        let text: String = session
            .view
            .seqs()
            .filter(|seq| range.contains(seq))
            .filter_map(|seq| session.buffer.get(seq))
            .map(|line| session.format_line(line) + "\n")
            .collect();
        (!text.is_empty()).then_some(text)
    }

    pub fn copy_selection(&mut self, cx: &mut Context<Self>) {
        match self.selection_text() {
            Some(text) => {
                let lines = text.lines().count();
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                let noun = if lines == 1 { "line" } else { "lines" };
                self.notify(Notice::info(format!("Copied {lines} {noun}")), cx);
            }
            None => self.notify(Notice::info("Select a line first"), cx),
        }
    }

    pub fn copy_text(&mut self, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.notify(Notice::info("Copied"), cx);
    }

    /// Asks where to save, then writes the visible lines there.
    pub fn export(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session() else {
            return;
        };
        let text = session.visible_text();
        let name = format!("{}-logs.txt", project::project_name(&session.project));
        let receiver = cx.prompt_for_new_path(&session.project, Some(&name));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            let notice = match std::fs::write(&path, text) {
                Ok(()) => Notice::info(format!("Exported to {}", path.display())),
                Err(e) => Notice::error(format!("Export failed: {e}")),
            };
            this.update(cx, |app, cx| app.notify(notice, cx)).ok();
        })
        .detach();
    }

    pub fn notify(&mut self, notice: Notice, cx: &mut Context<Self>) {
        self.notice = Some(notice);
        self.notice_task = Some(Notice::dismiss_later(cx, |this: &mut Self, _| {
            this.notice = None;
        }));
        cx.notify();
    }
}
