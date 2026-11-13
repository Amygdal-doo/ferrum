//! Text: [`Label`], [`LoadingLabel`] and [`Headline`].

use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, App, Div, FontWeight, IntoElement, ParentElement, Rems,
    RenderOnce, SharedString, Styled, Window, div, relative, rems,
};

use super::Color;
use super::traits::{StyledTypography, TextSize};
use crate::theme::{ActiveTheme, theme_settings};

/// Label text sizes (16 / 14 / 12 / 10 px).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabelSize {
    Large,
    #[default]
    Default,
    Small,
    XSmall,
}

impl From<LabelSize> for TextSize {
    fn from(size: LabelSize) -> Self {
        match size {
            LabelSize::Large => TextSize::Large,
            LabelSize::Default => TextSize::Default,
            LabelSize::Small => TextSize::Small,
            LabelSize::XSmall => TextSize::XSmall,
        }
    }
}

/// `UiLabel` sets the line height to the font size (tight rows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineHeightStyle {
    #[default]
    TextLabel,
    UiLabel,
}

/// Styling shared by every label kind.
#[derive(Debug, Clone, Default)]
pub struct LabelProps {
    size: LabelSize,
    color: Color,
    weight: Option<FontWeight>,
    alpha: Option<f32>,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    truncate: bool,
    single_line: bool,
    buffer_font: bool,
    line_height: LineHeightStyle,
}

impl LabelProps {
    /// Applies the props to a text container.
    fn apply(&self, base: Div, cx: &App) -> Div {
        let mut color = self.color.color(cx);
        if let Some(alpha) = self.alpha {
            color = color.alpha(alpha);
        }
        let mut el = base.text_ui_size(self.size.into(), cx).text_color(color);
        if self.buffer_font {
            el = el.font_buffer(cx);
        }
        if let Some(weight) = self.weight {
            el = el.font_weight(weight);
        }
        if self.italic {
            el = el.italic();
        }
        if self.underline {
            el = el.underline();
        }
        if self.strikethrough {
            el = el.line_through();
        }
        if self.line_height == LineHeightStyle::UiLabel {
            el = el.line_height(relative(1.));
        }
        if self.truncate {
            el = el.min_w_0().truncate();
        } else if self.single_line {
            el = el.whitespace_nowrap();
        }
        el
    }
}

/// Builder methods for [`Label`] and [`LoadingLabel`].
pub trait LabelCommon: Sized {
    fn props(&mut self) -> &mut LabelProps;

    fn size(mut self, size: LabelSize) -> Self {
        self.props().size = size;
        self
    }

    fn color(mut self, color: Color) -> Self {
        self.props().color = color;
        self
    }

    fn weight(mut self, weight: FontWeight) -> Self {
        self.props().weight = Some(weight);
        self
    }

    fn alpha(mut self, alpha: f32) -> Self {
        self.props().alpha = Some(alpha);
        self
    }

    fn italic(mut self) -> Self {
        self.props().italic = true;
        self
    }

    fn underline(mut self) -> Self {
        self.props().underline = true;
        self
    }

    fn strikethrough(mut self) -> Self {
        self.props().strikethrough = true;
        self
    }

    /// Cut off with an ellipsis instead of wrapping.
    fn truncate(mut self) -> Self {
        self.props().truncate = true;
        self
    }

    /// Never wrap; newlines show as spaces.
    fn single_line(mut self) -> Self {
        self.props().single_line = true;
        self
    }

    /// Render in the code font.
    fn buffer_font(mut self, _cx: &App) -> Self {
        self.props().buffer_font = true;
        self
    }

    fn line_height_style(mut self, style: LineHeightStyle) -> Self {
        self.props().line_height = style;
        self
    }
}

/// A run of UI text.
#[derive(IntoElement)]
pub struct Label {
    text: SharedString,
    props: LabelProps,
}

impl Label {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            props: LabelProps::default(),
        }
    }

    pub fn set_text(&mut self, text: impl Into<SharedString>) {
        self.text = text.into();
    }
}

impl LabelCommon for Label {
    fn props(&mut self) -> &mut LabelProps {
        &mut self.props
    }
}

impl RenderOnce for Label {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let text = if self.props.single_line && self.text.contains('\n') {
            SharedString::from(self.text.replace('\n', " "))
        } else {
            self.text
        };
        self.props.apply(div(), cx).child(text)
    }
}

/// Muted text with animated trailing dots ("Loading...").
#[derive(IntoElement)]
pub struct LoadingLabel {
    text: SharedString,
    props: LabelProps,
}

impl LoadingLabel {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            props: LabelProps {
                color: Color::Muted,
                ..LabelProps::default()
            },
        }
    }
}

impl LabelCommon for LoadingLabel {
    fn props(&mut self) -> &mut LabelProps {
        &mut self.props
    }
}

/// `text` followed by 0-3 dots for an animation position in `0..1`.
fn with_dots(text: &str, progress: f32) -> String {
    let dots = ((progress * 4.).floor() as usize).min(3);
    format!("{text}{}", ".".repeat(dots))
}

impl RenderOnce for LoadingLabel {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let text = self.text;
        self.props.apply(div(), cx).with_animation(
            "ferrum-loading-label",
            Animation::new(Duration::from_millis(1200)).repeat(),
            move |el, progress| el.child(with_dots(&text, progress)),
        )
    }
}

/// Heading sizes, in rems of the 16px root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeadlineSize {
    XSmall,
    #[default]
    Small,
    Medium,
    Large,
    XLarge,
}

impl HeadlineSize {
    pub fn rems(self) -> Rems {
        rems(match self {
            HeadlineSize::XSmall => 0.88,
            HeadlineSize::Small => 1.,
            HeadlineSize::Medium => 1.125,
            HeadlineSize::Large => 1.27,
            HeadlineSize::XLarge => 1.43,
        })
    }
}

/// A title in the UI font.
#[derive(IntoElement)]
pub struct Headline {
    text: SharedString,
    size: HeadlineSize,
    color: Color,
}

impl Headline {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            size: HeadlineSize::default(),
            color: Color::Default,
        }
    }

    pub fn size(mut self, size: HeadlineSize) -> Self {
        self.size = size;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl RenderOnce for Headline {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = match self.color {
            Color::Default => cx.theme().colors().text,
            other => other.color(cx),
        };
        div()
            .font(theme_settings(cx).ui_font.clone())
            .line_height(rems(1.6))
            .text_size(self.size.rems())
            .text_color(color)
            .child(self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_dots_cycle() {
        assert_eq!(with_dots("Loading", 0.), "Loading");
        assert_eq!(with_dots("Loading", 0.3), "Loading.");
        assert_eq!(with_dots("Loading", 0.99), "Loading...");
        assert_eq!(with_dots("Loading", 1.), "Loading...");
    }
}
