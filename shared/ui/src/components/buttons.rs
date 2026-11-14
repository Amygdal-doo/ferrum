//! [`Button`] (label with an optional icon) and [`IconButton`].

use gpui::{AnyElement, App, ElementId, IntoElement, RenderOnce, SharedString, Window};

use super::button::ButtonBase;
use super::button_traits::button_traits;
use super::{Color, Icon, IconName, IconSize, Label, LabelCommon, LabelSize};

/// The content colour for a button state.
fn content_color(base: &ButtonBase, color: Color, selected: Color) -> Color {
    if base.disabled {
        Color::Disabled
    } else if base.selected {
        selected
    } else {
        color
    }
}

/// A text button.
#[derive(IntoElement)]
pub struct Button {
    base: ButtonBase,
    label: SharedString,
    label_size: LabelSize,
    color: Color,
    selected_color: Option<Color>,
    start_icon: Option<Icon>,
    end_icon: Option<Icon>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            base: ButtonBase::new(id),
            label: label.into(),
            label_size: LabelSize::Default,
            color: Color::Default,
            selected_color: None,
            start_icon: None,
            end_icon: None,
        }
    }

    pub fn label_size(mut self, size: LabelSize) -> Self {
        self.label_size = size;
        self
    }

    /// Label colour.
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Label colour while toggled on (default: accent).
    pub fn selected_label_color(mut self, color: Color) -> Self {
        self.selected_color = Some(color);
        self
    }

    /// Icon before the label.
    pub fn start_icon(mut self, icon: impl Into<Icon>) -> Self {
        self.start_icon = Some(icon.into());
        self
    }

    /// Icon after the label.
    pub fn end_icon(mut self, icon: impl Into<Icon>) -> Self {
        self.end_icon = Some(icon.into());
        self
    }
}

button_traits!(Button);

impl RenderOnce for Button {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let selected = self.selected_color.unwrap_or(Color::Selected);
        let color = content_color(&self.base, self.color, selected);
        let disabled = self.base.disabled;
        let tint = |icon: Icon| {
            let icon = if disabled {
                icon.color(Color::Disabled)
            } else {
                icon
            };
            icon.into_any_element()
        };
        let mut children = Vec::with_capacity(3);
        children.extend(self.start_icon.map(tint));
        children.push(
            Label::new(self.label)
                .size(self.label_size)
                .color(color)
                .into_any_element(),
        );
        children.extend(self.end_icon.map(tint));
        self.base.render(children, cx)
    }
}

/// A button showing one icon.
#[derive(IntoElement)]
pub struct IconButton {
    base: ButtonBase,
    icon: IconName,
    icon_size: IconSize,
    icon_color: Color,
    selected_icon: Option<IconName>,
    selected_color: Option<Color>,
}

impl IconButton {
    pub fn new(id: impl Into<ElementId>, icon: IconName) -> Self {
        Self {
            base: ButtonBase::new(id),
            icon,
            icon_size: IconSize::Small,
            icon_color: Color::Default,
            selected_icon: None,
            selected_color: None,
        }
    }

    pub fn icon_size(mut self, size: IconSize) -> Self {
        self.icon_size = size;
        self
    }

    pub fn icon_color(mut self, color: Color) -> Self {
        self.icon_color = color;
        self
    }

    /// A different glyph while toggled on.
    pub fn selected_icon(mut self, icon: impl Into<Option<IconName>>) -> Self {
        self.selected_icon = icon.into();
        self
    }

    /// Icon colour while toggled on (default: accent).
    pub fn selected_icon_color(mut self, color: impl Into<Option<Color>>) -> Self {
        self.selected_color = color.into();
        self
    }
}

button_traits!(IconButton);

impl RenderOnce for IconButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let selected = match (self.base.selected_style, self.selected_color) {
            (_, Some(color)) => color,
            (Some(style), None) => style.into(),
            (None, None) => Color::Selected,
        };
        let color = content_color(&self.base, self.icon_color, selected);
        let name = match self.selected_icon {
            Some(icon) if self.base.selected => icon,
            _ => self.icon,
        };
        let icon = Icon::new(name).size(self.icon_size).color(color);
        self.base.render(vec![icon.into_any_element()], cx)
    }
}

/// A button around any content, e.g. a logo or a tile.
#[derive(IntoElement)]
pub struct ContentButton {
    base: ButtonBase,
    content: AnyElement,
}

impl ContentButton {
    pub fn new(id: impl Into<ElementId>, content: impl IntoElement) -> Self {
        Self {
            base: ButtonBase::new(id),
            content: content.into_any_element(),
        }
    }
}

button_traits!(ContentButton);

impl RenderOnce for ContentButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        self.base.render(vec![self.content], cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_wins_over_selected() {
        let mut base = ButtonBase::new("b");
        base.selected = true;
        assert_eq!(
            content_color(&base, Color::Muted, Color::Accent),
            Color::Accent
        );
        base.disabled = true;
        assert_eq!(
            content_color(&base, Color::Muted, Color::Accent),
            Color::Disabled
        );
    }
}
