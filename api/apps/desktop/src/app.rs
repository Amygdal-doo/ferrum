//! Root view and window layout.

use crate::components::fields::FieldCache;
use crate::components::response::BodyDisplay;
use crate::state::AppState;
use ferrum_ui::components::{ActiveTheme as _, Color, Label, LabelCommon as _, h_flex, v_flex};
use ferrum_ui::{
    DraggedSplitter, Notice, ProjectBar, ToggleProjectSwitcher, gap_splitter, render_notice,
};
use ferrumapi_application::AppServices;
use gpui::{
    App, Axis, Context, DragMoveEvent, Entity, FocusHandle, Image, KeyBinding, ScrollHandle,
    SharedString, Task, Window, actions, div, prelude::*, px,
};
use std::sync::Arc;

actions!(
    ferrum_api,
    [
        SendRequest,
        SaveRequest,
        FindInResponse,
        FindNext,
        FindPrevious,
        ToggleSidebar
    ]
);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-enter", SendRequest, None),
        KeyBinding::new("ctrl-enter", SendRequest, None),
        KeyBinding::new("cmd-s", SaveRequest, None),
        KeyBinding::new("ctrl-s", SaveRequest, None),
        KeyBinding::new("cmd-f", FindInResponse, None),
        KeyBinding::new("ctrl-f", FindInResponse, None),
        KeyBinding::new("cmd-g", FindNext, None),
        KeyBinding::new("ctrl-g", FindNext, None),
        KeyBinding::new("cmd-shift-g", FindPrevious, None),
        KeyBinding::new("ctrl-shift-g", FindPrevious, None),
        KeyBinding::new("cmd-b", ToggleSidebar, None),
        KeyBinding::new("ctrl-b", ToggleSidebar, None),
    ]);
}

pub type ImageKey = (ferrumapi_domain::TabId, u64, usize);

const SIDEBAR_SPLITTER: &str = "sidebar";
const RESULTS_SPLITTER: &str = "results";

pub struct FerrumApi {
    pub services: AppServices,
    pub state: AppState,
    pub fields: FieldCache,
    /// Decoded image preview, keyed by tab and response identity.
    pub image_cache: Option<(ImageKey, Arc<Image>)>,
    /// Displayed response body text, tokens and search matches.
    pub body_display: Option<BodyDisplay>,
    /// Scroll position of the response body (for revealing search matches).
    pub response_scroll: ScrollHandle,
    /// Shared Ferrum project bar, inline under the sidebar's title row.
    pub project_bar: Entity<ProjectBar>,
    focus_handle: FocusHandle,
    /// Whether focus was already moved into the open dialog.
    pub dialog_focused: bool,
    notice: Option<Notice>,
    notice_task: Option<Task<()>>,
    _autosave: Task<()>,
    _project_events: gpui::Subscription,
}

impl FerrumApi {
    pub fn new(services: AppServices, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self::without_loading(services, window, cx);
        this.bootstrap(cx);
        this
    }

    /// The view before local data is loaded (tests render it headless
    /// without touching the services runtime).
    pub fn without_loading(
        services: AppServices,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        let (project_bar, project_events) = crate::components::shell::project::new_bar(window, cx);
        Self {
            services,
            state: AppState::new(),
            fields: FieldCache::default(),
            image_cache: None,
            body_display: None,
            response_scroll: ScrollHandle::new(),
            project_bar,
            focus_handle,
            dialog_focused: false,
            notice: None,
            notice_task: None,
            _autosave: Self::autosave_loop(cx),
            _project_events: project_events,
        }
    }

