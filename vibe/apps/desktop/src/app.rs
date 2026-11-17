//! The root view's state: the project, its chat (one per project, kept
//! alive), the FerrumGIT and FerrumTerm panes, the sidebar and the page
//! shown in the main card. Rendering lives in `chrome`, `sidebar`,
//! `columns` and `pages`; navigation in `navigate`; the views in `panes`
//! and the Files pane in `files`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use ferrum_ui::prelude::*;
use ferrum_ui::project;
use ferrumagent_ui::{ChatOptions, ChatView};
use ferrumgit_ui::{GitView, RepoSummary};
use ferrumterm_ui::TermView;

use crate::layout::{self, Column, Layout};
use crate::nav::{History, Page, UiState};
use crate::pages::PageState;
use crate::threads::ThreadIndex;

actions!(
    ferrum_vibe,
    [
        ToggleFiles,
        ToggleGit,
        ToggleTerm,
        ToggleEditor,
        ReviewChanges,
        MovePaneLeft,
        MovePaneRight,
        OpenFolder,
        ToggleSidebar,
        NewTask,
        NewDiscussion,
        GoBack,
        GoForward,
        ShowSearch
    ]
);

pub const KEY_CONTEXT: &str = "FerrumEngineer";
/// This app's binary, recorded in `~/.ferrum/projects.json`.
pub const BINARY: &str = "ferrumvibe";
/// No pane gets narrower than this while resizing.
pub const MIN_COLUMN_WIDTH: f32 = 260.;
/// Folders listed in the sidebar besides the current project.
pub const SIDEBAR_FOLDERS: usize = 6;

pub fn bind_keys(cx: &mut App) {
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("cmd-alt-1", ToggleFiles, None),
        KeyBinding::new("cmd-alt-2", ToggleGit, None),
        KeyBinding::new("cmd-alt-3", ToggleTerm, None),
        KeyBinding::new("cmd-alt-4", ToggleEditor, None),
        // The chat's own ⌥⌘R wins while it has focus.
        KeyBinding::new("cmd-alt-r", ReviewChanges, context),
        KeyBinding::new("cmd-alt-shift-left", MovePaneLeft, context),
        KeyBinding::new("cmd-alt-shift-right", MovePaneRight, context),
        KeyBinding::new("cmd-\\", ToggleSidebar, context),
        KeyBinding::new("cmd-b", ToggleSidebar, context),
        KeyBinding::new("cmd-n", NewTask, context),
        KeyBinding::new("cmd-shift-n", NewDiscussion, context),
        KeyBinding::new("cmd-[", GoBack, context),
        KeyBinding::new("cmd-]", GoForward, context),
        KeyBinding::new("cmd-shift-f", ShowSearch, context),
        KeyBinding::new("cmd-o", OpenFolder, context),
    ]);
    crate::files::bind_keys(cx);
}

/// The project FerrumEngineer had open last (from `~/.ferrum/projects.json`).
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
    /// `~/.ferrum/vibe`: the sidebar state, sites and automations
    /// (`None`: nothing saved).
    pub data_dir: Option<PathBuf>,
    /// FerrumChat's home (threads, `mcp.json`).
    pub agent_home: PathBuf,
    /// How each chat starts, given its project root.
    pub chat_options: ChatOptionsFn,
    /// Terminals in the shared `fterm` server (else a private one).
    pub term_attach_to_server: bool,
    /// Opens a file in FerrumIDE (the Editor column's Open in FerrumIDE).
    pub open_file: OpenFileFn,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            project: None,
            layouts_file: layout::default_file(),
            projects_file: project::projects_file(),
            data_dir: ferrum_ui::prefs::path("vibe"),
            agent_home: ferrumagent_ui::core::paths::agent_home(),
            chat_options: Rc::new(|_| ChatOptions::default()),
            term_attach_to_server: true,
            open_file: Rc::new(project::open_file_in_editor),
        }
    }
}

/// Something for a project's chat to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatAction {
    OpenThread(String),
    StartTask(ferrumagent_ui::core::Mode, Option<String>),
    Settings,
}

/// A project's chat, kept while the window lives so its agent keeps
/// working when another project is shown.
pub struct ChatSlot {
    pub view: Entity<ChatView>,
    pub(crate) _subscriptions: Vec<Subscription>,
}

