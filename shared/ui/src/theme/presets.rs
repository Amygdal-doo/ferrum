//! The built-in theme catalog: Ferrum Dark / Light and tinted variants of
//! them. A variant recolours the neutral tones (backgrounds, borders, text)
//! towards one hue and swaps the accent; [`Theme::make_legible`] then keeps
//! every text colour readable.

use gpui::{Hsla, hsla};

use super::palette::build;
use super::tones::{Palette, ferrum_dark, ferrum_light};
use super::{Appearance, Theme};

/// One entry of the catalog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemePreset {
    /// Stable id stored in `~/.ferrum/appearance.json`.
    pub id: &'static str,
    pub name: &'static str,
    pub appearance: Appearance,
    /// `None`: the untinted Ferrum base.
    tint: Option<Tint>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Tint {
    /// Hue in degrees for the neutral tones.
    hue: f32,
    /// Saturation (0–1) of the neutral tones.
    saturation: f32,
    /// Accent as `0xRRGGBB`.
    accent: u32,
}

const fn preset(id: &'static str, name: &'static str, appearance: Appearance) -> ThemePreset {
    ThemePreset {
        id,
        name,
        appearance,
        tint: None,
    }
}

const fn tinted(
    id: &'static str,
    name: &'static str,
    appearance: Appearance,
    hue: f32,
    saturation: f32,
    accent: u32,
) -> ThemePreset {
    ThemePreset {
        id,
        name,
        appearance,
        tint: Some(Tint {
            hue,
            saturation,
            accent,
        }),
    }
}

use Appearance::{Dark, Light};

/// Every built-in theme, dark ones first.
pub const PRESETS: &[ThemePreset] = &[
    preset("ferrum-dark", "Ferrum Dark", Dark),
    tinted("forge-ember", "Forge Ember", Dark, 22., 0.14, 0xe8743b),
    tinted(
        "amber-phosphor",
        "Amber Phosphor",
        Dark,
        36.,
        0.22,
        0xf0b43c,
    ),
    tinted("deep-harbor", "Deep Harbor", Dark, 212., 0.32, 0x3fb8a0),
    tinted("night-moss", "Night Moss", Dark, 140., 0.10, 0x7cc85a),
    tinted("plum-ink", "Plum Ink", Dark, 285., 0.16, 0xc792ea),
    tinted("graphite", "Graphite", Dark, 0., 0., 0x8ab4f8),
    tinted("rust-red", "Oxide", Dark, 6., 0.12, 0xef6a5a),
    preset("ferrum-light", "Ferrum Light", Light),
    tinted("paper-sand", "Paper Sand", Light, 40., 0.32, 0xb85c1f),
    tinted("glacier", "Glacier", Light, 205., 0.34, 0x1f6fc5),
    tinted("sage", "Sage", Light, 115., 0.16, 0x3b7b3a),
    tinted("lilac-mist", "Lilac Mist", Light, 270., 0.24, 0x7d4fc4),
    tinted("porcelain", "Porcelain", Light, 0., 0., 0x2a2a2e),
];

/// The default theme id for an appearance.
pub fn default_id(appearance: Appearance) -> &'static str {
    match appearance {
        Dark => "ferrum-dark",
        Light => "ferrum-light",
    }
}

/// The preset with `id`, if there is one.
pub fn find(id: &str) -> Option<&'static ThemePreset> {
    PRESETS.iter().find(|p| p.id == id)
}

impl ThemePreset {
    /// The full theme.
    pub fn theme(&self) -> Theme {
        build(self.name, &self.palette())
    }

    /// Colours for a preview card: background, text, accent, then the eight
    /// normal ANSI colours.
    pub fn swatches(&self) -> PresetSwatches {
        let p = self.palette();
        PresetSwatches {
            background: p.background,
            surface: p.surface,
            text: p.text,
            accent: p.accent,
            ansi: p.ansi,
        }
    }

    fn palette(&self) -> Palette {
        let base = match self.appearance {
            Dark => ferrum_dark(),
            Light => ferrum_light(),
        };
        match self.tint {
            Some(tint) => tint.apply(base),
            None => base,
        }
    }
}

/// See [`ThemePreset::swatches`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PresetSwatches {
    pub background: Hsla,
    pub surface: Hsla,
    pub text: Hsla,
    pub accent: Hsla,
    pub ansi: [Hsla; 8],
}

impl Tint {
    fn apply(self, mut p: Palette) -> Palette {
        let hue = self.hue / 360.;
        let dark = p.appearance == Dark;
        // Lighter neutrals carry less saturation so text and borders stay calm.
        let neutral = |c: Hsla, weight: f32| hsla(hue, self.saturation * weight, c.l, c.a);
        for c in [
            &mut p.background,
            &mut p.editor,
            &mut p.surface,
            &mut p.element,
            &mut p.hover,
            &mut p.active,
            &mut p.selected,
            &mut p.border,
            &mut p.border_variant,
            &mut p.border_disabled,
            &mut p.active_line,
        ] {
            *c = neutral(*c, if dark { 1. } else { 0.8 });
        }
        for c in [
            &mut p.text,
            &mut p.text_muted,
            &mut p.text_placeholder,
            &mut p.icon,
            &mut p.icon_muted,
            &mut p.line_number,
        ] {
            *c = neutral(*c, 0.45);
        }
        let accent: Hsla = gpui::rgb(self.accent).into();
        p.accent = accent;
        p.selection = if dark {
            hsla(accent.h, accent.s * 0.6, 0.22, 1.)
        } else {
            hsla(accent.h, accent.s * 0.7, 0.86, 1.)
        };
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::contrast::contrast_ratio;

    #[test]
    fn ids_are_unique_and_both_appearances_have_a_default() {
        let mut ids: Vec<_> = PRESETS.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PRESETS.len());
        assert_eq!(find(default_id(Dark)).map(|p| p.appearance), Some(Dark));
        assert_eq!(find(default_id(Light)).map(|p| p.appearance), Some(Light));
    }

    #[test]
    fn every_preset_keeps_readable_text() {
        for preset in PRESETS {
            let theme = preset.theme();
            let c = theme.colors();
            assert_eq!(theme.appearance, preset.appearance, "{}", preset.id);
            let ratio = contrast_ratio(c.text, c.background);
            assert!(ratio >= 4.5, "{}: text {ratio}", preset.id);
            let muted = contrast_ratio(c.text_muted, c.background);
            assert!(muted >= 4.5, "{}: muted {muted}", preset.id);
        }
    }

    #[test]
    fn untinted_presets_match_the_base_themes() {
        assert_eq!(
            find("ferrum-dark").map(ThemePreset::theme),
            Some(super::super::dark())
        );
        assert_eq!(
            find("ferrum-light").map(ThemePreset::theme),
            Some(super::super::light())
        );
    }
}
