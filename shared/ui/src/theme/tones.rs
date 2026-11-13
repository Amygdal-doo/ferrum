//! The base tones of Ferrum's built-in dark and light themes.

use gpui::{Hsla, hsla};

use super::Appearance;

/// HSL with the hue in degrees and saturation/lightness in percent.
fn hsl(h: f32, s: f32, l: f32) -> Hsla {
    hsla(h / 360., s / 100., l / 100., 1.)
}

/// `0xRRGGBB` as a colour.
fn hex(rgb: u32) -> Hsla {
    gpui::rgb(rgb).into()
}

/// The tones every other colour is derived from.
pub(super) struct Palette {
    pub(super) appearance: Appearance,
    pub(super) background: Hsla,
    pub(super) editor: Hsla,
    pub(super) surface: Hsla,
    pub(super) element: Hsla,
    pub(super) hover: Hsla,
    pub(super) active: Hsla,
    pub(super) selected: Hsla,
    pub(super) border: Hsla,
    pub(super) border_variant: Hsla,
    pub(super) border_disabled: Hsla,
    pub(super) text: Hsla,
    pub(super) text_muted: Hsla,
    pub(super) text_placeholder: Hsla,
    pub(super) icon: Hsla,
    pub(super) icon_muted: Hsla,
    pub(super) accent: Hsla,
    pub(super) line_number: Hsla,
    pub(super) active_line: Hsla,
    pub(super) selection: Hsla,
    /// blue, red, green, yellow, orange, purple, teal, gray
    pub(super) hues: [Hsla; 8],
    /// ANSI black, red, green, yellow, blue, magenta, cyan, white
    pub(super) ansi: [Hsla; 8],
    pub(super) ansi_bright: [Hsla; 8],
    pub(super) ansi_dim: [Hsla; 8],
    pub(super) vcs_added: Hsla,
    pub(super) vcs_modified: Hsla,
    pub(super) vcs_deleted: Hsla,
}

pub(super) fn ferrum_dark() -> Palette {
    let hover = hsl(225., 11.8, 26.7);
    Palette {
        appearance: Appearance::Dark,
        background: hsl(215., 12., 15.),
        editor: hsl(220., 12., 18.),
        surface: hsl(225., 12., 17.),
        element: hsl(223., 13., 21.),
        hover,
        active: hsl(220., 11.8, 20.),
        selected: hsl(224., 11.3, 26.1),
        border: hsl(225., 13., 12.),
        border_variant: hsl(228., 8., 25.),
        border_disabled: hsl(222., 11.6, 33.7),
        text: hsl(221., 11., 86.),
        text_muted: hsl(218., 7., 46.),
        text_placeholder: hsl(220., 6.6, 44.5),
        icon: hsl(222.9, 9.9, 86.1),
        icon_muted: hsl(220., 12.1, 66.1),
        accent: hsl(222.6, 77.5, 65.1),
        line_number: hsl(222., 11.5, 34.1),
        active_line: hsl(222.9, 13.5, 20.4),
        selection: hex(0x0d2847),
        hues: [
            hsl(207.8, 81., 66.),
            hsl(355., 65., 65.),
            hsl(95., 38., 62.),
            hsl(39., 67., 69.),
            hsl(29., 54., 61.),
            hsl(286., 51., 64.),
            hsl(187., 47., 55.),
            hsl(218.8, 10., 40.),
        ],
        ansi: [
            hex(0x111111),
            hex(0xff9592),
            hex(0x3dd68c),
            hex(0xf5e147),
            hex(0x70b8ff),
            hex(0xbaa7ff),
            hex(0x4ccce6),
            hex(0xeeeeee),
        ],
        ansi_bright: [
            hex(0x5c6370),
            hex(0xec5d5e),
            hex(0x33b074),
            hex(0xffff57),
            hex(0x3b9eff),
            hex(0x9b8afb),
            hex(0x23afd0),
            hex(0xb4b4b4),
        ],
        ansi_dim: [
            hex(0x3a3f4b),
            hex(0xe5484d),
            hex(0x30a46c),
            hex(0xffe629),
            hex(0x0090ff),
            hex(0x6e56cf),
            hex(0x00a2c7),
            hex(0x7b7b7b),
        ],
        vcs_added: hsl(134., 55., 40.),
        vcs_modified: hsl(48., 76., 47.),
        vcs_deleted: hsl(350., 88., 25.),
    }
}

pub(super) fn ferrum_light() -> Palette {
    Palette {
        appearance: Appearance::Light,
        background: hex(0xebebec),
        editor: hex(0xfafafa),
        surface: hex(0xf2f2f3),
        element: hex(0xe1e1e3),
        hover: hex(0xdcdcdf),
        active: hex(0xcfcfd3),
        selected: hex(0xd5d5d9),
        border: hex(0xc9c9cc),
        border_variant: hex(0xd9d9dc),
        border_disabled: hex(0xd3d3d6),
        text: hex(0x242529),
        text_muted: hex(0x58585c),
        text_placeholder: hex(0x8b8b90),
        icon: hex(0x242529),
        icon_muted: hex(0x58585c),
        accent: hex(0x5c78e2),
        line_number: hex(0xa0a1a7),
        active_line: hex(0xefeff1),
        selection: hex(0xcbdbfb),
        hues: [
            hex(0x4078f2),
            hex(0xca1243),
            hex(0x50a14f),
            hex(0xa77b00),
            hex(0xc18401),
            hex(0xa626a4),
            hex(0x0184bc),
            hex(0x8e8f96),
        ],
        ansi: [
            hex(0x242529),
            hex(0xca1243),
            hex(0x50a14f),
            hex(0xa77b00),
            hex(0x4078f2),
            hex(0xa626a4),
            hex(0x0184bc),
            hex(0xa0a1a7),
        ],
        ansi_bright: [
            hex(0x58585c),
            hex(0xe0466e),
            hex(0x6cba6b),
            hex(0xc29a10),
            hex(0x6a95f5),
            hex(0xc04fbe),
            hex(0x2aa2d4),
            hex(0xfafafa),
        ],
        ansi_dim: [
            hex(0x8b8b90),
            hex(0x9a0e33),
            hex(0x3b7b3a),
            hex(0x7d5c00),
            hex(0x2d58b5),
            hex(0x7d1c7b),
            hex(0x01628c),
            hex(0x7b7b80),
        ],
        vcs_added: hex(0x2ea043),
        vcs_modified: hex(0xbf8803),
        vcs_deleted: hex(0xcf222e),
    }
}
