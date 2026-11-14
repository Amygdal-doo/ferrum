//! The look every window shares: the backdrop behind a window's content
//! (flat, gradient or glass, see [`crate::appearance::InterfaceStyle`]) and
//! the rounded pane cards apps arrange on it, with gap splitters between.
//!
//! ```ignore
//! h_flex().p(PANE_GAP).child(pane_frame(cx).child(pane_header(cx).child(..)).child(body))
//!     .child(gap_splitter("split", Axis::Horizontal, cx))
//!     .child(pane_frame(cx).child(..))
//! ```

use gpui::{
    App, Axis, CursorStyle, Div, Hsla, IntoElement, SharedString, div, linear_color_stop,
    linear_gradient, prelude::*, px,
};

use crate::appearance::{self, InterfaceStyle};
use crate::components::{h_flex, v_flex};
use crate::splitter::DraggedSplitter;
use crate::theme::ActiveTheme;

/// Space around and between pane cards.
pub const PANE_GAP: gpui::Pixels = px(6.);

fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (a, b) = (gpui::Rgba::from(a), gpui::Rgba::from(b));
    gpui::Rgba {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.,
    }
    .into()
}

/// `base` moved `t` of the way towards `toward`, keeping `base`'s alpha.
pub(crate) fn lighten(base: Hsla, toward: Hsla, t: f32) -> Hsla {
    Hsla {
        a: base.a,
        ..mix(base, toward, t)
    }
}

/// `div` filled with the window backdrop. `overlay`: drawn over other
/// content (onboarding), so it stays nearly opaque even in Glass.
pub fn backdrop(overlay: bool, cx: &App) -> Div {
    let b = appearance::backdrop(cx);
    let tinted = mix(b.base, b.accent, 0.38);
    let deep = mix(b.base, gpui::black(), 0.25);
    match b.style {
        InterfaceStyle::Solid => div().bg(b.base),
        InterfaceStyle::Gradient => div().bg(linear_gradient(
            160.,
            linear_color_stop(tinted, 0.),
            linear_color_stop(deep, 1.),
        )),
        InterfaceStyle::Glass => div().bg(linear_gradient(
            160.,
            linear_color_stop(tinted.opacity(if overlay { 0.95 } else { 0.55 }), 0.),
            linear_color_stop(deep.opacity(if overlay { 0.97 } else { 0.7 }), 1.),
        )),
    }
}

/// A pane card: rounded, bordered, on the pane surface colour. Put a
/// [`pane_header`] and a body in it.
pub fn pane_frame(cx: &App) -> Div {
    let colors = cx.theme().colors();
    v_flex()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .rounded(px(10.))
        .border_1()
        .border_color(colors.border_variant)
        .bg(colors.panel_background)
        .shadow_sm()
}

/// The header strip of a pane card: icon, title and actions in a row.
pub fn pane_header(cx: &App) -> Div {
    let colors = cx.theme().colors();
    let compact = appearance::current(cx).toolbar == crate::appearance::ToolbarStyle::Compact;
    h_flex()
        .flex_none()
        .h(if compact { px(26.) } else { px(34.) })
        .px_2()
        .gap_1p5()
        .border_b_1()
        .border_color(colors.border_variant)
}

/// A resize handle that is just the gap between two pane cards; a thin
/// accent line shows on hover. Same drag payload as [`crate::splitter`].
pub fn gap_splitter(id: impl Into<SharedString>, axis: Axis, cx: &App) -> impl IntoElement {
    let id = id.into();
    let accent = cx.theme().colors().border_focused;
    let payload = DraggedSplitter {
        id: id.clone(),
        axis,
    };
    let line = div().rounded_full().bg(gpui::transparent_black());
    div()
        .id(gpui::ElementId::Name(id))
        .group("gap-splitter")
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .map(|this| match axis {
            Axis::Horizontal => this
                .w(PANE_GAP)
                .h_full()
                .cursor(CursorStyle::ResizeLeftRight)
                .child(
                    line.w(px(2.))
                        .h(px(32.))
                        .group_hover("gap-splitter", |s| s.bg(accent)),
                ),
            Axis::Vertical => this
                .h(PANE_GAP)
                .w_full()
                .cursor(CursorStyle::ResizeUpDown)
                .child(
                    line.h(px(2.))
                        .w(px(32.))
                        .group_hover("gap-splitter", |s| s.bg(accent)),
                ),
        })
        .on_drag(payload, |payload, _, _, cx| cx.new(|_| payload.clone()))
}