    pub fn root_focus(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    pub fn notify(
        &mut self,
        message: impl Into<SharedString>,
        error: bool,
        cx: &mut Context<Self>,
    ) {
        let message = message.into();
        self.notice = Some(if error {
            Notice::error(message)
        } else {
            Notice::info(message)
        });
        self.notice_task = Some(Notice::dismiss_later(cx, |this: &mut Self, _| {
            this.notice = None;
        }));
        cx.notify();
    }

    fn on_send(&mut self, _: &SendRequest, _: &mut Window, cx: &mut Context<Self>) {
        if self.state.dialog.is_none() {
            self.send_or_cancel_active(cx);
            cx.notify();
        }
    }

    fn on_save(&mut self, _: &SaveRequest, _: &mut Window, cx: &mut Context<Self>) {
        if self.state.dialog.is_none()
            && let Some(id) = self.state.active_tab
        {
            self.save_tab(id, cx);
            cx.notify();
        }
    }

    fn on_cancel(
        &mut self,
        _: &ferrum_ui::menu::Cancel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.dialog.take().is_none() {
            if self.state.renaming.is_none()
                && self.state.renaming_tab.is_none()
                && self.state.response_search.take().is_some()
            {
                window.focus(&self.focus_handle, cx);
            }
            self.state.renaming = None;
            self.state.renaming_tab = None;
        }
        cx.notify();
    }

    fn on_find(&mut self, _: &FindInResponse, _: &mut Window, cx: &mut Context<Self>) {
        if self.state.dialog.is_none() {
            self.open_response_search();
            cx.notify();
        }
    }

    fn on_find_next(&mut self, _: &FindNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step_response_search(1);
        cx.notify();
    }

    fn on_find_previous(&mut self, _: &FindPrevious, _: &mut Window, cx: &mut Context<Self>) {
        self.step_response_search(-1);
        cx.notify();
    }

    fn on_drag_splitter(
        &mut self,
        event: &DragMoveEvent<DraggedSplitter>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let position = event.event.position;
        match event.drag(cx).id.as_ref() {
            SIDEBAR_SPLITTER => {
                let width = f64::from(position.x - event.bounds.left());
                self.state.settings.sidebar_width = width.clamp(200.0, 560.0);
            }
            RESULTS_SPLITTER => {
                let height = f64::from(event.bounds.bottom() - position.y);
                self.state.results_height = height.clamp(120.0, 900.0);
            }
            _ => return,
        }
        cx.notify();
    }
}

impl Render for FerrumApi {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.fields.begin();
        self.sync_project_bar(cx);
        let colors = cx.theme().colors().clone();
        let gap = ferrum_ui::PANE_GAP;
        let sidebar_open = self.state.sidebar_open;
        let content = if self.state.loaded {
            let sidebar = sidebar_open.then(|| self.render_sidebar(window, cx));
            let request = self.render_request_pane(window, cx);
            let response = self.render_response_panel(window, cx);
            let response_active = self.state.response_active;
            let request_card = ferrum_ui::pane_card(!response_active, cx)
                .flex_1()
                .w_full()
                .capture_any_mouse_down(cx.listener(|this, _, _, cx| {
                    if this.state.response_active {
                        this.state.response_active = false;
                        cx.notify();
                    }
                }))
                .child(request);
            let response_card = ferrum_ui::pane_card(response_active, cx)
                .flex_none()
                .w_full()
                .h(px(self.state.results_height as f32))
                .capture_any_mouse_down(cx.listener(|this, _, _, cx| {
                    if !this.state.response_active {
                        this.state.response_active = true;
                        cx.notify();
                    }
                }))
                .child(response);
            h_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .on_drag_move(cx.listener(Self::on_drag_splitter))
                .on_drop(cx.listener(|this, splitter: &DraggedSplitter, _, cx| {
                    if splitter.id.as_ref() == SIDEBAR_SPLITTER {
                        this.save_settings(cx);
                    }
                }))
                .when_some(sidebar, |row, sidebar| {
                    row.child(
                        div()
                            .flex_none()
                            .h_full()
                            .w(px(self.state.sidebar_width() as f32))
                            .overflow_hidden()
                            .child(sidebar),
                    )
                    .child(gap_splitter(
                        SIDEBAR_SPLITTER,
                        Axis::Horizontal,
                        cx,
                    ))
                })
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .pt(gap)
                        .pr(gap)
                        .when(!sidebar_open, |main| main.pl(gap))
                        .child(request_card)
                        .child(gap_splitter(RESULTS_SPLITTER, Axis::Vertical, cx))
                        .child(response_card),
                )
                .into_any_element()
        } else {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(Label::new("Loading workspace…").color(Color::Muted))
                .into_any_element()
        };
        let status = self.render_status_bar(cx);
        let dialog = self.render_dialog(window, cx);
        let notice = self.notice.as_ref().map(|notice| {
            render_notice(
                notice,
                cx.listener(|this, _, _, cx| {
                    this.notice = None;
                    cx.notify();
                }),
                cx,
            )
        });
        self.fields.sweep();

        v_flex()
            .key_context("FerrumApi")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_send))
            .on_action(cx.listener(Self::on_save))
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(Self::on_find))
            .on_action(cx.listener(Self::on_find_next))
            .on_action(cx.listener(Self::on_find_previous))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| this.toggle_sidebar(cx)))
            .on_action(cx.listener(|this, _: &ToggleProjectSwitcher, window, cx| {
                this.project_bar
                    .update(cx, |bar, cx| bar.toggle_switcher(window, cx));
            }))
            .relative()
            .size_full()
            .text_color(colors.text)
            .child(content)
            .child(status)
            .children(dialog)
            .children(notice)
    }
}
