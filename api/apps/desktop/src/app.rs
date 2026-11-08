//! Root view and window layout.

use crate::components::fields::FieldCache;
use crate::state::AppState;
use ferrum_ui::{DraggedSplitter, Notice, render_notice, splitter};
use ferrumapi_application::AppServices;
use gpui::{
    App, Axis, Context, DragMoveEvent, FocusHandle, Image, KeyBinding, SharedString, Task, Window,
    actions, div, prelude::*, px,
};
use std::sync::Arc;
use ui::{ActiveTheme as _, Color, Label, LabelCommon as _, h_flex, v_flex};

actions!(ferrum_api, [SendRequest, SaveRequest]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-enter", SendRequest, None),
        KeyBinding::new("ctrl-enter", SendRequest, None),
        KeyBinding::new("cmd-s", SaveRequest, None),
        KeyBinding::new("ctrl-s", SaveRequest, None),
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
    focus_handle: FocusHandle,
    /// Whether focus was already moved into the open dialog.
    pub dialog_focused: bool,
    notice: Option<Notice>,
    notice_task: Option<Task<()>>,
    _autosave: Task<()>,
}

impl FerrumApi {
    pub fn new(services: AppServices, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        let mut this = Self {
            services,
            state: AppState::new(),
            fields: FieldCache::default(),
            image_cache: None,
            focus_handle,
            dialog_focused: false,
            notice: None,
            notice_task: None,
            _autosave: Self::autosave_loop(cx),
        };
        this.bootstrap(cx);
        this
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

    fn on_cancel(&mut self, _: &menu::Cancel, _: &mut Window, cx: &mut Context<Self>) {
        if self.state.dialog.take().is_none() {
            self.state.renaming = None;
            self.state.renaming_tab = None;
        }
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
        let colors = cx.theme().colors().clone();
        let header = self.render_header(window, cx);
        let content = if self.state.loaded {
            let sidebar = self.render_sidebar(window, cx);
            let request = self.render_request_pane(window, cx);
            let response = self.render_response_panel(window, cx);
            h_flex()
                .flex_1()
                .min_h_0()
                .items_start()
                .on_drag_move(cx.listener(Self::on_drag_splitter))
                .on_drop(cx.listener(|this, splitter: &DraggedSplitter, _, cx| {
                    if splitter.id.as_ref() == SIDEBAR_SPLITTER {
                        this.save_settings(cx);
                    }
                }))
                .child(
                    div()
                        .flex_none()
                        .h_full()
                        .w(px(self.state.sidebar_width() as f32))
                        .overflow_hidden()
                        .child(sidebar),
                )
                .child(splitter(SIDEBAR_SPLITTER, Axis::Horizontal, cx))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .child(div().flex_1().min_h_0().child(request))
                        .child(splitter(RESULTS_SPLITTER, Axis::Vertical, cx))
                        .child(
                            div()
                                .flex_none()
                                .h(px(self.state.results_height as f32))
                                .child(response),
                        ),
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
            .relative()
            .size_full()
            .bg(colors.background)
            .text_color(colors.text)
            .child(header)
            .child(content)
            .child(status)
            .children(dialog)
            .children(notice)
    }
}
