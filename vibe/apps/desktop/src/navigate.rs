//! Moving around: pages in the main card, threads across projects, new
//! tasks and discussions, back / forward, and the thread index behind the
//! sidebar.

use std::path::{Path, PathBuf};

use ferrum_ui::prelude::*;
use ferrumagent_ui::core::Mode;

use crate::app::{ChatAction, FerrumEngineer, SIDEBAR_FOLDERS};
use crate::nav::{Location, Page};
use crate::threads::ThreadIndex;

impl FerrumEngineer {
    /// A folder picked in the switcher, the folder dialog or the Projects
    /// page: switch to it and show its chat.
    pub fn open_project(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.set_project(path, window, cx);
        self.show_page(Page::Chat, window, cx);
    }

    /// Shows `page` in the main card (recorded for back / forward).
    pub fn show_page(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        if self.history.visit(Location::Page(page)) {
            self.page_opened(page, window, cx);
        }
        cx.notify();
    }

    fn page_opened(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        match page {
            Page::Chat => self.focus_chat(window, cx),
            Page::Search => {
                let input = self.pages.search.clone();
                input.update(cx, |input, cx| input.focus(window, cx));
            }
            _ => {}
        }
        if page != Page::Chat {
            self.refresh_index(cx);
            let project = self.project.clone();
            let mcp = self.chat().map(|c| c.read(cx).mcp_status().to_vec());
            self.pages
                .refresh(page, project.as_deref(), &self.agent_home, mcp, cx);
        }
    }

    pub(crate) fn focus_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(chat) = self.chat() {
            let handle = chat.focus_handle(cx);
            window.focus(&handle, cx);
            self.active_column = crate::layout::Column::Chat;
        }
    }

    /// Opens thread `id` of `project` (switching project if needed).
    pub fn open_thread(
        &mut self,
        project: &Path,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.history.visit(Location::Thread {
            project: project.to_path_buf(),
            id: id.to_string(),
        });
        self.go_to_thread(project, id, window, cx);
    }

    fn go_to_thread(
        &mut self,
        project: &Path,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.project.as_deref() != Some(project) {
            self.set_project(project.to_path_buf(), window, cx);
        }
        self.chat_action(project, ChatAction::OpenThread(id.to_string()), window, cx);
        self.focus_chat(window, cx);
        cx.notify();
    }

    /// A new thread in `mode` in the current project, optionally with a
    /// prompt sent right away (automations).
    pub fn start_task(
        &mut self,
        mode: Mode,
        prompt: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.project.clone() else {
            self.show_notice(Notice::error("Open a project folder first."), cx);
            return;
        };
        self.history.visit(Location::Page(Page::Chat));
        self.chat_action(&project, ChatAction::StartTask(mode, prompt), window, cx);
        self.focus_chat(window, cx);
        cx.notify();
    }

    /// The chat's review of everything its agent changed since the session
    /// started, per hunk (shown or hidden).
    pub fn review_changes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(chat) = self.chat() else {
            self.show_notice(Notice::error("Open a project folder first."), cx);
            return;
        };
        if self.page() != Page::Chat {
            self.show_page(Page::Chat, window, cx);
        }
        chat.update(cx, |chat, cx| chat.toggle_review(cx));
        self.focus_chat(window, cx);
        cx.notify();
    }

    /// FerrumChat's own settings (models, keys, agents).
    pub fn chat_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(project) = self.project.clone() {
            self.history.visit(Location::Page(Page::Chat));
            self.chat_action(&project, ChatAction::Settings, window, cx);
            cx.notify();
        }
    }

    pub fn go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(to) = self.history.back() {
            self.go(to, window, cx);
        }
    }

    pub fn go_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(to) = self.history.forward() {
            self.go(to, window, cx);
        }
    }

    fn go(&mut self, to: Location, window: &mut Window, cx: &mut Context<Self>) {
        match to {
            Location::Page(page) => self.page_opened(page, window, cx),
            Location::Thread { project, id } => {
                if project.is_dir() {
                    self.go_to_thread(&project, &id, window, cx);
                }
            }
        }
        cx.notify();
    }

    /// The projects the sidebar and pages list threads of.
    pub(crate) fn indexed_projects(&self) -> Vec<PathBuf> {
        self.recent
            .iter()
            .take(SIDEBAR_FOLDERS + 1)
            .cloned()
            .collect()
    }

    /// Re-reads the threads of the recent projects in the background.
    pub(crate) fn refresh_index(&mut self, cx: &mut Context<Self>) {
        let home = self.agent_home.clone();
        let projects = self.indexed_projects();
        let load = cx.background_spawn(async move { ThreadIndex::load(&home, &projects) });
        self.index_task = Some(cx.spawn(async move |this, cx| {
            let index = load.await;
            this.update(cx, |this, cx| {
                if this.index != index {
                    this.index = index;
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// The thread the current chat shows.
    pub(crate) fn current_thread_id(&self, cx: &App) -> Option<String> {
        self.chat().map(|chat| chat.read(cx).thread().id.clone())
    }
}
