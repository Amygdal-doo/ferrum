//! The root view: title row (project, column toggles), the three columns, and
//! the wiring between them (chat → terminal, chat → git, files → Editor).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use ferrum_ui::prelude::*;
use ferrum_ui::project;
use ferrumagent_ui::{ChatOptions, ChatView};
use ferrumgit_ui::GitView;
use ferrumterm_ui::TermView;

use crate::layout::{self, Column, Layout, Side};

actions!(ferrum_agentic, [ToggleChat, ToggleGit, ToggleTerm]);

pub const KEY_CONTEXT: &str = "FerrumAgentic";
/// This app's binary, recorded in `~/.ferrum/projects.json`.
pub const BINARY: &str = "ferrumagentic";
/// No column gets narrower than this while resizing.
pub const MIN_COLUMN_WIDTH: f32 = 260.;

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-alt-1", ToggleChat, None),
        KeyBinding::new("cmd-alt-2", ToggleGit, None),
        KeyBinding::new("cmd-alt-3", ToggleTerm, None),
    ]);
}

/// The project FerrumAgentic had open last (from `~/.ferrum/projects.json`).
#[must_use]
pub fn last_project() -> Option<PathBuf> {
    let file = project::projects_file()?;
    project::load(&file)
        .into_iter()
        .find(|p| p.apps.iter().any(|a| a == BINARY) && p.path.is_dir())
        .map(|p| p.path)
}

pub type ChatOptionsFn = Rc<dyn Fn(&Path) -> ChatOptions>;
pub type OpenFileFn = Rc<dyn Fn(Option<&Path>, &Path, Option<u32>) -> Result<(), String>>;

pub struct Options {
    /// The project at start.
    pub project: Option<PathBuf>,
    /// Where layouts are saved (`None`: not saved).
    pub layouts_file: Option<PathBuf>,
    /// The recent-projects list (`None`: not recorded).
    pub projects_file: Option<PathBuf>,
    /// How each chat starts, given its project root.
    pub chat_options: ChatOptionsFn,
    /// Terminals in the shared `fterm` server (else a private one).
    pub term_attach_to_server: bool,
    /// Opens a file from the chat or git column.
    pub open_file: OpenFileFn,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            project: None,
            layouts_file: layout::default_file(),
            projects_file: project::projects_file(),
            chat_options: Rc::new(|_| ChatOptions::default()),
            term_attach_to_server: true,
            open_file: Rc::new(project::open_file_in_editor),
        }
    }
}

pub struct FerrumAgentic {
    pub(crate) project_bar: Entity<ProjectBar>,
    pub(crate) project: Option<PathBuf>,
    pub(crate) layout: Layout,
    /// The column with focus, or used last: its card is lighter.
    pub(crate) active_column: Column,
    layouts_file: Option<PathBuf>,
    pub(crate) chat: Option<Entity<ChatView>>,
    pub(crate) git: Option<Entity<GitView>>,
    pub(crate) term: Option<Entity<TermView>>,
    /// Files the chat's agent changed that are still to review.
    pub(crate) pending_changes: usize,
    pub(crate) term_title: Option<SharedString>,
    /// Times the git column was asked to reload (the chat changed files).
    pub(crate) git_refreshes: usize,
    pub(crate) notice: Option<Notice>,
    notice_task: Option<Task<()>>,
    save_task: Option<Task<()>>,
    pub(crate) chat_options: ChatOptionsFn,
    pub(crate) open_file: OpenFileFn,
    pub(crate) term_attach: bool,
    focus_handle: FocusHandle,
    pub(crate) view_subscriptions: HashMap<Column, Subscription>,
    _subscriptions: Vec<Subscription>,
}

impl FerrumAgentic {
    pub fn new(options: Options, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let projects_file = options.projects_file.clone();
        let project_bar = cx.new(|cx| {
            ProjectBar::new(BINARY, window, cx)
                .projects_file(projects_file)
                .inline(true)
        });
        let subscription = cx.subscribe_in(&project_bar, window, |this, _, event, window, cx| {
            let ProjectBarEvent::Open(path) = event;
            this.set_project(path.clone(), window, cx);
        });
        let layout = match &options.layouts_file {
            Some(file) => layout::load(file, options.project.as_deref()),
            None => Layout::default(),
        };
        let mut app = Self {
            project_bar,
            project: None,
            layout,
            active_column: Column::Chat,
            layouts_file: options.layouts_file,
            chat: None,
            git: None,
            term: None,
            pending_changes: 0,
            term_title: None,
            git_refreshes: 0,
            notice: None,
            notice_task: None,
            save_task: None,
            chat_options: options.chat_options,
            open_file: options.open_file,
            term_attach: options.term_attach_to_server,
            focus_handle: cx.focus_handle(),
            view_subscriptions: HashMap::new(),
            _subscriptions: vec![subscription],
        };
        if let Some(path) = options.project {
            app.project = Some(path.clone());
            app.project_bar
                .update(cx, |bar, cx| bar.set_project(Some(path), cx));
        }
        app.ensure_views(window, cx);
        window.focus(&app.focus_handle, cx);
        app
    }

