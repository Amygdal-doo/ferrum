//! Window chrome in the style of the Rex terminal: the native title bar is
//! hidden (the traffic lights float over the window, see
//! [`crate::AppConfig::unified_titlebar`]) and each app draws a slim
//! [`title_row`] instead, with the sidebar toggle, ⌘ and + buttons in it.
//! The sidebar sits on the window backdrop (no card) with [`sidebar_item`]
//! rows; content panes are [`pane_card`]s, the active one a little lighter.
//!
//! ```ignore
//! h_flex()
//!     .child(v_flex().w(sidebar_width)
//!         .child(title_row("title", true, window).child(sidebar_toggle(..)).child(div().flex_1()).child(command_button(..)))
//!         .child(sidebar_item("row-1", true, cx).child("Working Copy")))
//!     .child(v_flex().p(PANE_GAP).child(pane_card(active, cx).child(..)))
//! ```

use std::cell::Cell;

use gpui::{
    App, Div, ElementId, Hsla, InteractiveElement, MouseButton, ParentElement, Pixels, Point,
    SharedString, Stateful, Styled, Window, WindowControlArea, div, point, prelude::FluentBuilder,
    px,
};

use crate::components::{
    ButtonCommon, ButtonStyle, Clickable, Color, IconButton, IconName, IconSize, Tooltip, h_flex,
};
use crate::theme::ActiveTheme;

/// Height of the title row (and of the pane headers that line up with it)
/// in the Comfortable style; Compact uses [`COMPACT_TITLE_ROW_HEIGHT`].
pub const TITLE_ROW_HEIGHT: Pixels = px(40.);
/// [`TITLE_ROW_HEIGHT`] in the Compact style.
pub const COMPACT_TITLE_ROW_HEIGHT: Pixels = px(32.);

/// The title row's height in the current style (Comfortable or Compact).
#[must_use]
pub fn title_row_height() -> Pixels {
    if crate::appearance::compact() {
        COMPACT_TITLE_ROW_HEIGHT
    } else {
        TITLE_ROW_HEIGHT
    }
}
/// Room left of the title row's content for the traffic lights.
pub const TRAFFIC_LIGHTS_WIDTH: Pixels = px(76.);
/// Where the traffic lights sit, centred in the title row.
pub(crate) const TRAFFIC_LIGHT_POSITION: Point<Pixels> = Point {
    x: px(14.),
    y: px(13.),
};

thread_local! {
    /// A press in a title row that may turn into a window drag.
    static PRESSED: Cell<bool> = const { Cell::new(false) };
}

/// A row that moves the window when dragged and zooms it on double-click.
/// `lights`: leave room for the traffic lights (none in full screen).
pub fn title_row(id: impl Into<ElementId>, lights: bool, window: &Window) -> Stateful<Div> {
    let lights = lights && !window.is_fullscreen();
    h_flex()
        .id(id)
        .flex_none()
        .w_full()
        .h(title_row_height())
        .px_2()
        .gap_0p5()
        .when(lights, |row| row.pl(TRAFFIC_LIGHTS_WIDTH))
        .when_some(crate::appearance::title_bar_fill(), |row, fill| {
            row.bg(fill)
        })
        .window_control_area(WindowControlArea::Drag)
        .on_mouse_down(MouseButton::Left, |event, window, _| {
            if event.click_count == 2 {
                window.titlebar_double_click();
            } else {
                PRESSED.set(true);
            }
        })
        .on_mouse_up(MouseButton::Left, |_, _, _| PRESSED.set(false))
        .on_mouse_move(|event, window, _| {
            if event.pressed_button == Some(MouseButton::Left) && PRESSED.replace(false) {
                window.start_window_move();
            }
        })
}

/// Where the traffic lights go: centred in the title row of the current
/// style (the window moves them when the style changes).
#[must_use]
pub fn traffic_light_position() -> Point<Pixels> {
    let shift = (TITLE_ROW_HEIGHT - title_row_height()) / 2.;
    point(TRAFFIC_LIGHT_POSITION.x, TRAFFIC_LIGHT_POSITION.y - shift)
}

/// A small icon button for a title row or pane header.
pub fn title_button(
    id: impl Into<ElementId>,
    icon: IconName,
    tooltip: impl Into<SharedString>,
) -> IconButton {
    IconButton::new(id, icon)
        .style(ButtonStyle::Subtle)
        .icon_size(IconSize::Small)
        .icon_color(Color::Muted)
        .tooltip(Tooltip::text(tooltip.into()))
}

/// The ◧ button that shows or hides the sidebar (⌘B in most apps).
pub fn sidebar_toggle(id: impl Into<ElementId>, open: bool) -> IconButton {
    title_button(
        id,
        if open {
            IconName::ThreadsSidebarLeftOpen
        } else {
            IconName::ThreadsSidebarLeftClosed
        },
        if open { "Hide sidebar" } else { "Show sidebar" },
    )
}

