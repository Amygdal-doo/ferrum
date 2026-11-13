//! Semantic colours resolved against the active theme.

use gpui::{App, Hsla};

use crate::theme::ActiveTheme;

/// A text/icon colour by meaning; [`Color::color`] looks it up in the theme.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Color {
    #[default]
    Default,
    Accent,
    Conflict,
    Created,
    Custom(Hsla),
    Deleted,
    Disabled,
    Error,
    Hidden,
    Hint,
    Ignored,
    Info,
    Modified,
    Muted,
    Placeholder,
    Selected,
    Success,
    Warning,
}

impl Color {
    pub fn color(self, cx: &App) -> Hsla {
        let theme = cx.theme();
        let (c, s) = (theme.colors(), theme.status());
        match self {
            Color::Default => c.text,
            Color::Accent | Color::Selected => c.text_accent,
            Color::Conflict => c.version_control_conflict,
            Color::Created => c.version_control_added,
            Color::Custom(color) => color,
            Color::Deleted => c.version_control_deleted,
            Color::Disabled => c.text_disabled,
            Color::Error => s.error,
            Color::Hidden => s.hidden,
            Color::Hint => s.hint,
            Color::Ignored => s.ignored,
            Color::Info => s.info,
            Color::Modified => c.version_control_modified,
            Color::Muted => c.text_muted,
            Color::Placeholder => c.text_placeholder,
            Color::Success => s.success,
            Color::Warning => s.warning,
        }
    }
}

impl From<Hsla> for Color {
    fn from(color: Hsla) -> Self {
        Color::Custom(color)
    }
}

/// The status hue of a [`crate::components::ButtonStyle::Tinted`] button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TintColor {
    #[default]
    Accent,
    Error,
    Warning,
    Success,
}

impl TintColor {
    /// (background, border) for a tinted button.
    pub(crate) fn fill(self, cx: &App) -> (Hsla, Hsla) {
        let s = cx.theme().status();
        match self {
            TintColor::Accent => (s.info_background, s.info_border),
            TintColor::Error => (s.error_background, s.error_border),
            TintColor::Warning => (s.warning_background, s.warning_border),
            TintColor::Success => (s.success_background, s.success_border),
        }
    }
}

impl From<TintColor> for Color {
    fn from(tint: TintColor) -> Self {
        match tint {
            TintColor::Accent => Color::Accent,
            TintColor::Error => Color::Error,
            TintColor::Warning => Color::Warning,
            TintColor::Success => Color::Success,
        }
    }
}