    /// Switches every column to `path`: a new chat rooted there, the git
    /// column on its repository, new terminals opening in it.
    pub fn set_project(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.project.as_deref() == Some(path.as_path()) {
            return;
        }
        self.save_layout(cx);
        self.project = Some(path.clone());
        self.project_bar
            .update(cx, |bar, cx| bar.set_project(Some(path.clone()), cx));
        if let Some(file) = &self.layouts_file {
            self.layout = layout::load(file, Some(&path));
        }
        // The chat's session is tied to its root: start a new one.
        self.chat = None;
        self.view_subscriptions.remove(&Column::Chat);
        self.pending_changes = 0;
        if let Some(git) = &self.git {
            git.update(cx, |git, cx| git.set_repo(&path, cx));
        }
        if let Some(term) = &self.term {
            term.update(cx, |term, cx| term.set_cwd(path.clone(), cx));
        }
        self.ensure_views(window, cx);
        cx.notify();
    }

    /// Shows or hides a column (creating its view on first show).
    pub fn set_visible(
        &mut self,
        column: Column,
        visible: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.layout.set_visible(column, visible) {
            self.ensure_views(window, cx);
            if visible && column == Column::Git {
                // Catch up on changes made while it was hidden.
                cx.on_next_frame(window, |this, window, cx| this.refresh_git(window, cx));
            }
            self.save_layout(cx);
            cx.notify();
        }
    }

    pub fn toggle(&mut self, column: Column, window: &mut Window, cx: &mut Context<Self>) {
        let visible = !self.layout.is_visible(column);
        self.set_visible(column, visible, window, cx);
    }

    /// "Move left" / "Move right" in a column's menu.
    pub fn move_column(&mut self, column: Column, side: Side, cx: &mut Context<Self>) {
        if self.layout.move_to_side(column, side) {
            self.save_layout(cx);
            cx.notify();
        }
    }

    /// A column header dropped on `target`.
    pub fn drop_column(&mut self, column: Column, target: Column, cx: &mut Context<Self>) {
        if self.layout.move_onto(column, target) {
            self.save_layout(cx);
            cx.notify();
        }
    }

    /// Splitter drag: the border right of `column` at `x` px of `width`.
    pub fn resize(&mut self, column: Column, x: f32, width: f32, cx: &mut Context<Self>) {
        if width <= 0. {
            return;
        }
        if self
            .layout
            .resize(column, x / width, MIN_COLUMN_WIDTH / width)
        {
            self.save_layout_later(cx);
            cx.notify();
        }
    }

    pub(crate) fn save_layout(&mut self, _cx: &mut Context<Self>) {
        self.save_task = None;
        let Some(file) = &self.layouts_file else {
            return;
        };
        if let Err(error) = layout::save(file, self.project.as_deref(), &self.layout) {
            tracing::warn!(%error, "could not save the column layout");
        }
    }

    /// Saves once a drag has paused, not on every mouse move.
    fn save_layout_later(&mut self, cx: &mut Context<Self>) {
        self.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            this.update(cx, |this, cx| this.save_layout(cx)).ok();
        }));
    }

    pub(crate) fn show_notice(&mut self, notice: Notice, cx: &mut Context<Self>) {
        self.notice = Some(notice);
        self.notice_task = Some(Notice::dismiss_later(cx, |this: &mut Self, _| {
            this.notice = None;
        }));
        cx.notify();
    }

    pub(crate) fn on_toggle(
        &mut self,
        column: Column,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle(column, window, cx);
    }

    /// The title row's +: a new thread in the chat column (shown first).
    pub fn new_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_visible(Column::Chat, true, window, cx);
        if let Some(chat) = &self.chat {
            let handle = chat.focus_handle(cx);
            window.focus(&handle, cx);
            handle.dispatch_action(&ferrumagent_ui::NewThread, window, cx);
            self.active_column = Column::Chat;
            cx.notify();
        }
    }

    /// Marks the column whose view has focus as the active one (it stays
    /// active when focus leaves the columns).
    pub(crate) fn follow_focus(&mut self, window: &Window, cx: &App) {
        let views = [
            (Column::Chat, self.chat.as_ref().map(|v| v.focus_handle(cx))),
            (Column::Git, self.git.as_ref().map(|v| v.focus_handle(cx))),
            (Column::Term, self.term.as_ref().map(|v| v.focus_handle(cx))),
        ];
        if let Some((column, _)) = views.into_iter().find(|(column, handle)| {
            self.layout.is_visible(*column)
                && handle
                    .as_ref()
                    .is_some_and(|h| h.contains_focused(window, cx))
        }) {
            self.active_column = column;
        }
    }

    pub(crate) fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }
}

impl Focusable for FerrumAgentic {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Drop for FerrumAgentic {
    fn drop(&mut self) {
        // A resize still waiting to be saved.
        if self.save_task.take().is_some()
            && let Some(file) = &self.layouts_file
        {
            layout::save(file, self.project.as_deref(), &self.layout).ok();
        }
    }
}
