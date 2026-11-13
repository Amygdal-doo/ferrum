//! Root view and window layout (Tower style):
//! toolbar / sidebar | list | details.

use crate::components::actions::Action;
use crate::components::detail::{middle_column, right_column};
use crate::components::dialogs::{self, DialogState};
use crate::components::graph::HistoryKey;
use crate::components::shell::{project, toolbar};
use crate::components::{home, menu, sidebar};
use crate::state::{AppState, Dialog, Page};
use ferrum_ui::components::{Color, Label, LabelCommon, h_flex, v_flex};
use ferrum_ui::theme::ActiveTheme;
use ferrum_ui::{
    DraggedSplitter, Notice as UiNotice, ProjectBar, TextInput, TextInputEvent,
    ToggleProjectSwitcher, render_notice, splitter,
};
use ferrumgit_application::AppServices;
use ferrumgit_domain::RepoId;
use gpui::{
    App, Axis, Context, DragMoveEvent, Entity, FocusHandle, KeyBinding, ListAlignment, ListState,
    MouseButton, Pixels, Point, Subscription, Task, Window, actions, div, prelude::*, px,
};
use std::collections::HashSet;

actions!(ferrumgit, [QuickOpen, Reload, GoBack, GoForward]);

const KEY_CONTEXT: &str = "FerrumGit";

pub fn bind_keys(cx: &mut App) {
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("cmd-o", QuickOpen, context),
        KeyBinding::new("cmd-p", QuickOpen, context),
        KeyBinding::new("ctrl-o", QuickOpen, context),
        KeyBinding::new("ctrl-p", QuickOpen, context),
        KeyBinding::new("cmd-r", Reload, context),
        KeyBinding::new("ctrl-r", Reload, context),
        KeyBinding::new("cmd-[", GoBack, context),
        KeyBinding::new("cmd-]", GoForward, context),
        KeyBinding::new(
            "up",
            ferrum_ui::menu::SelectPrevious,
            Some("FerrumGitPicker"),
        ),
        KeyBinding::new("down", ferrum_ui::menu::SelectNext, Some("FerrumGitPicker")),
    ]);
}

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

pub struct FerrumGit {
    pub state: AppState,
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
    resizing: bool,
    focus_handle: FocusHandle,
    _poll: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl FerrumGit {
    pub fn new(services: AppServices, window: &mut Window, cx: &mut Context<Self>) -> Self {
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
            focus_handle: cx.focus_handle(),
            _poll: Self::start_polling(cx),
            _subscriptions: subscriptions,
        };
        window.focus(&app.focus_handle, cx);
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
        } else if let Some(id) = self.state.active {
            // Esc closes the file panel first, then the diff.
            self.state.update_view(id, |v| {
                if v.file.take().is_none() {
                    v.diff = None;
                }
            });
        }
        cx.notify();
    }

    fn on_quick_open(&mut self, _: &QuickOpen, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog.is_none() {
            self.open_dialog(Dialog::QuickOpen, window, cx);
        }
    }

    fn on_reload(&mut self, _: &Reload, _: &mut Window, cx: &mut Context<Self>) {
        if self.dialog.is_none()
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

    fn on_resize(
        &mut self,
        event: &DragMoveEvent<DraggedSplitter>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let x = f64::from(f32::from(event.event.position.x - event.bounds.left()));
        match event.drag(cx).id.as_ref() {
            "sidebar" => self.state.settings.sidebar_width = x.clamp(180.0, 480.0),
            "detail" => {
                let width = x - self.state.sidebar_width() - 4.0;
                self.state.settings.detail_width = width.clamp(300.0, 900.0);
            }
            _ => return,
        }
        self.resizing = true;
        cx.notify();
    }

    fn render_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        if !self.state.loaded {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(Label::new("Loading…").color(Color::Muted))
                .into_any_element();
        }
        if self.state.active.is_none() {
            return home::render(self, window, cx);
        }
        let sidebar_width = px(self.state.sidebar_width() as f32);
        let list_width = px(self.state.detail_width() as f32);
        h_flex()
            .id("ferrumgit-columns")
            .flex_1()
            .min_h_0()
            .items_stretch()
            .on_drag_move(cx.listener(Self::on_resize))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|app, _, _, cx| {
                    if std::mem::take(&mut app.resizing) {
                        app.save_settings(cx);
                    }
                }),
            )
            .child(
                div()
                    .flex_none()
                    .w(sidebar_width)
                    .h_full()
                    .overflow_hidden()
                    .child(sidebar::render(self, window, cx)),
            )
            .child(splitter("sidebar", Axis::Horizontal, cx))
            .child(
                div()
                    .flex_none()
                    .w(list_width)
                    .h_full()
                    .overflow_hidden()
                    .child(middle_column(self, window, cx)),
            )
            .child(splitter("detail", Axis::Horizontal, cx))
            .child(right_column(self, window, cx))
            .into_any_element()
    }
}

impl Render for FerrumGit {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_inputs(cx);
        self.sync_project_bar(cx);
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
        let body = self.render_body(window, cx);
        let dialog = dialogs::render(self, window, cx);
        let context_menu = menu::render(self, cx);

        v_flex()
            .id("ferrumgit")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(Self::on_quick_open))
            .on_action(cx.listener(Self::on_reload))
            .on_action(cx.listener(Self::on_back))
            .on_action(cx.listener(Self::on_forward))
            .on_action(cx.listener(|app, _: &ToggleProjectSwitcher, window, cx| {
                app.project_bar
                    .update(cx, |bar, cx| bar.toggle_switcher(window, cx));
            }))
            .relative()
            .size_full()
            .bg(colors.background)
            .text_color(colors.text)
            .text_sm()
            .child(self.project_bar.clone())
            .child(toolbar::render(self, window, cx))
            .child(body)
            .children(dialog)
            .children(context_menu)
            .children(notice)
    }
}

impl gpui::Focusable for FerrumGit {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
