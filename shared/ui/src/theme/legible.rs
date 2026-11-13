//! [`Theme::make_legible`]: nudges a theme's text colours to their contrast
//! targets. The built-in themes run it on their raw tones; hosts that load
//! their own colours (e.g. the editor's token themes) run it after applying
//! them, so every app keeps readable text whatever the theme.
//!
//! Targets (WCAG 2.x AA):
//! - 4.5:1: text, muted text, accent/link text, every status colour, git
//!   states and syntax colours, on every surface of the theme's appearance;
//! - 3:1: placeholders, icons and line numbers (hints and non-text UI).

use gpui::Hsla;

use super::Theme;
use super::contrast::{
    MIN_TEXT_CONTRAST, MIN_UI_CONTRAST, composite, ensure_contrast, readable_text_on,
};

/// Opacity of a status hue in its `*_background` tint.
pub const STATUS_TINT: f32 = 0.16;
/// Opacity of a status hue in its `*_border`.
pub const STATUS_BORDER: f32 = 0.5;

/// `color` nudged until it reaches `min` against every one of `on`.
fn legible(color: Hsla, on: &[Hsla], min: f32) -> Hsla {
    on.iter().fold(color, |c, &bg| ensure_contrast(c, bg, min))
}

impl Theme {
    /// The opaque surfaces text is drawn on, without bars of the opposite
    /// lightness (a dark title bar in a light theme): no single colour can
    /// suit both.
    fn text_surfaces(&self) -> Vec<Hsla> {
        let c = &self.styles.colors;
        let base = c.background;
        let side = readable_text_on(base);
        [
            c.background,
            c.surface_background,
            c.elevated_surface_background,
            c.panel_background,
            c.editor_background,
            c.element_background,
            c.element_hover,
            c.element_selected,
            c.element_active,
            c.tab_active_background,
            c.tab_inactive_background,
            c.tab_bar_background,
            c.title_bar_background,
            c.toolbar_background,
            c.status_bar_background,
        ]
        .into_iter()
        .map(|s| composite(s, base))
        .filter(|&s| readable_text_on(s) == side)
        .collect()
    }

    /// `color` (e.g. a fixed brand or ANSI colour) made readable as text on
    /// this theme's surfaces at 4.5:1, keeping its hue.
    pub fn legible_text(&self, color: Hsla) -> Hsla {
        legible(color, &self.text_surfaces(), MIN_TEXT_CONTRAST)
    }

    /// (background, text) of a tinted chip in `color`: a faint opaque tint
    /// and `color` nudged to 4.5:1 on it and on the plain surfaces.
    pub fn tinted_chip(&self, color: Hsla) -> (Hsla, Hsla) {
        let tint = composite(color.alpha(STATUS_TINT), self.styles.colors.background);
        let mut on = self.text_surfaces();
        on.push(tint);
        (tint, legible(color, &on, MIN_TEXT_CONTRAST))
    }

    /// Moves the lightness of every text-like colour (keeping its hue) until
    /// it meets its contrast target, and rebuilds each status `*_background`
    /// as a faint opaque tint of its hue and `*_border` as a half-strength
    /// one. Colours that already pass are left untouched.
    pub fn make_legible(&mut self) {
        let surfaces = self.text_surfaces();
        let c = &mut self.styles.colors;
        let base = c.background;
        let text = |color: Hsla| legible(color, &surfaces, MIN_TEXT_CONTRAST);
        let ui = |color: Hsla| legible(color, &surfaces, MIN_UI_CONTRAST);

        for slot in [
            &mut c.text,
            &mut c.text_muted,
            &mut c.text_accent,
            &mut c.link_text_hover,
            &mut c.editor_foreground,
            &mut c.editor_active_line_number,
            &mut c.version_control_added,
            &mut c.version_control_deleted,
            &mut c.version_control_modified,
            &mut c.version_control_renamed,
            &mut c.version_control_conflict,
            &mut c.version_control_ignored,
        ] {
            *slot = text(*slot);
        }
        for slot in [
            &mut c.text_placeholder,
            &mut c.icon,
            &mut c.icon_muted,
            &mut c.icon_placeholder,
            &mut c.icon_accent,
        ] {
            *slot = ui(*slot);
        }
        let code = [
            composite(c.editor_background, base),
            composite(c.editor_active_line_background, c.editor_background),
        ];
        c.editor_line_number = legible(c.editor_line_number, &code, MIN_UI_CONTRAST);

        let mut tints = Vec::new();
        for (fg, bg, border) in self.styles.status.triples_mut() {
            let tint = composite(fg.alpha(STATUS_TINT), base);
            let mut on = surfaces.clone();
            on.push(tint);
            *fg = legible(*fg, &on, MIN_TEXT_CONTRAST);
            *bg = tint;
            *border = composite(fg.alpha(STATUS_BORDER), base);
            tints.push(tint);
        }
        // Banners and tinted buttons put regular text on the tints.
        c.text = legible(c.text, &tints, MIN_TEXT_CONTRAST);

        std::sync::Arc::make_mut(&mut self.styles.syntax)
            .map_colors(|color| legible(color, &code, MIN_TEXT_CONTRAST));
    }
}