pub struct FerrumEngineer {
    /// Hidden: hosts the ⌥⌘O switcher and the folder dialog.
    pub(crate) project_bar: Entity<ProjectBar>,
    pub(crate) project: Option<PathBuf>,
    pub(crate) projects_file: Option<PathBuf>,
    /// Recent project folders, newest first (the current one included).
    pub(crate) recent: Vec<PathBuf>,
    pub(crate) layout: Layout,
    /// The pane with focus, or used last: its card is lighter.
    pub(crate) active_column: Column,
    layouts_file: Option<PathBuf>,
    pub(crate) chats: HashMap<PathBuf, ChatSlot>,
    pub(crate) git: Option<Entity<GitView>>,
    pub(crate) term: Option<Entity<TermView>>,
    /// The Files pane, created when first shown with a project open.
    pub(crate) files: Option<Entity<crate::files::FilesView>>,
    /// The Editor column, created when a file is first opened or it is shown.
    pub(crate) editor: Option<Entity<crate::editor::EditorView>>,
    pub(crate) editor_subscription: Option<Subscription>,
    pub(crate) files_subscription: Option<Subscription>,
    /// The project's head, mirrored into the chat's footer.
    pub(crate) git_summary: Option<RepoSummary>,
    /// Files the chat's agent changed that are still to review.
    pub(crate) pending_changes: usize,
    /// Times the git pane was asked to reload (the chat changed files).
    pub(crate) git_refreshes: usize,
    pub(crate) notice: Option<Notice>,
    notice_task: Option<Task<()>>,
    save_task: Option<Task<()>>,
    pub(crate) chat_options: ChatOptionsFn,
    pub(crate) open_file: OpenFileFn,
    pub(crate) term_attach: bool,
    pub(crate) agent_home: PathBuf,
    pub(crate) data_dir: Option<PathBuf>,
    pub(crate) ui: UiState,
    pub(crate) resizing_sidebar: bool,
    /// The chat whose header holds our controls, and whether they include
    /// the window controls (sidebar folded).
    pub(crate) header_slots_for: Option<(gpui::EntityId, bool)>,
    pub(crate) history: History,
    pub(crate) index: ThreadIndex,
    pub(crate) index_task: Option<Task<()>>,
    pub(crate) pages: PageState,
    focus_handle: FocusHandle,
    pub(crate) git_subscriptions: Vec<Subscription>,
    _tasks: Vec<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl FerrumEngineer {
    pub fn new(options: Options, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let projects_file = options.projects_file.clone();
        let project_bar = cx.new(|cx| {
            ProjectBar::new(BINARY, window, cx)
                .projects_file(projects_file)
                .inline(true)
        });
        let subscription = cx.subscribe_in(&project_bar, window, |this, _, event, window, cx| {
            let ProjectBarEvent::Open(path) = event;
            this.open_project(path.clone(), window, cx);
        });
        let layout = match &options.layouts_file {
            Some(file) => layout::load(file, options.project.as_deref()),
            None => Layout::default(),
        };
        let ui = options
            .data_dir
            .as_ref()
            .map(|dir| UiState::load(&dir.join("ui.json")))
            .unwrap_or_default();
        let pages = PageState::new(options.data_dir.as_deref(), window, cx);
        let mut app = Self {
            project_bar,
            project: None,
            projects_file: options.projects_file,
            recent: Vec::new(),
            layout,
            active_column: Column::Chat,
            layouts_file: options.layouts_file,
            chats: HashMap::new(),
            git: None,
            term: None,
            files: None,
            files_subscription: None,
            editor: None,
            editor_subscription: None,
            git_summary: None,
            pending_changes: 0,
            git_refreshes: 0,
            notice: None,
            notice_task: None,
            save_task: None,
            chat_options: options.chat_options,
            open_file: options.open_file,
            term_attach: options.term_attach_to_server,
            agent_home: options.agent_home,
            data_dir: options.data_dir,
            ui,
            resizing_sidebar: false,
            header_slots_for: None,
            history: History::default(),
            index: ThreadIndex::default(),
            index_task: None,
            pages,
            focus_handle: cx.focus_handle(),
            git_subscriptions: Vec::new(),
            _tasks: vec![crate::pages::automations::start_timer(window, cx)],
            _subscriptions: vec![subscription],
        };
        if let Some(path) = options.project {
            app.project = Some(path.clone());
            app.project_bar
                .update(cx, |bar, cx| bar.set_project(Some(path), cx));
        }
        app.reload_recent();
        app.ensure_views(window, cx);
        app.refresh_index(cx);
        window.focus(&app.focus_handle, cx);
        app
    }

    /// Re-reads the recent projects (the current one first).
    pub(crate) fn reload_recent(&mut self) {
        let mut recent: Vec<PathBuf> = self
            .projects_file
            .as_deref()
            .map(project::load)
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.path)
            .filter(|p| p.is_dir())
            .collect();
        if let Some(current) = &self.project {
            recent.retain(|p| p != current);
            recent.insert(0, current.clone());
        }
        self.recent = recent;
    }