/// The ◨ button that shows or hides a right-hand panel (inspector,
/// properties): [`sidebar_toggle`]'s mirror for the trailing edge.
pub fn panel_toggle(id: impl Into<ElementId>, open: bool) -> IconButton {
    title_button(
        id,
        if open {
            IconName::ThreadsSidebarRightOpen
        } else {
            IconName::ThreadsSidebarRightClosed
        },
        if open { "Hide panel" } else { "Show panel" },
    )
}

/// The gear next to ◧: opens the app's Settings window (⌘,).
pub fn settings_button(id: impl Into<ElementId>) -> IconButton {
    title_button(id, IconName::Settings, "Settings (⌘,)")
        .on_click(|_, _, cx| crate::open_settings(None, cx))
}

/// The ⌘ button: the app's command palette or project switcher.
pub fn command_button(id: impl Into<ElementId>, tooltip: impl Into<SharedString>) -> IconButton {
    title_button(id, IconName::Command, tooltip)
}

/// Fill of the active sidebar row or tab: a touch lighter than what is
/// behind it (darker on light themes).
#[must_use]
pub fn active_fill(cx: &App) -> Hsla {
    cx.theme().colors().text.opacity(0.09)
}

/// Fill of a hovered sidebar row or tab.
#[must_use]
pub fn hover_fill(cx: &App) -> Hsla {
    cx.theme().colors().text.opacity(0.05)
}

/// A sidebar row: rounded pill, lighter when `active`. Add an icon and a
/// label (and `.on_click`).
pub fn sidebar_item(id: impl Into<ElementId>, active: bool, cx: &App) -> Stateful<Div> {
    let colors = cx.theme().colors();
    let (active_bg, hover_bg) = (active_fill(cx), hover_fill(cx));
    h_flex()
        .id(id)
        .flex_none()
        .mx_1p5()
        .px_2()
        .min_h(if crate::appearance::compact() {
            px(24.)
        } else {
            px(30.)
        })
        .gap_2()
        .rounded(px(8.))
        .cursor_pointer()
        .text_color(if active {
            colors.text
        } else {
            colors.text_muted
        })
        .hover(move |s| s.bg(hover_bg))
        .when(active, |row| row.bg(active_bg))
}

/// A quiet group label in a sidebar ("Branches", "Agents").
pub fn sidebar_label(text: impl Into<SharedString>, cx: &App) -> Div {
    let compact = crate::appearance::compact();
    div()
        .flex_none()
        .px_3p5()
        .pt(if compact { px(8.) } else { px(12.) })
        .pb(if compact { px(2.) } else { px(4.) })
        .text_size(if compact { px(11.) } else { px(12.) })
        .text_color(cx.theme().colors().text_muted)
        .child(text.into())
}

/// A tab in a pane header: rounded, lighter when `active`.
pub fn header_tab(id: impl Into<ElementId>, active: bool, cx: &App) -> Stateful<Div> {
    let colors = cx.theme().colors();
    let (active_bg, hover_bg) = (active_fill(cx), hover_fill(cx));
    h_flex()
        .id(id)
        .flex_none()
        .h(if crate::appearance::compact() {
            px(22.)
        } else {
            px(26.)
        })
        .px_2p5()
        .gap_1p5()
        .rounded(px(7.))
        .cursor_pointer()
        .text_color(if active {
            colors.text
        } else {
            colors.text_muted
        })
        .hover(move |s| s.bg(hover_bg))
        .when(active, |tab| tab.bg(active_bg))
}

/// A content pane: [`crate::pane_frame`], a little lighter while `active`
/// (it has focus or was used last).
pub fn pane_card(active: bool, cx: &App) -> Div {
    let colors = cx.theme().colors();
    let lifted = crate::surface::lighten(colors.panel_background, colors.text, 0.035);
    crate::pane_frame(cx).when(active, |card| card.bg(lifted).border_color(colors.border))
}

/// Whether `handle` or something inside it has focus: the usual test for
/// a [`pane_card`]'s `active`.
#[must_use]
pub fn is_active(handle: &gpui::FocusHandle, window: &Window, cx: &App) -> bool {
    handle.contains_focused(window, cx)
}

/// `children` laid out as the window's title row when the sidebar is
/// hidden: traffic-light room, then the given controls.
pub fn floating_controls(window: &Window) -> Div {
    h_flex()
        .flex_none()
        .gap_0p5()
        .when(!window.is_fullscreen(), |row| {
            row.pl(TRAFFIC_LIGHTS_WIDTH - px(8.))
        })
}
