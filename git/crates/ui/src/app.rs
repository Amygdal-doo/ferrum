//! Root view (`GitView`) and window layout (Tower style):
//! sidebar | list card | detail card; compact pane layout for hosts.

use crate::components::actions::Action;
use crate::components::dialogs::{self, DialogState};
use crate::components::graph::HistoryKey;
use crate::components::menu;
use crate::components::shell::{compact, layout, project};
use crate::embed::{self, GitViewEvent, GitViewOptions};
use crate::state::{AppState, Dialog, Page};
use ferrum_ui::components::v_flex;
use ferrum_ui::theme::ActiveTheme;
use ferrum_ui::{
    DraggedSplitter, Notice as UiNotice, ProjectBar, TextInput, TextInputEvent,
    ToggleProjectSwitcher, render_notice,
};
use ferrumgit_application::AppServices;
use ferrumgit_domain::RepoId;
use gpui::{
    App, AppContext as _, Context, DragMoveEvent, Entity, EventEmitter, FocusHandle, ListAlignment,
    ListState, Pixels, Point, Subscription, Task, Window, actions, canvas, prelude::*, px,
};
use std::cell::Cell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

actions!(ferrumgit, [QuickOpen, Reload, GoBack, GoForward]);

/// Key context of the view: FerrumGIT's bindings only apply inside it.
pub const KEY_CONTEXT: &str = "FerrumGit";
/// Key context of the list pickers (branch / repository choosers).
pub const PICKER_CONTEXT: &str = "FerrumGitPicker";

/// Text inputs that live as long as the window (their text mirrors `RepoView`).
pub struct Inputs {
    pub home_filter: Entity<TextInput>,
    pub ref_filter: Entity<TextInput>,
    pub search: Entity<TextInput>,
    pub summary: Entity<TextInput>,
    pub description: Entity<TextInput>,
    search_synced: Option<RepoId>,
    draft_synced: Option<(RepoId, u64)>,
}

