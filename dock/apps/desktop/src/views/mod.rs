//! Rendering. Each section is a function of the root view's state.

pub mod chrome;
pub mod compose_rows;
pub mod containers;
pub mod detail;
pub mod dialogs;
pub mod engines;
pub mod image_dialogs;
pub mod images;
pub mod inspect;
pub mod logs;
pub mod resources;
pub mod run_dialog;
pub mod services;
pub mod settings_page;
pub mod stats;
pub mod widgets;

use crate::app::FerrumContainers;
use crate::state::{EngineStatus, Section};
use ferrum_ui::components::h_flex;
use ferrum_ui::{DraggedSplitter, gap_splitter};
use gpui::{
    AnyElement, App, Axis, Context, DragMoveEvent, MouseButton, Window, div, prelude::*, px,
};

/// Everything between the title row and the status line: the section in a
/// pane card (and the container detail in a second one).
pub fn body(
    app: &mut FerrumContainers,
    window: &mut Window,
    cx: &mut Context<FerrumContainers>,
) -> AnyElement {
    match &app.status {
        EngineStatus::Connecting => card(true, chrome::connecting(), cx),
        EngineStatus::Offline(error) => {
            let error = error.clone();
            let offline = chrome::offline(&error, cx);
            card(true, offline, cx)
        }
        EngineStatus::Connected(_) => section(app, window, cx),
    }
}

/// `content` in a pane card filling the space it is given.
fn card(active: bool, content: AnyElement, cx: &App) -> AnyElement {
    ferrum_ui::pane_card(active, cx)
        .size_full()
        .child(content)
        .into_any_element()
}

fn section(
    app: &mut FerrumContainers,
    window: &mut Window,
    cx: &mut Context<FerrumContainers>,
) -> AnyElement {
    match app.state.section {
        Section::Containers => {
            let list = containers::render(app, cx);
            if app.state.detail.is_none() {
                return card(true, list, cx);
            }
            let panel = detail::render(app, window, cx);
            let detail_active = app.detail_active;
            h_flex()
                .id("containers-split")
                .size_full()
                .items_stretch()
                .on_drag_move(
                    cx.listener(|app, event: &DragMoveEvent<DraggedSplitter>, _, cx| {
                        if event.drag(cx).id.as_ref() == "detail" {
                            let right = f32::from(event.bounds.right() - event.event.position.x);
                            app.detail_width = right.clamp(360., 1400.);
                            cx.notify();
                        }
                    }),
                )
                .child(
                    div()
                        .id("list-card")
                        .flex_1()
                        .min_w(px(320.))
                        .h_full()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|app, _, _, cx| {
                                app.set_detail_active(false, cx);
                            }),
                        )
                        .child(card(!detail_active, list, cx)),
                )
                .child(gap_splitter("detail", Axis::Horizontal, cx))
                .child(
                    div()
                        .id("detail-card")
                        .flex_none()
                        .w(px(app.detail_width))
                        .h_full()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|app, _, _, cx| {
                                app.set_detail_active(true, cx);
                            }),
                        )
                        .child(card(detail_active, panel, cx)),
                )
                .into_any_element()
        }
        Section::Images => card(true, images::render(app, cx), cx),
        Section::Volumes => card(true, resources::volumes(app, cx), cx),
        Section::Networks => card(true, resources::networks(app, cx), cx),
    }
}
