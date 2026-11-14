//! The app's body: the list and detail cards (each with its own header row),
//! or the compact pane layout (list over diff when narrow, side by side when
//! wide).

use super::headers;
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
        let loading = div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .child(Label::new("Loading…").color(Color::Muted));
        if app.options.compact {
            return loading.into_any_element();
        }
        return ferrum_ui::pane_card(true, cx)
            .flex_1()
            .h_full()
            .child(headers::home_header(app, "FerrumGIT", window, cx))
            .child(loading)
            .into_any_element();
    }
    if app.state.active.is_none() {
        if app.options.compact {
            return home::render(app, window, cx);
        }
        return ferrum_ui::pane_card(true, cx)
            .flex_1()
            .h_full()
            .child(headers::home_header(app, "Repositories", window, cx))
            .child(home::render(app, window, cx))
            .into_any_element();
    }
    if app.options.compact {
        compact_body(app, window, cx)
    } else {
        columns(app, window, cx)
    }
}

/// The list and detail cards, each headed by its title row; the one used
/// last is lighter.
fn columns(app: &mut GitView, window: &mut Window, cx: &mut Context<GitView>) -> AnyElement {
    let list_width = px(app.state.detail_width() as f32);
    let active = app.active_pane;
    h_flex()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .items_stretch()
        .child(
            ferrum_ui::pane_card(active == 0, cx)
                .flex_none()
                .w(list_width)
                .h_full()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|app, _, _, _| app.active_pane = 0),
                )
                .child(headers::list_header(app, window, cx))
                .child(middle_column(app, window, cx)),
        )
        .child(ferrum_ui::gap_splitter("detail", Axis::Horizontal, cx))
        .child(
            ferrum_ui::pane_card(active == 1, cx)
                .flex_1()
                .h_full()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|app, _, _, _| app.active_pane = 1),
                )
                .child(headers::detail_header(app, window, cx))
                .child(right_column(app, window, cx)),
        )
        .into_any_element()
}

/// The window's sidebar: title row (traffic lights, ◧ ⌘ +), the project,
/// then workspace pages, branches and remotes. On the backdrop, no card.
pub fn sidebar_column(
    app: &mut GitView,
    window: &mut Window,
    cx: &mut Context<GitView>,
) -> AnyElement {
    let width = px(app.state.sidebar_width() as f32);
    let show_project = app.options.show_project_bar;
    v_flex()
        .flex_none()
        .w(width)
        .h_full()
        .overflow_hidden()
        .child(
            ferrum_ui::title_row("sidebar-title", true, window)
                .child(div().flex_1())
                .child(headers::title_controls(app, cx)),
        )
        .when(show_project, |col| {
            col.child(div().px_1().child(app.project_bar.clone()))
        })
        .child(sidebar::render(app, window, cx))
        .into_any_element()
}

/// The window layout: sidebar, gap, then the cards from the top edge down.
pub fn window_row(
    sidebar: Option<AnyElement>,
    body: AnyElement,
    cx: &mut Context<GitView>,
) -> AnyElement {
    let has_sidebar = sidebar.is_some();
    resizable_row(cx)
        .size_full()
        .when_some(sidebar, |row, sidebar| {
            row.child(sidebar)
                .child(ferrum_ui::gap_splitter("sidebar", Axis::Horizontal, cx))
        })
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .py(ferrum_ui::PANE_GAP)
                .pr(ferrum_ui::PANE_GAP)
                .when(!has_sidebar, |d| d.pl(ferrum_ui::PANE_GAP))
                .child(body),
        )
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
