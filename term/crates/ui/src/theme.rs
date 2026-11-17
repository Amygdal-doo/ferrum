//! Terminal colour themes (Settings › Terminal › Theme, `theme` in
//! `config.toml`). *Ferrum* follows the shared Ferrum theme (light or dark,
//! as the other apps); the others are fixed palettes.
//!
//! The server resolves colours to RGB with the emulator's default palette
//! (`ferrumterm_emulator::ANSI`, `DEFAULT_FG` / `DEFAULT_BG`), so painting
//! maps those values onto the theme's; any other colour (256-colour cube,
//! truecolor) is painted as sent.

use ferrum_ui::gpui::{App, Hsla, rgb};
use ferrum_ui::theme::ActiveTheme;
use ferrumterm_emulator::{ANSI, DEFAULT_BG, DEFAULT_FG};

/// A built-in theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermTheme {
    /// Its name in `config.toml`.
    pub id: &'static str,
    pub name: &'static str,
    /// `None`: the shared Ferrum theme.
    colors: Option<Fixed>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fixed {
    foreground: u32,
    background: u32,
    ansi: [u32; 16],
}

const SOLARIZED: [u32; 16] = [
    0x073642, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5, 0x002b36,
    0xcb4b16, 0x586e75, 0x657b83, 0x839496, 0x6c71c4, 0x93a1a1, 0xfdf6e3,
];

pub const THEMES: [TermTheme; 7] = [
    TermTheme {
        id: "ferrum",
        name: "Ferrum",
        colors: None,
    },
    TermTheme {
        id: "classic",
        name: "Classic",
        colors: Some(Fixed {
            foreground: DEFAULT_FG,
            background: DEFAULT_BG,
            ansi: ANSI,
        }),
    },
    TermTheme {
        id: "solarized-dark",
        name: "Solarized Dark",
        colors: Some(Fixed {
            foreground: 0x839496,
            background: 0x002b36,
            ansi: SOLARIZED,
        }),
    },
    TermTheme {
        id: "solarized-light",
        name: "Solarized Light",
        colors: Some(Fixed {
            foreground: 0x657b83,
            background: 0xfdf6e3,
            ansi: SOLARIZED,
        }),
    },
    TermTheme {
        id: "dracula",
        name: "Dracula",
        colors: Some(Fixed {
            foreground: 0xf8f8f2,
            background: 0x282a36,
            ansi: [
                0x21222c, 0xff5555, 0x50fa7b, 0xf1fa8c, 0xbd93f9, 0xff79c6, 0x8be9fd, 0xf8f8f2,
                0x6272a4, 0xff6e6e, 0x69ff94, 0xffffa5, 0xd6acff, 0xff92df, 0xa4ffff, 0xffffff,
            ],
        }),
    },
    TermTheme {
        id: "nord",
        name: "Nord",
        colors: Some(Fixed {
            foreground: 0xd8dee9,
            background: 0x2e3440,
            ansi: [
                0x3b4252, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x88c0d0, 0xe5e9f0,
                0x4c566a, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x8fbcbb, 0xeceff4,
            ],
        }),
    },
    TermTheme {
        id: "gruvbox-dark",
        name: "Gruvbox Dark",
        colors: Some(Fixed {
            foreground: 0xebdbb2,
            background: 0x282828,
            ansi: [
                0x282828, 0xcc241d, 0x98971a, 0xd79921, 0x458588, 0xb16286, 0x689d6a, 0xa89984,
                0x928374, 0xfb4934, 0xb8bb26, 0xfabd2f, 0x83a598, 0xd3869b, 0x8ec07c, 0xebdbb2,
            ],
        }),
    },
];

/// The theme named `id`; *Ferrum* for names it doesn't know.
#[must_use]
pub fn by_id(id: &str) -> TermTheme {
    THEMES
        .into_iter()
        .find(|t| t.id.eq_ignore_ascii_case(id.trim()))
        .unwrap_or(THEMES[0])
}

/// The colours a pane is painted with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub foreground: Hsla,
    pub background: Hsla,
    pub ansi: [Hsla; 16],
}

impl Palette {
    /// The palette of `theme` (the shared theme's terminal colours for *Ferrum*).
    #[must_use]
    pub fn of(theme: TermTheme, cx: &App) -> Self {
        match theme.colors {
            Some(fixed) => Self {
                foreground: rgb(fixed.foreground).into(),
                background: rgb(fixed.background).into(),
                ansi: fixed.ansi.map(|c| rgb(c).into()),
            },
            None => {
                let c = cx.theme().colors();
                Self {
                    foreground: c.terminal_foreground,
                    background: c.terminal_background,
                    ansi: [
                        c.terminal_ansi_black,
                        c.terminal_ansi_red,
                        c.terminal_ansi_green,
                        c.terminal_ansi_yellow,
                        c.terminal_ansi_blue,
                        c.terminal_ansi_magenta,
                        c.terminal_ansi_cyan,
                        c.terminal_ansi_white,
                        c.terminal_ansi_bright_black,
                        c.terminal_ansi_bright_red,
                        c.terminal_ansi_bright_green,
                        c.terminal_ansi_bright_yellow,
                        c.terminal_ansi_bright_blue,
                        c.terminal_ansi_bright_magenta,
                        c.terminal_ansi_bright_cyan,
                        c.terminal_ansi_bright_white,
                    ],
                }
            }
        }
    }

    /// The palette of the theme in the settings.
    #[must_use]
    pub fn current(cx: &App) -> Self {
        Self::of(by_id(&crate::settings::get().theme), cx)
    }

    /// A colour from a frame: the emulator's default colours become the
    /// theme's, others are painted as sent.
    #[must_use]
    pub fn color(&self, value: u32) -> Hsla {
        if let Some(i) = ANSI.iter().position(|c| *c == value) {
            return self.ansi[i];
        }
        match value {
            DEFAULT_FG => self.foreground,
            DEFAULT_BG => self.background,
            other => rgb(other).into(),
        }
    }

    /// A span's foreground (`None`: the default).
    #[must_use]
    pub fn foreground(&self, value: Option<u32>) -> Hsla {
        value.map_or(self.foreground, |v| self.color(v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_resolve_and_unknown_ones_follow_ferrum() {
        assert_eq!(by_id("Dracula").name, "Dracula");
        assert_eq!(by_id(" nord ").id, "nord");
        assert_eq!(by_id("no-such-theme").id, "ferrum");
        let ids: std::collections::HashSet<_> = THEMES.iter().map(|t| t.id).collect();
        assert_eq!(ids.len(), THEMES.len(), "unique ids");
    }

    #[test]
    fn the_emulator_palette_maps_onto_the_theme() {
        let TermTheme {
            colors: Some(fixed),
            ..
        } = by_id("solarized-light")
        else {
            panic!("fixed colours");
        };
        let palette = Palette {
            foreground: rgb(fixed.foreground).into(),
            background: rgb(fixed.background).into(),
            ansi: fixed.ansi.map(|c| rgb(c).into()),
        };
        // ANSI red as the server resolves it → Solarized red.
        assert_eq!(palette.color(ANSI[1]), Hsla::from(rgb(0xdc322f)));
        assert_eq!(palette.color(DEFAULT_BG), Hsla::from(rgb(0xfdf6e3)));
        assert_eq!(palette.foreground(None), Hsla::from(rgb(0x657b83)));
        // Truecolor and the 256-colour cube are left alone.
        assert_eq!(palette.color(0x123456), Hsla::from(rgb(0x123456)));
    }
}