    /// Switches to `path`: its own chat (started on first use), the git
    /// pane on its repository, new terminals opening in it.
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
        self.pending_changes = 0;
        self.git_summary = None;
        if let Some(git) = &self.git {
            git.update(cx, |git, cx| git.set_repo(&path, cx));
        }
        if let Some(term) = &self.term {
            term.update(cx, |term, cx| term.set_cwd(path.clone(), cx));
        }
        if let Some(files) = &self.files {
            files.update(cx, |files, cx| files.set_root(path.clone(), cx));
        }
        self.reload_recent();
        self.ensure_views(window, cx);
        self.pages.project_changed(&path, cx);
        self.refresh_index(cx);
        cx.notify();
    }

    /// The current project's chat.
    #[must_use]
    pub fn chat(&self) -> Option<Entity<ChatView>> {
        let project = self.project.as_ref()?;
        self.chats.get(project).map(|slot| slot.view.clone())
    }

    /// Shows or hides the Git or Term pane (creating its view on first
    /// show); the chat stays.
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
            if visible {
                self.active_column = column;
            } else {
                // Keys keep working: focus left inside a hidden pane
                // reaches no handler.
                let focused = self
                    .column_focus(column, cx)
                    .is_some_and(|h| h.contains_focused(window, cx));
                if focused {
                    self.focus_chat(window, cx);
                }
                if self.active_column == column {
                    self.active_column = Column::Chat;
                }
            }
            self.save_layout(cx);
            cx.notify();
        }
    }

    /// Shows or hides the Files pane.
    pub fn set_files_visible(
        &mut self,
        visible: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.layout.files.visible == visible {
            return;
        }
        self.layout.files.visible = visible;
        self.ensure_views(window, cx);
        if let Some(files) = self.files.clone() {
            if visible {
                // Catch up on changes made while it was hidden.
                files.update(cx, |files, cx| files.refresh(cx));
            } else if files.focus_handle(cx).contains_focused(window, cx) {
                self.focus_chat(window, cx);
            }
        }
        self.save_layout(cx);
        cx.notify();
    }

    /// Drag of the splitter right of the Files pane, `width` px from its
    /// left edge.
    pub fn resize_files(&mut self, width: f32, cx: &mut Context<Self>) {
        if self.layout.files.resize(width) {
            self.save_layout_later(cx);
            cx.notify();
        }
    }

    /// Files and folders dropped on the chat (from the Files pane or the
    /// Finder) become context chips in its composer.
    pub fn add_context_paths(
        &mut self,
        paths: &[PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let paths = crate::files::without_nested(paths);
        if paths.is_empty() {
            return;
        }
        let Some(chat) = self.chat() else {
            self.show_notice(Notice::error("Open a project folder first."), cx);
            return;
        };
        chat.update(cx, |chat, cx| chat.add_context_paths(paths, cx));
        self.focus_chat(window, cx);
        cx.notify();
    }

    /// Moves `column` one place left (`-1`) or right (`1`) among the shown
    /// panes (the chat stays first).
    pub fn move_column(&mut self, column: Column, by: isize, cx: &mut Context<Self>) {
        if self.layout.move_column(column, by) {
            self.save_layout(cx);
            cx.notify();
        }
    }

    pub fn toggle(&mut self, column: Column, window: &mut Window, cx: &mut Context<Self>) {
        let visible = !self.layout.is_visible(column);
        self.set_visible(column, visible, window, cx);
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
            tracing::warn!(%error, "could not save the pane layout");
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

    pub(crate) fn save_ui(&self) {
        if let Some(dir) = &self.data_dir
            && let Err(error) = ferrum_ui::prefs::write(&dir.join("ui.json"), &self.ui)
        {
            tracing::warn!(%error, "could not save the sidebar state");
        }
    }

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.ui.sidebar_open = !self.ui.sidebar_open;
        self.save_ui();
        cx.notify();
    }

    pub(crate) fn show_notice(&mut self, notice: Notice, cx: &mut Context<Self>) {
        self.notice = Some(notice);
        self.notice_task = Some(Notice::dismiss_later(cx, |this: &mut Self, _| {
            this.notice = None;
        }));
        cx.notify();
    }

    /// The focus handle of `column`'s view, once created.
    fn column_focus(&self, column: Column, cx: &App) -> Option<FocusHandle> {
        match column {
            Column::Chat => self.chat().map(|v| v.focus_handle(cx)),
            Column::Git => self.git.as_ref().map(|v| v.focus_handle(cx)),
            Column::Term => self.term.as_ref().map(|v| v.focus_handle(cx)),
            // The view itself: the active tab's editor is inside it.
            Column::Editor => self
                .editor
                .as_ref()
                .map(|v| v.read(cx).focus_handle.clone()),
        }
    }

    /// Marks the pane whose view has focus as the active one (it stays
    /// active when focus leaves the panes).
    pub(crate) fn follow_focus(&mut self, window: &Window, cx: &App) {
        if let Some(column) = Column::ALL.into_iter().find(|column| {
            self.layout.is_visible(*column)
                && self
                    .column_focus(*column, cx)
                    .is_some_and(|h| h.contains_focused(window, cx))
        }) {
            self.active_column = column;
        }
    }

    /// The page in the main card (the chat when none).
    #[must_use]
    pub fn page(&self) -> Page {
        match &self.history.current {
            crate::nav::Location::Page(page) => *page,
            crate::nav::Location::Thread { .. } => Page::Chat,
        }
    }

    pub(crate) fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }
}

impl Focusable for FerrumEngineer {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Drop for FerrumEngineer {
    fn drop(&mut self) {
        // A resize still waiting to be saved.
        if self.save_task.take().is_some()
            && let Some(file) = &self.layouts_file
        {
            layout::save(file, self.project.as_deref(), &self.layout).ok();
        }
    }
}
