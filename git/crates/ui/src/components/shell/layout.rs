//! The body under the toolbar: the app's three columns, or the compact pane
//! layout (list over diff when narrow, side by side when wide).

use crate::app::GitView;
use crate::components::detail::{middle_column, right_column};
use crate::components::{home, sidebar};
use ferrum_ui::components::{Color, Label, LabelCommon, h_flex, v_flex};
use ferrum_ui::splitter;
use ferrum_ui::theme::ActiveTheme;
use gpui::{AnyElement, Axis, Context, MouseButton, Pixels, Window, div, prelude::*, px};

/// Narrowest width at which the compact layout puts list and diff side by side.
pub const WIDE_LAYOUT_MIN_WIDTH: f32 = 720.0;
/// Narrowest width the compact layout is designed for.
pub const MIN_WIDTH: f32 = 320.0;
/// Branch drawer width in the compact layout.
const DRAWER_WIDTH: f32 = 240.0;

/// Width of the compact branch drawer (narrow panes keep some list visible).
#[must_use]
pub fn drawer_width(view_width: Option<Pixels>) -> f32 {
    view_width.map_or(DRAWER_WIDTH, |w| DRAWER_WIDTH.min(f32::from(w) * 0.8))
}

#[must_use]
pub fn is_wide(width: Pixels) -> bool {
    f32::from(width) >= WIDE_LAYOUT_MIN_WIDTH
}

pub fn render_body(
    app: &mut GitView,
    window: &mut Window,
    cx: &mut Context<GitView>,
) -> AnyElement {
    if !app.state.loaded {
        return div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .child(Label::new("Loading…").color(Color::Muted))
            .into_any_element();
    }
    if app.state.active.is_none() {
        return home::render(app, window, cx);
    }
    if app.options.compact {
        compact_body(app, window, cx)
    } else {
        columns(app, window, cx)
    }
}

fn columns(app: &mut GitView, window: &mut Window, cx: &mut Context<GitView>) -> AnyElement {
    let sidebar_width = px(app.state.sidebar_width() as f32);
    let list_width = px(app.state.detail_width() as f32);
    resizable_row(cx)
        .child(
            div()
                .flex_none()
                .w(sidebar_width)
                .h_full()
                .overflow_hidden()
                .child(sidebar::render(app, window, cx)),
        )
        .child(splitter("sidebar", Axis::Horizontal, cx))
        .child(
            div()
                .flex_none()
                .w(list_width)
                .h_full()
                .overflow_hidden()
                .child(middle_column(app, window, cx)),
        )
        .child(splitter("detail", Axis::Horizontal, cx))
        .child(right_column(app, window, cx))
        .into_any_element()
}

fn compact_body(app: &mut GitView, window: &mut Window, cx: &mut Context<GitView>) -> AnyElement {
    let width = app.width.get();
    let wide = width.is_some_and(is_wide);
    let border = cx.theme().colors().border;
    let drawer = app.drawer_open.then(|| {
        div()
            .flex_none()
            .w(px(drawer_width(width)))
            .h_full()
            .overflow_hidden()
            .border_r_1()
            .border_color(border)
            .child(sidebar::render(app, window, cx))
    });
    let list = middle_column(app, window, cx);
    let detail = right_column(app, window, cx);
    let content = if wide {
        let list_width = app.compact_split.unwrap_or(360.0);
        h_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .items_stretch()
            .child(
                div()
                    .flex_none()
                    .w(px(list_width))
                    .h_full()
                    .overflow_hidden()
                    .child(list),
            )
            .child(splitter("detail", Axis::Horizontal, cx))
            .child(detail)
            .into_any_element()
    } else {
        let list_height = app.compact_split.unwrap_or(240.0);
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                div()
                    .flex_none()
                    .h(px(list_height))
                    .w_full()
                    .overflow_hidden()
                    .child(list),
            )
            .child(splitter("stack", Axis::Vertical, cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_hidden()
                    .child(detail),
            )
            .into_any_element()
    };
    resizable_row(cx)
        .children(drawer)
        .child(content)
        .into_any_element()
}

/// Row that turns splitter drags into sizes (saved on release in the app).
fn resizable_row(cx: &mut Context<GitView>) -> gpui::Stateful<gpui::Div> {
    h_flex()
        .id("ferrumgit-columns")
        .flex_1()
        .min_h_0()
        .min_w_0()
        .items_stretch()
        .on_drag_move(cx.listener(GitView::on_resize))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|app, _, _, cx| {
                if std::mem::take(&mut app.resizing) {
                    app.save_settings(cx);
                }
            }),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_by_side_from_the_breakpoint() {
        assert!(!is_wide(px(MIN_WIDTH)));
        assert!(!is_wide(px(WIDE_LAYOUT_MIN_WIDTH - 1.0)));
        assert!(is_wide(px(WIDE_LAYOUT_MIN_WIDTH)));
        assert!(is_wide(px(1600.0)));
        assert!((drawer_width(None) - DRAWER_WIDTH).abs() < f32::EPSILON);
        assert!((drawer_width(Some(px(MIN_WIDTH))) - DRAWER_WIDTH).abs() < f32::EPSILON);
        assert!((drawer_width(Some(px(200.0))) - 160.0).abs() < f32::EPSILON);
    }
}
