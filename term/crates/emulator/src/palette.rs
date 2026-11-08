//! Default colours (Ghostty-like dark theme) and colour resolution.

use alacritty_terminal::term::Term;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

/// ANSI 0–15.
pub const ANSI: [u32; 16] = [
    0x1d1f21, 0xcc6666, 0xb5bd68, 0xf0c674, 0x81a2be, 0xb294bb, 0x8abeb7, 0xc5c8c6, 0x666666,
    0xd54e53, 0xb9ca4a, 0xe7c547, 0x7aa6da, 0xc397d8, 0x70c0b1, 0xeaeaea,
];
/// Default foreground / background, used when a cell must be resolved (inverse video).
pub const DEFAULT_FG: u32 = 0xd8d8d8;
pub const DEFAULT_BG: u32 = 0x161719;

/// xterm 256-colour palette entry.
#[must_use]
pub fn indexed(i: u8) -> u32 {
    match i {
        0..=15 => ANSI[usize::from(i)],
        16..=231 => {
            let i = i - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + u32::from(v) * 40 };
            (level(i / 36) << 16) | (level((i / 6) % 6) << 8) | level(i % 6)
        }
        232..=255 => {
            let v = 8 + u32::from(i - 232) * 10;
            (v << 16) | (v << 8) | v
        }
    }
}

fn rgb(c: Rgb) -> u32 {
    (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)
}

/// Resolves a cell colour; `None` = the client's default fg/bg.
#[must_use]
pub fn resolve(color: Color, colors: &Colors) -> Option<u32> {
    match color {
        Color::Spec(c) => Some(rgb(c)),
        Color::Indexed(i) => Some(colors[usize::from(i)].map_or_else(|| indexed(i), rgb)),
        Color::Named(n) => {
            let idx = n as usize;
            if let Some(c) = colors[n] {
                return Some(rgb(c));
            }
            match n {
                NamedColor::Foreground
                | NamedColor::BrightForeground
                | NamedColor::Background
                | NamedColor::Cursor => None,
                NamedColor::DimForeground => Some(0x9a9a9a),
                _ if idx < 16 => Some(ANSI[idx]),
                // Dim variants map onto the base colours.
                _ => {
                    let base = idx
                        .checked_sub(NamedColor::DimBlack as usize)
                        .filter(|b| *b < 8);
                    base.map(|b| ANSI[b])
                }
            }
        }
    }
}

/// Answer for OSC 4/10/11 colour queries.
pub fn request_color<T>(term: &Term<T>, index: usize) -> Rgb {
    if let Some(c) = term.colors()[index] {
        return c;
    }
    let v = match index {
        0..=255 => indexed(u8::try_from(index).unwrap_or(0)),
        i if i == NamedColor::Background as usize => DEFAULT_BG,
        _ => DEFAULT_FG,
    };
    Rgb {
        r: (v >> 16) as u8,
        g: (v >> 8) as u8,
        b: v as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::indexed;

    #[test]
    fn cube_and_grays() {
        assert_eq!(indexed(16), 0x000000);
        assert_eq!(indexed(231), 0xffffff);
        assert_eq!(indexed(196), 0xff0000);
        assert_eq!(indexed(232), 0x080808);
    }
}
