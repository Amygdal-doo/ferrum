//! Text/background contrast following the WCAG 2.x relative-luminance
//! formula, and helpers that pick or nudge colours until text is readable.
//!
//! Rule of thumb used across Ferrum:
//! - body and label text needs [`MIN_TEXT_CONTRAST`] (4.5:1) against what it
//!   sits on;
//! - text on a *filled* colour (chips, badges, primary buttons) uses
//!   [`readable_text_on`], which picks near-black or near-white, whichever
//!   contrasts more;
//! - text on a *tinted* surface keeps its hue and is moved with
//!   [`ensure_contrast`] until it reaches 4.5:1.

use gpui::{Hsla, hsla};

/// Minimum contrast for normal text (WCAG AA).
pub const MIN_TEXT_CONTRAST: f32 = 4.5;
/// Minimum contrast for large text and non-text UI such as placeholders and
/// borders that carry meaning (WCAG AA).
pub const MIN_UI_CONTRAST: f32 = 3.0;

/// The dark text [`readable_text_on`] puts on light fills.
pub fn dark_text() -> Hsla {
    hsla(220. / 360., 0.15, 0.06, 1.)
}

/// The light text [`readable_text_on`] puts on dark fills.
pub fn light_text() -> Hsla {
    hsla(0., 0., 0.99, 1.)
}

/// `color` painted over `base`: the opaque colour the eye actually sees.
pub fn composite(color: Hsla, base: Hsla) -> Hsla {
    if color.a >= 1. {
        return color;
    }
    let opaque_base = Hsla { a: 1., ..base };
    Hsla {
        a: 1.,
        ..opaque_base.blend(color)
    }
}

/// WCAG relative luminance (0 = black, 1 = white); alpha is ignored.
pub fn relative_luminance(color: Hsla) -> f32 {
    fn linear(channel: f32) -> f32 {
        if channel <= 0.040_45 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }
    let rgb = color.to_rgb();
    0.2126 * linear(rgb.r) + 0.7152 * linear(rgb.g) + 0.0722 * linear(rgb.b)
}

/// WCAG contrast ratio between two opaque colours, from 1 (none) to 21
/// (black on white). Symmetric in its arguments.
pub fn contrast_ratio(a: Hsla, b: Hsla) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Contrast of `fg` drawn on `bg`, where either may be translucent: `bg` is
/// composited over `base` and `fg` over the result.
pub fn contrast_over(fg: Hsla, bg: Hsla, base: Hsla) -> f32 {
    let bg = composite(bg, base);
    contrast_ratio(composite(fg, bg), bg)
}

/// Near-black or near-white, whichever reads better on the opaque `bg`.
///
/// That is at least 4.5:1 on any fill except a narrow band of mid-tones
/// (relative luminance around 0.18) where neither reaches it; the worst case
/// is about 4.4:1. Theme fills are kept out of that band (see the theme
/// tests).
pub fn readable_text_on(bg: Hsla) -> Hsla {
    let (dark, light) = (dark_text(), light_text());
    if contrast_ratio(dark, bg) >= contrast_ratio(light, bg) {
        dark
    } else {
        light
    }
}

/// [`readable_text_on`] for a possibly translucent `bg` drawn over `base`.
pub fn readable_text_over(bg: Hsla, base: Hsla) -> Hsla {
    readable_text_on(composite(bg, base))
}

/// `fg` with its lightness moved away from `bg` (keeping hue and
/// saturation) until the pair reaches `min` contrast. Returns `fg` unchanged
/// when it already does; falls back to [`readable_text_on`] if no lightness
/// of that hue gets there.
pub fn ensure_contrast(fg: Hsla, bg: Hsla, min: f32) -> Hsla {
    let bg = composite(bg, hsla(0., 0., 0., 1.));
    let fg = composite(fg, bg);
    if contrast_ratio(fg, bg) >= min {
        return fg;
    }
    // Lighten on dark backgrounds, darken on light ones.
    let lighten = contrast_ratio(light_text(), bg) >= contrast_ratio(dark_text(), bg);
    let mut candidate = fg;
    for _ in 0..100 {
        candidate.l = if lighten {
            (candidate.l + 0.01).min(1.)
        } else {
            (candidate.l - 0.01).max(0.)
        };
        if contrast_ratio(candidate, bg) >= min {
            return candidate;
        }
    }
    readable_text_on(bg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gray(l: f32) -> Hsla {
        hsla(0., 0., l, 1.)
    }

    #[test]
    fn black_on_white_is_21_to_1() {
        let ratio = contrast_ratio(gray(0.), gray(1.));
        assert!((ratio - 21.).abs() < 0.01, "{ratio}");
        assert!((contrast_ratio(gray(1.), gray(0.)) - ratio).abs() < f32::EPSILON);
        assert!((contrast_ratio(gray(0.5), gray(0.5)) - 1.).abs() < f32::EPSILON);
    }

    #[test]
    fn known_reference_ratio() {
        // #777777 on white is the classic 4.48:1 just-fails-AA pair.
        let ratio = contrast_ratio(gpui::rgb(0x777777).into(), gray(1.));
        assert!((ratio - 4.48).abs() < 0.02, "{ratio}");
    }

    #[test]
    fn picks_dark_text_on_light_fills_and_light_on_dark() {
        let yellow: Hsla = gpui::rgb(0xf5e147).into();
        let navy: Hsla = gpui::rgb(0x102040).into();
        assert_eq!(readable_text_on(yellow), dark_text());
        assert_eq!(readable_text_on(navy), light_text());
        for l in 0..=40 {
            let bg = hsla(0.6, 0.7, l as f32 / 40., 1.);
            let ratio = contrast_ratio(readable_text_on(bg), bg);
            assert!(ratio >= 4.3, "{bg:?}: {ratio}");
        }
    }

    #[test]
    fn translucent_backgrounds_are_composited_over_the_base() {
        let base = gray(0.1);
        let veil = gray(1.).opacity(0.0);
        assert_eq!(composite(veil, base).l, base.l);
        assert_eq!(readable_text_over(veil, base), light_text());
        let half = composite(gray(1.).opacity(0.5), gray(0.));
        assert!((half.l - 0.5).abs() < 0.01, "{half:?}");
    }

    #[test]
    fn ensure_contrast_keeps_hue_and_reaches_target() {
        let bg = gray(0.15);
        let dim_red = hsla(0., 0.6, 0.3, 1.);
        let fixed = ensure_contrast(dim_red, bg, MIN_TEXT_CONTRAST);
        assert!(contrast_ratio(fixed, bg) >= MIN_TEXT_CONTRAST);
        assert_eq!((fixed.h, fixed.s), (dim_red.h, dim_red.s));
        assert!(fixed.l > dim_red.l);

        let light_bg = gray(0.95);
        let pale = hsla(0.15, 0.8, 0.7, 1.);
        let fixed = ensure_contrast(pale, light_bg, MIN_TEXT_CONTRAST);
        assert!(contrast_ratio(fixed, light_bg) >= MIN_TEXT_CONTRAST);
        assert!(fixed.l < pale.l);

        let good = gray(0.9);
        assert_eq!(ensure_contrast(good, bg, MIN_TEXT_CONTRAST), good);
    }
}
