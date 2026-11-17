//! Rendering. Each section is a function of the root view's state.

pub mod chrome;
pub mod compose_file;
pub mod compose_rows;
pub mod containers;
pub mod detail;
pub mod dialogs;
pub mod engines;
pub mod files;
pub mod image_dialogs;
pub mod images;
pub mod inspect;
pub mod logs;
pub mod registries;
pub mod resources;
pub mod run_dialog;
pub mod services;
pub mod settings_page;
pub mod stats;
pub mod widgets;

use crate::app::FerrumContainers;
use crate::state::{EngineStatus, Section};
use ferrum_ui::components::{h_flex, v_flex};
use ferrum_ui::{DraggedSplitter, gap_splitter};
use gpui::{
    AnyElement, App, Axis, Context, DragMoveEvent, MouseButton, Window, div, prelude::*, px,
};

/// The window's content: the section in a pane card headed by its title row
/// (with the engine, filter and Refresh) and closed by the quiet status
/// line, and the container detail in a second card.
pub fn body(
    app: &mut FerrumContainers,
    window: &mut Window,
    cx: &mut Context<FerrumContainers>,
) -> AnyElement {
    match &app.status {
        EngineStatus::Connecting => {
            let header = chrome::card_header(app, "Engine", None, Vec::new(), window, cx);
            let footer = chrome::card_footer(app, cx);
            card(true, header, chrome::connecting(), Some(footer), cx)
        }
        EngineStatus::Offline(error) => {
            let error = error.clone();
            let header = chrome::card_header(app, "Engine", None, Vec::new(), window, cx);
            let offline = chrome::offline(&error, cx);
            let footer = chrome::card_footer(app, cx);
            card(true, header, offline, Some(footer), cx)
        }
        EngineStatus::Connected(_) => section(app, window, cx),
    }
}

/// `header`, `content` and an optional `footer` in a pane card filling the
/// space it is given.
fn card(
    active: bool,
    header: AnyElement,
    content: AnyElement,
    footer: Option<AnyElement>,
    cx: &App,
) -> AnyElement {
    ferrum_ui::pane_card(active, cx)
        .size_full()
        .child(header)
        .child(v_flex().flex_1().min_h_0().child(content))
        .children(footer)
        .into_any_element()
}

fn section(
    app: &mut FerrumContainers,
    window: &mut Window,
    cx: &mut Context<FerrumContainers>,
) -> AnyElement {
    let footer = Some(chrome::card_footer(app, cx));
    match app.state.section {
        Section::Containers => {
            let (header, list) = containers::render(app, window, cx);
            if app.state.detail.is_none() {
                return card(true, header, list, footer, cx);
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
                        .child(card(!detail_active, header, list, footer, cx)),
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
                        .child(
                            ferrum_ui::pane_card(detail_active, cx)
                                .size_full()
                                .child(panel),
                        ),
                )
                .into_any_element()
        }
        Section::Images => {
            let (header, list) = images::render(app, window, cx);
            card(true, header, list, footer, cx)
        }
        Section::Volumes => {
            let (header, list) = resources::volumes(app, window, cx);
            card(true, header, list, footer, cx)
        }
        Section::Networks => {
            let (header, list) = resources::networks(app, window, cx);
            card(true, header, list, footer, cx)
        }
    }
}
