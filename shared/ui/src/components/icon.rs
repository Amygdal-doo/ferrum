//! [`Icon`]: a tinted SVG from the bundled set.

use gpui::{App, IntoElement, Rems, RenderOnce, Styled, Transformation, Window, svg};

use super::traits::rems_from_px;
use super::{Color, IconName};

/// Icon edge lengths (10 / 12 / 14 / 16 / 48 px).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum IconSize {
    Indicator,
    XSmall,
    #[default]
    Small,
    Medium,
    XLarge,
    Custom(Rems),
}

impl IconSize {
    pub fn rems(self) -> Rems {
        match self {
            IconSize::Indicator => rems_from_px(10.),
            IconSize::XSmall => rems_from_px(12.),
            IconSize::Small => rems_from_px(14.),
            IconSize::Medium => rems_from_px(16.),
            IconSize::XLarge => rems_from_px(48.),
            IconSize::Custom(size) => size,
        }
    }
}

/// Which side of a label an icon sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconPosition {
    #[default]
    Start,
    End,
}

#[derive(IntoElement)]
pub struct Icon {
    name: IconName,
    size: IconSize,
    color: Color,
    transformation: Option<Transformation>,
}

impl Icon {
    pub fn new(name: IconName) -> Self {
        Self {
            name,
            size: IconSize::default(),
            color: Color::Default,
            transformation: None,
        }
    }

    pub fn size(mut self, size: IconSize) -> Self {
        self.size = size;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Rotate/scale the glyph (e.g. a spinning refresh icon).
    pub fn transform(mut self, transformation: Transformation) -> Self {
        self.transformation = Some(transformation);
        self
    }
}

impl From<IconName> for Icon {
    fn from(name: IconName) -> Self {
        Icon::new(name)
    }
}

impl RenderOnce for Icon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let size = self.size.rems();
        let mut el = svg()
            .path(self.name.path())
            .flex_none()
            .size(size)
            .text_color(self.color.color(cx));
        if let Some(transformation) = self.transformation {
            el = el.with_transformation(transformation);
        }
        el
    }
}