pub struct GitView {
    pub state: AppState,
    pub options: GitViewOptions,
    pub services: AppServices,
    pub dialog: Option<DialogState>,
    pub dialog_subscriptions: Vec<Subscription>,
    /// Open context menu: position and its actions.
    pub menu: Option<(Point<Pixels>, Vec<Action>)>,
    /// Collapsed sidebar sections / folders.
    pub collapsed: HashSet<String>,
    pub inputs: Inputs,
    /// Shared Ferrum project strip above the toolbar.
    pub project_bar: Entity<ProjectBar>,
    pub history: ListState,
    pub history_key: Option<HistoryKey>,
    pub(crate) notice_task: Option<Task<()>>,
    pub(crate) resizing: bool,
    /// Compact layout: branches drawer open.
    pub(crate) drawer_open: bool,
    /// Page to show once the repository being opened is ready (see
    /// [`GitView::show_changes`]).
    pub(crate) pending_page: Option<Page>,
    /// Embedded on a folder outside any work tree: its changes since a
    /// snapshot and the repositories below it (see [`crate::folder`]).
    pub(crate) folder: Option<crate::folder::FolderMode>,
    /// Compact layout: list width (side by side) or height (stacked); not saved.
    pub(crate) compact_split: Option<f32>,
    /// The window's sidebar is shown (◧ in the title row).
    pub(crate) sidebar_open: bool,
    /// The card used last (lighter): 0 the list, 1 the details.
    pub(crate) active_pane: u8,
    /// Width the view was last laid out at (measured during paint).
    pub(crate) width: Rc<Cell<Option<Pixels>>>,
    /// The stand-alone app's file editor (see `components::editor::pane`).
    pub(crate) editor: Option<crate::components::editor::pane::OpenEditor>,
    /// A pull request review comment being written.
    pub(crate) review_draft: Option<crate::components::review::ReviewDraft>,
    pub(crate) review_subscription: Option<Subscription>,
    focus_handle: FocusHandle,
    _poll: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl GitView {
    /// Creates a view on `repo_path` (the repository containing it; `None`
    /// shows the repository list). Call [`crate::init`] once beforehand, or
    /// let the first view start the shared services. The view does not take
    /// focus: hosts focus it through [`gpui::Focusable`].
    pub fn new(
        repo_path: Option<PathBuf>,
        options: GitViewOptions,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let shared = embed::services(cx);
        cx.new(|cx| {
            let mut view = Self::build(shared.services, options, window, cx);
            if let Some(warning) = shared.startup_warning {
                view.notify(warning, true, cx);
            }
            if let Some(path) = repo_path {
                view.open_path(path, cx);
            }
            view
        })
    }

    /// Shows the repository containing `path` (no-op if it is already shown).
    /// Without repository tabs it replaces the current one, and a folder
    /// outside any work tree shows its changes and the repositories below.
    pub fn set_repo(&mut self, path: impl AsRef<Path>, cx: &mut Context<Self>) {
        let path = path.as_ref();
        if self.repo_path().is_some_and(|p| p == path)
            || self.folder.as_ref().is_some_and(|f| f.path == path)
        {
            return;
        }
        self.open_path(path.to_path_buf(), cx);
    }

    /// Work tree of the shown repository.
    #[must_use]
    pub fn repo_path(&self) -> Option<PathBuf> {
        self.state.active_entry().map(|e| PathBuf::from(e.path))
    }

    /// Changes the presentation (e.g. when the host pane is undocked).
    pub fn set_options(&mut self, options: GitViewOptions, cx: &mut Context<Self>) {
        self.options = options;
        cx.notify();
    }

    /// Asks the host to open `path` (relative to repository `id`) at `line`.
    pub(crate) fn open_in_host(
        &mut self,
        id: RepoId,
        path: &str,
        line: Option<u32>,
        cx: &mut Context<Self>,
    ) {
        if let Some(entry) = self.state.entry(id) {
            cx.emit(GitViewEvent::OpenFile {
                path: Path::new(&entry.path).join(path),
                line,
            });
        }
    }

    fn build(
        services: AppServices,
        options: GitViewOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = |placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| TextInput::new(window, cx).placeholder(placeholder))
        };
        let home_filter = input("Filter", window, cx);
        let ref_filter = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Filter branches & tags")
                .borderless()
        });
        let search = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Search for Message")
                .borderless()
        });
        let summary = input("Summary (required)", window, cx);
        let description = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Description")
                .multiline(4)
        });

        let (project_bar, project_events) = project::new_bar(window, cx);
        let subscriptions = vec![
            project_events,
            cx.subscribe(&home_filter, |_, _, _, cx| cx.notify()),
            cx.subscribe(&ref_filter, |_, _, _, cx| cx.notify()),
            cx.subscribe(&search, |app: &mut Self, input, event, cx| {
                let Some(id) = app.state.active else { return };
                match event {
                    TextInputEvent::Changed => {
                        let text = input.read(cx).text().to_owned();
                        if app.state.view(id).page != Page::History {
                            app.set_page(id, Page::History, cx);
                        }
                        app.search_changed(id, text, cx);
                        cx.notify();
                    }
                    // Enter jumps to the next match.
                    TextInputEvent::Submit => app.search_step(id, true, cx),
                    TextInputEvent::Blurred => {}
                }
            }),
            cx.subscribe(&summary, |app: &mut Self, input, event, cx| {
                let Some(id) = app.state.active else { return };
                match event {
                    TextInputEvent::Changed => {
                        let text = input.read(cx).text().to_owned();
                        app.state.update_view(id, |v| v.summary = text);
                        cx.notify();
                    }
                    TextInputEvent::Submit if app.can_commit(id) => app.commit(id, cx),
                    _ => {}
                }
            }),
            cx.subscribe(&description, |app: &mut Self, input, event, cx| {
                let Some(id) = app.state.active else { return };
                match event {
                    TextInputEvent::Changed => {
                        let text = input.read(cx).text().to_owned();
                        app.state.update_view(id, |v| v.description = text);
                    }
                    TextInputEvent::Submit if app.can_commit(id) => app.commit(id, cx),
                    _ => {}
                }
            }),
        ];

        let mut app = Self {
            state: AppState::default(),
            options,
            services,
            dialog: None,
            dialog_subscriptions: Vec::new(),
            menu: None,
            collapsed: HashSet::from(["section:Tags".to_owned()]),
            inputs: Inputs {
                home_filter,
                ref_filter,
                search,
                summary,
                description,
                search_synced: None,
                draft_synced: None,
            },
            project_bar,
            history: ListState::new(0, ListAlignment::Top, px(600.)),
            history_key: None,
            notice_task: None,
            resizing: false,
            drawer_open: false,
            pending_page: None,
            folder: None,
            compact_split: None,
            sidebar_open: true,
            active_pane: 0,
            width: Rc::default(),
            editor: None,
            review_draft: None,
            review_subscription: None,
            focus_handle: cx.focus_handle(),
            _poll: Self::start_polling(cx),
            _subscriptions: subscriptions,
        };
        app.bootstrap(cx);
        app
    }

    #[must_use]
    pub fn can_commit(&self, id: RepoId) -> bool {
        let view = self.state.view(id);
        let staged = view
            .data
            .as_ref()
            .map_or(0, |d| d.snapshot.status.staged().len());
        !self.state.is_busy(id) && !view.summary.trim().is_empty() && (staged > 0 || view.amend)
    }

    /// Copies the active repository's search and commit draft into the inputs
    /// when the repository changes or the draft was replaced programmatically.
    fn sync_inputs(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.state.active else {
            self.inputs.search_synced = None;
            return;
        };
        let view = self.state.view(id);
        if self.inputs.search_synced != Some(id) {
            self.inputs.search_synced = Some(id);
            self.inputs
                .search
                .update(cx, |input, cx| input.set_text(view.search.clone(), cx));
        }
        if self.inputs.draft_synced != Some((id, view.draft_rev)) {
            self.inputs.draft_synced = Some((id, view.draft_rev));
            self.inputs
                .summary
                .update(cx, |input, cx| input.set_text(view.summary.clone(), cx));
            self.inputs
                .description
                .update(cx, |input, cx| input.set_text(view.description.clone(), cx));
        }
    }

    pub fn focus_root(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle, cx);
    }

    pub fn open_menu(
        &mut self,
        position: Point<Pixels>,
        actions: Vec<Action>,
        cx: &mut Context<Self>,
    ) {
        self.menu = Some((position, actions));
        cx.notify();
    }

    fn on_cancel(
        &mut self,
        _: &ferrum_ui::menu::Cancel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu.take().is_some() {
        } else if self.dialog.is_some() {
            self.close_dialog(window, cx);
        } else if self.editor_shown() {
            self.close_editor(window, cx);
        } else if self.in_folder() {
            self.close_folder_diff(cx);
        } else if let Some(id) = self.state.active {
            // Esc closes the file panel first, then the conflict view, then the diff.
            self.state.update_view(id, |v| {
                if v.file.take().is_none() && v.conflict.take().is_none() {
                    v.diff = None;
                }
            });
        }
        cx.notify();
    }

    fn on_quick_open(&mut self, _: &QuickOpen, window: &mut Window, cx: &mut Context<Self>) {
        if !self.options.show_repo_tabs {
            // One repository: leave the shortcut to the host.
            cx.propagate();
        } else if self.dialog.is_none() {
            self.open_dialog(Dialog::QuickOpen, window, cx);
        }
    }

    fn on_reload(&mut self, _: &Reload, _: &mut Window, cx: &mut Context<Self>) {
        if self.in_folder() {
            self.reload_folder(cx);
        } else if self.dialog.is_none()
            && let Some(id) = self.state.active
        {
            self.load(id, cx);
        }
    }

    fn on_back(&mut self, _: &GoBack, _: &mut Window, cx: &mut Context<Self>) {
        if self.dialog.is_none()
            && let Some(id) = self.state.active
        {
            self.go(id, true, cx);
        }
    }

    fn on_forward(&mut self, _: &GoForward, _: &mut Window, cx: &mut Context<Self>) {
        if self.dialog.is_none()
            && let Some(id) = self.state.active
        {
            self.go(id, false, cx);
        }
    }

    /// The window layout shows the sidebar column (a repository is open
    /// and ◧ is on).
    pub(crate) fn sidebar_shown(&self) -> bool {
        !self.options.compact && self.sidebar_open && self.state.active.is_some()
    }

    pub(crate) fn on_resize(
        &mut self,
        event: &DragMoveEvent<DraggedSplitter>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let x = f64::from(f32::from(event.event.position.x - event.bounds.left()));
        let y = f32::from(event.event.position.y - event.bounds.top());
        let gap = f64::from(f32::from(ferrum_ui::PANE_GAP));
        let sidebar = match (self.options.compact, self.drawer_open) {
            (false, _) if self.sidebar_shown() => self.state.sidebar_width() + gap,
            (false, _) => gap,
            (true, true) => f64::from(layout::drawer_width(self.width.get())),
            (true, false) => 0.0,
        };
        match event.drag(cx).id.as_ref() {
            "sidebar" if !self.options.compact => {
                self.state.settings.sidebar_width = x.clamp(180.0, 480.0);
            }
            "detail" if self.options.compact => {
                let total = f32::from(event.bounds.size.width) - sidebar as f32;
                self.compact_split = Some((x as f32 - sidebar as f32).clamp(200.0, total - 200.0));
            }
            "detail" => {
                let width = x - sidebar;
                self.state.settings.detail_width = width.clamp(300.0, 900.0);
            }
            "stack" => {
                let total = f32::from(event.bounds.size.height);
                self.compact_split = Some(y.clamp(96.0, (total - 96.0).max(96.0)));
            }
            _ => return,
        }
        // Compact sizes are per view and not saved.
        self.resizing = !self.options.compact;
        cx.notify();
    }

    /// Records the laid-out width; re-renders when it crosses the
    /// side-by-side breakpoint.
    fn measure(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let cell = self.width.clone();
        let this = cx.entity().downgrade();
        canvas(
            move |bounds, window, _| {
                let width = bounds.size.width;
                let before = cell.replace(Some(width));
                if before.map(layout::is_wide) != Some(layout::is_wide(width)) {
                    window.on_next_frame(move |_, cx| {
                        if let Err(error) = this.update(cx, |_, cx| cx.notify()) {
                            tracing::debug!(error = %error, "view closed before relayout");
                        }
                    });
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full()
        .into_any_element()
    }
}

impl EventEmitter<GitViewEvent> for GitView {}

impl Render for GitView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_inputs(cx);
        if self.options.show_project_bar {
            self.sync_project_bar(cx);
        }
        let theme = cx.theme().clone();
        let colors = theme.colors();
        let notice = self.state.notice.as_ref().map(|n| {
            let notice = if n.error {
                UiNotice::error(n.message.clone())
            } else {
                UiNotice::info(n.message.clone())
            };
            render_notice(
                &notice,
                cx.listener(|app, _, _, cx| {
                    app.state.notice = None;
                    cx.notify();
                }),
                cx,
            )
        });
        let compact = self.options.compact;
        let in_folder = self.in_folder();
        let toolbar = (compact && !in_folder).then(|| compact::render(self, window, cx));
        let picked = crate::folder::picked_strip(self, cx);
        let body = if in_folder {
            crate::folder::render(self, cx)
        } else {
            layout::render_body(self, window, cx)
        };
        let sidebar = self
            .sidebar_shown()
            .then(|| layout::sidebar_column(self, window, cx));
        let dialog = dialogs::render(self, window, cx);
        let context_menu = menu::render(self, cx);
        let measure = self.measure(cx);

        v_flex()
            .id("ferrumgit")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(Self::on_quick_open))
            .on_action(cx.listener(Self::on_reload))
            .on_action(cx.listener(Self::on_back))
            .on_action(cx.listener(Self::on_forward))
            .on_action(cx.listener(Self::on_quit))
            .when(self.options.show_project_bar, |d| {
                d.on_action(cx.listener(|app, _: &ToggleProjectSwitcher, window, cx| {
                    app.project_bar
                        .update(cx, |bar, cx| bar.toggle_switcher(window, cx));
                }))
            })
            .relative()
            .size_full()
            .min_w(px(0.))
            .overflow_hidden()
            .when(compact, |d| d.bg(colors.background))
            .text_color(colors.text)
            .text_sm()
            .child(measure)
            .map(|d| {
                if compact {
                    d.children(picked).children(toolbar).child(body)
                } else {
                    // Window: sidebar on the backdrop, then the list and
                    // detail cards, each with its own header row.
                    d.child(layout::window_row(sidebar, body, cx))
                }
            })
            .children(dialog)
            .children(context_menu)
            .children(notice)
    }
}

impl gpui::Focusable for GitView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
