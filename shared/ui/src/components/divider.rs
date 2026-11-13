//! [`Divider`] lines and the [`ProgressBar`].

use gpui::{
    App, ElementId, Hsla, InteractiveElement, IntoElement, ParentElement, RenderOnce, Styled,
    Window, div, px, relative,
};

use crate::theme::ActiveTheme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    Horizontal,
    Vertical,
}

/// A 1px rule across (horizontal) or down (vertical) its container.
#[derive(IntoElement)]
pub struct Divider {
    axis: Axis,
    inset: bool,
}

impl Divider {
    pub fn horizontal() -> Self {
        Self {
            axis: Axis::Horizontal,
            inset: false,
        }
    }

    pub fn vertical() -> Self {
        Self {
            axis: Axis::Vertical,
            inset: false,
        }
    }

    /// Leave a small gap at both ends.
    pub fn inset(mut self) -> Self {
        self.inset = true;
        self
    }
}

impl RenderOnce for Divider {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = cx.theme().colors().border_variant;
        let line = div().flex_none().bg(color);
        let line = match self.axis {
            Axis::Horizontal => line.h(px(1.)).w_full(),
            Axis::Vertical => line.w(px(1.)).h_full(),
        };
        match (self.inset, self.axis) {
            (false, _) => line,
            (true, Axis::Horizontal) => line.mx_1p5(),
            (true, Axis::Vertical) => line.my_1p5(),
        }
    }
}

/// A rounded bar filled to `value / max`; overflow shows in the error colour.
#[derive(IntoElement)]
pub struct ProgressBar {
    id: ElementId,
    value: f32,
    max: f32,
    track: Hsla,
    fill: Hsla,
    overflow: Hsla,
}

impl ProgressBar {
    pub fn new(id: impl Into<ElementId>, value: f32, max: f32, cx: &App) -> Self {
        let theme = cx.theme();
        Self {
            id: id.into(),
            value,
            max,
            track: theme.colors().background,
            fill: theme.status().info,
            overflow: theme.status().error,
        }
    }

    pub fn fg_color(mut self, color: Hsla) -> Self {
        self.fill = color;
        self
    }

    pub fn bg_color(mut self, color: Hsla) -> Self {
        self.track = color;
        self
    }
}

/// Filled fraction, never quite empty so the bar reads as a bar.
fn fraction(value: f32, max: f32) -> f32 {
    if max > 0. && value.is_finite() {
        (value / max).clamp(0.02, 1.)
    } else {
        0.02
    }
}

impl RenderOnce for ProgressBar {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let fill = if self.value > self.max {
            self.overflow
        } else {
            self.fill
        };
        div()
            .id(self.id)
            .w_full()
            .h_2()
            .p_0p5()
            .rounded_full()
            .bg(self.track)
            .child(
                div()
                    .h_full()
                    .rounded_full()
                    .bg(fill)
                    .w(relative(fraction(self.value, self.max))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::fraction;

    #[test]
    fn fraction_is_clamped() {
        assert_eq!(fraction(0.5, 1.), 0.5);
        assert_eq!(fraction(0., 1.), 0.02);
        assert_eq!(fraction(3., 1.), 1.);
        assert_eq!(fraction(1., 0.), 0.02);
    }
}
