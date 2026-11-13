//! Ferrum's theme: colour slots, the built-in dark/light themes, the active
//! theme global and the UI/buffer font settings.
//!
//! ```ignore
//! ferrum_ui::theme::init(ThemeSettings::default(), cx);
//! let colors = cx.theme().colors();
//! div().bg(colors.panel_background).text_color(colors.text)
//! ```

mod colors;
pub mod contrast;
mod legible;
mod palette;
pub mod presets;
mod tones;

use std::sync::Arc;

use gpui::{App, Font, Global, Hsla, Pixels, SharedString, font, px};

pub use colors::{PlayerColor, PlayerColors, StatusColors, SyntaxTheme, ThemeColors};
pub use palette::{dark, light};

/// Whether a theme is light or dark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Appearance {
    Light,
    #[default]
    Dark,
}

impl Appearance {
    pub fn is_light(self) -> bool {
        self == Self::Light
    }
}

/// Every colour group of a theme.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeStyles {
    pub colors: ThemeColors,
    pub status: StatusColors,
    pub player: PlayerColors,
    pub syntax: Arc<SyntaxTheme>,
}

/// A complete theme. Clone it, adjust `styles`, and install it with
/// [`GlobalTheme::update_theme`] to recolour every window.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: SharedString,
    pub appearance: Appearance,
    pub styles: ThemeStyles,
}

impl Theme {
    pub fn colors(&self) -> &ThemeColors {
        &self.styles.colors
    }

    pub fn status(&self) -> &StatusColors {
        &self.styles.status
    }

    pub fn players(&self) -> &PlayerColors {
        &self.styles.player
    }

    pub fn syntax(&self) -> &Arc<SyntaxTheme> {
        &self.styles.syntax
    }

    pub fn appearance(&self) -> Appearance {
        self.appearance
    }

    /// Near-black or near-white text that reads on `bg`; a translucent `bg`
    /// is judged as drawn over this theme's background.
    pub fn readable_text_on(&self, bg: Hsla) -> Hsla {
        contrast::readable_text_over(bg, self.styles.colors.background)
    }

    /// `color` darkened by `light` on light themes and `dark` on dark ones.
    pub fn darken(&self, color: Hsla, light: f32, dark: f32) -> Hsla {
        let amount = if self.appearance.is_light() {
            light
        } else {
            dark
        };
        Hsla {
            l: (color.l - amount).max(0.),
            ..color
        }
    }
}

/// The theme every window renders with.
pub struct GlobalTheme(Arc<Theme>);

impl Global for GlobalTheme {}

impl GlobalTheme {
    /// Replaces the active theme; call `cx.refresh_windows()` afterwards to
    /// repaint.
    pub fn update_theme(cx: &mut App, theme: Arc<Theme>) {
        cx.set_global(Self(theme));
    }
}

/// `cx.theme()` on anything that derefs to [`App`].
pub trait ActiveTheme {
    fn theme(&self) -> &Arc<Theme>;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Arc<Theme> {
        match self.try_global::<GlobalTheme>() {
            Some(global) => &global.0,
            None => fallback(),
        }
    }
}

/// Used before [`init`] runs (e.g. in tests that skip it).
fn fallback() -> &'static Arc<Theme> {
    static FALLBACK: std::sync::OnceLock<Arc<Theme>> = std::sync::OnceLock::new();
    FALLBACK.get_or_init(|| Arc::new(dark()))
}

/// The fonts and sizes the UI renders with.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeSettings {
    pub ui_font: Font,
    pub buffer_font: Font,
    pub ui_font_size: Pixels,
    pub buffer_font_size: Pixels,
}

impl Default for ThemeSettings {
    /// The bundled IBM Plex Sans (UI) and Lilex (code) fonts.
    fn default() -> Self {
        Self::with_fonts(crate::assets::UI_FONT, crate::assets::BUFFER_FONT)
    }
}

impl ThemeSettings {
    /// 14px UI text and 13px code text in the given families.
    pub fn with_fonts(ui: impl Into<SharedString>, buffer: impl Into<SharedString>) -> Self {
        Self {
            ui_font: font(ui.into()),
            buffer_font: font(buffer.into()),
            ui_font_size: px(14.),
            buffer_font_size: px(13.),
        }
    }
}

impl Global for ThemeSettings {}

/// Installs `settings` and the default (dark) theme.
pub fn init(settings: ThemeSettings, cx: &mut App) {
    cx.set_global(settings);
    GlobalTheme::update_theme(cx, Arc::new(dark()));
}

/// The active font settings (defaults if [`init`] has not run).
pub fn theme_settings(cx: &App) -> &ThemeSettings {
    static DEFAULT: std::sync::OnceLock<ThemeSettings> = std::sync::OnceLock::new();
    cx.try_global::<ThemeSettings>()
        .unwrap_or_else(|| DEFAULT.get_or_init(ThemeSettings::default))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_themes_differ_in_appearance() {
        assert_eq!(dark().appearance, Appearance::Dark);
        assert_eq!(light().appearance, Appearance::Light);
        assert_ne!(dark().colors().background, light().colors().background);
    }

    #[test]
    fn darken_clamps_at_black() {
        let theme = dark();
        let darker = theme.darken(gpui::hsla(0., 0., 0.1, 1.), 0.05, 0.2);
        assert_eq!(darker.l, 0.);
    }
}

#[cfg(test)]
mod legibility_tests;
