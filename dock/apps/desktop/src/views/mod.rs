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
pub mod stats;
pub mod widgets;

use crate::app::FerrumContainers;
use crate::state::{EngineStatus, Section};
use ferrum_ui::components::h_flex;
use ferrum_ui::{DraggedSplitter, splitter};
use gpui::{AnyElement, Axis, Context, DragMoveEvent, Window, div, prelude::*, px};

/// Everything between toolbar and status bar.
pub fn body(
    app: &mut FerrumContainers,
    window: &mut Window,
    cx: &mut Context<FerrumContainers>,
) -> AnyElement {
    let main = match &app.status {
        EngineStatus::Connecting => chrome::connecting(),
        EngineStatus::Offline(error) => {
            let error = error.clone();
            chrome::offline(&error, cx)
        }
        EngineStatus::Connected(_) => section(app, window, cx),
    };
    let connected = matches!(app.status, EngineStatus::Connected(_));
    h_flex()
        .id("ferrumdock-body")
        .flex_1()
        .min_h_0()
        .items_stretch()
        .when(connected, |d| d.child(chrome::sidebar(app, cx)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .child(main),
        )
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
                return list;
            }
            let panel = detail::render(app, window, cx);
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
                .child(div().flex_1().min_w(px(320.)).h_full().child(list))
                .child(splitter("detail", Axis::Horizontal, cx))
                .child(
                    div()
                        .flex_none()
                        .w(px(app.detail_width))
                        .h_full()
                        .child(panel),
                )
                .into_any_element()
        }
        Section::Images => images::render(app, cx),
        Section::Volumes => resources::volumes(app, cx),
        Section::Networks => resources::networks(app, cx),
    }
}
