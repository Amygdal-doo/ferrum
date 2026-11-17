//! Colour parsing and conversion between hex, RGB, HSL and OKLCH (CSS
//! Color 4, via Björn Ottosson's OKLab).

/// An sRGB colour with 8-bit channels and alpha in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

/// Hue in degrees `0..360`, saturation and lightness in percent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hsl {
    pub h: f32,
    pub s: f32,
    pub l: f32,
}

/// Lightness `0.0..=1.0`, chroma (about `0.0..=0.4` for sRGB colours) and
/// hue in degrees `0..360`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklch {
    pub l: f32,
    pub c: f32,
    pub h: f32,
}

fn to_linear(channel: u8) -> f32 {
    let v = f32::from(channel) / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear sRGB to an 8-bit channel, clipped to the sRGB gamut.
fn from_linear(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let v = if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (v * 255.0).round().clamp(0.0, 255.0) as u8
}

impl Rgba {
    pub fn opaque(&self) -> bool {
        self.a >= 1.0
    }

    /// `#rrggbb`, or `#rrggbbaa` when not opaque.
    pub fn to_hex(&self) -> String {
        let mut hex = format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b);
        if !self.opaque() {
            hex.push_str(&format!("{:02x}", (self.a * 255.0).round() as u8));
        }
        hex
    }

    pub fn to_rgb_string(&self) -> String {
        if self.opaque() {
            format!("rgb({}, {}, {})", self.r, self.g, self.b)
        } else {
            format!(
                "rgba({}, {}, {}, {})",
                self.r,
                self.g,
                self.b,
                trim(self.a, 3)
            )
        }
    }

    pub fn to_hsl(&self) -> Hsl {
        let r = f32::from(self.r) / 255.0;
        let g = f32::from(self.g) / 255.0;
        let b = f32::from(self.b) / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;
        let d = max - min;
        if d == 0.0 {
            return Hsl {
                h: 0.0,
                s: 0.0,
                l: l * 100.0,
            };
        }
        let s = d / (1.0 - (2.0 * l - 1.0).abs());
        let h = if max == r {
            60.0 * ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            60.0 * ((b - r) / d + 2.0)
        } else {
            60.0 * ((r - g) / d + 4.0)
        };
        Hsl {
            h,
            s: s * 100.0,
            l: l * 100.0,
        }
    }

    pub fn to_hsl_string(&self) -> String {
        let hsl = self.to_hsl();
        let (h, s, l) = (hsl.h.round(), hsl.s.round(), hsl.l.round());
        if self.opaque() {
            format!("hsl({h}, {s}%, {l}%)")
        } else {
            format!("hsla({h}, {s}%, {l}%, {})", trim(self.a, 3))
        }
    }

    pub fn to_oklch(&self) -> Oklch {
        let (r, g, b) = (to_linear(self.r), to_linear(self.g), to_linear(self.b));
        let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
        let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
        let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
        let lightness = 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s;
        let a = 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s;
        let b = 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s;
        let c = (a * a + b * b).sqrt();
        // Greys have no hue; avoid noise from rounding.
        let h = if c < 1e-4 {
            0.0
        } else {
            b.atan2(a).to_degrees().rem_euclid(360.0)
        };
        Oklch { l: lightness, c, h }
    }

    /// `oklch(62.8% 0.258 29.23)`, with `/ alpha` when not opaque.
    pub fn to_oklch_string(&self) -> String {
        let o = self.to_oklch();
        let mut text = format!(
            "oklch({}% {} {}",
            trim(o.l * 100.0, 2),
            trim(o.c, 4),
            trim(o.h, 2)
        );
        if !self.opaque() {
            text.push_str(&format!(" / {}", trim(self.a, 3)));
        }
        text.push(')');
        text
    }
}

impl Oklch {
    /// The sRGB colour; colours outside the sRGB gamut are clipped.
    pub fn to_rgba(&self, alpha: f32) -> Rgba {
        let (sin, cos) = self.h.to_radians().sin_cos();
        let (a, b) = (self.c * cos, self.c * sin);
        let l = (self.l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
        let m = (self.l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
        let s = (self.l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
        Rgba {
            r: from_linear(4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s),
            g: from_linear(-1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s),
            b: from_linear(-0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s),
            a: alpha.clamp(0.0, 1.0),
        }
    }
}

impl Hsl {
    pub fn to_rgba(&self, a: f32) -> Rgba {
        let h = self.h.rem_euclid(360.0);
        let s = (self.s / 100.0).clamp(0.0, 1.0);
        let l = (self.l / 100.0).clamp(0.0, 1.0);
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = l - c / 2.0;
        let (r, g, b) = match (h / 60.0) as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let to8 = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
        Rgba {
            r: to8(r),
            g: to8(g),
            b: to8(b),
            a: a.clamp(0.0, 1.0),
        }
    }
}

fn trim(value: f32, decimals: usize) -> String {
    let s = format!("{value:.decimals$}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

/// Parses `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa` (the `#` is optional),
/// `rgb()/rgba()`, `hsl()/hsla()` with comma or space separators, and
/// `oklch(L C H [/ A])` (L as `0.63` or `63%`).
pub fn parse(input: &str) -> Result<Rgba, String> {
    let input = input.trim().to_lowercase();
    if input.is_empty() {
        return Err("Enter a colour".into());
    }
    if let Some(args) = function_args(&input, &["oklch"]) {
        return parse_oklch(&args);
    }
    if let Some(args) = function_args(&input, &["rgba", "rgb"]) {
        return parse_rgb(&args);
    }
    if let Some(args) = function_args(&input, &["hsla", "hsl"]) {
        return parse_hsl(&args);
    }
    parse_hex(input.strip_prefix('#').unwrap_or(&input))
}

fn function_args(input: &str, names: &[&str]) -> Option<Vec<String>> {
    let name = names.iter().find(|n| input.starts_with(*n))?;
    let inner = input[name.len()..]
        .trim()
        .strip_prefix('(')?
        .strip_suffix(')')?;
    Some(
        inner
            .split([',', ' ', '/'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

fn parse_hex(hex: &str) -> Result<Rgba, String> {
    let invalid = || format!("'#{hex}' is not a hex colour");
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(invalid());
    }
    let expanded: String = match hex.len() {
        3 | 4 => hex.chars().flat_map(|c| [c, c]).collect(),
        6 | 8 => hex.to_string(),
        _ => return Err(invalid()),
    };
    let channel = |i: usize| u8::from_str_radix(&expanded[i..i + 2], 16).map_err(|_| invalid());
    let a = if expanded.len() == 8 {
        f32::from(channel(6)?) / 255.0
    } else {
        1.0
    };
    Ok(Rgba {
        r: channel(0)?,
        g: channel(2)?,
        b: channel(4)?,
        a,
    })
}

fn parse_alpha(args: &[String]) -> Result<f32, String> {
    match args.get(3) {
        None => Ok(1.0),
        Some(a) => match a.strip_suffix('%') {
            Some(pct) => number(pct).map(|v| (v / 100.0).clamp(0.0, 1.0)),
            None => number(a).map(|v| v.clamp(0.0, 1.0)),
        },
    }
}

fn number(s: &str) -> Result<f32, String> {
    s.parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("'{s}' is not a number"))
}

fn parse_rgb(args: &[String]) -> Result<Rgba, String> {
    if !(3..=4).contains(&args.len()) {
        return Err("rgb() needs 3 values (and an optional alpha)".into());
    }
    let channel = |s: &str| -> Result<u8, String> {
        let v = match s.strip_suffix('%') {
            Some(pct) => number(pct)? * 2.55,
            None => number(s)?,
        };
        if !(0.0..=255.0).contains(&v) {
            return Err(format!("'{s}' is outside 0–255"));
        }
        Ok(v.round() as u8)
    };
    Ok(Rgba {
        r: channel(&args[0])?,
        g: channel(&args[1])?,
        b: channel(&args[2])?,
        a: parse_alpha(args)?,
    })
}

fn parse_hsl(args: &[String]) -> Result<Rgba, String> {
    if !(3..=4).contains(&args.len()) {
        return Err("hsl() needs 3 values (and an optional alpha)".into());
    }
    let h = number(args[0].trim_end_matches("deg"))?;
    let pct = |s: &str| -> Result<f32, String> {
        let v = number(s.trim_end_matches('%'))?;
        if !(0.0..=100.0).contains(&v) {
            return Err(format!("'{s}' is outside 0–100%"));
        }
        Ok(v)
    };
    let hsl = Hsl {
        h,
        s: pct(&args[1])?,
        l: pct(&args[2])?,
    };
    Ok(hsl.to_rgba(parse_alpha(args)?))
}

fn parse_oklch(args: &[String]) -> Result<Rgba, String> {
    if !(3..=4).contains(&args.len()) {
        return Err("oklch() needs 3 values (and an optional alpha)".into());
    }
    let l = match args[0].strip_suffix('%') {
        Some(pct) => number(pct)? / 100.0,
        None => number(&args[0])?,
    };
    if !(0.0..=1.0).contains(&l) {
        return Err(format!("'{}' is outside 0–1 (0–100%)", args[0]));
    }
    // CSS allows chroma as a percentage of 0.4.
    let c = match args[1].strip_suffix('%') {
        Some(pct) => number(pct)? / 100.0 * 0.4,
        None => number(&args[1])?,
    };
    if c < 0.0 {
        return Err(format!("chroma '{}' is negative", args[1]));
    }
    let h = number(args[2].trim_end_matches("deg"))?;
    Ok(Oklch { l, c, h }.to_rgba(parse_alpha(args)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(r: u8, g: u8, b: u8) -> Rgba {
        Rgba { r, g, b, a: 1.0 }
    }

    #[test]
    fn parses_hex_forms() {
        assert_eq!(parse("#ff8000").unwrap(), rgb(255, 128, 0));
        assert_eq!(parse("FF8000").unwrap(), rgb(255, 128, 0));
        assert_eq!(parse("#f80").unwrap(), rgb(255, 136, 0));
        let with_alpha = parse("#ff800080").unwrap();
        assert!((with_alpha.a - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(parse("#f808").unwrap().r, 255);
    }

    #[test]
    fn parses_rgb_and_hsl_functions() {
        assert_eq!(parse("rgb(255, 128, 0)").unwrap(), rgb(255, 128, 0));
        assert_eq!(parse("rgb(255 128 0)").unwrap(), rgb(255, 128, 0));
        assert_eq!(parse("rgb(100%, 0%, 0%)").unwrap(), rgb(255, 0, 0));
        assert_eq!(parse("rgba(0, 0, 0, 0.5)").unwrap().a, 0.5);
        assert_eq!(parse("rgb(0 0 0 / 50%)").unwrap().a, 0.5);
        assert_eq!(parse("hsl(120, 100%, 50%)").unwrap(), rgb(0, 255, 0));
        assert_eq!(parse("HSL(240deg 100% 50%)").unwrap(), rgb(0, 0, 255));
        assert_eq!(parse("hsla(0, 0%, 100%, 0.25)").unwrap().a, 0.25);
    }

    #[test]
    fn formats_all_notations() {
        let orange = rgb(255, 128, 0);
        assert_eq!(orange.to_hex(), "#ff8000");
        assert_eq!(orange.to_rgb_string(), "rgb(255, 128, 0)");
        assert_eq!(orange.to_hsl_string(), "hsl(30, 100%, 50%)");
        let translucent = Rgba { a: 0.5, ..orange };
        assert_eq!(translucent.to_hex(), "#ff800080");
        assert_eq!(translucent.to_rgb_string(), "rgba(255, 128, 0, 0.5)");
        assert_eq!(translucent.to_hsl_string(), "hsla(30, 100%, 50%, 0.5)");
    }

    #[test]
    fn hsl_roundtrip_and_greys() {
        assert_eq!(rgb(128, 128, 128).to_hsl_string(), "hsl(0, 0%, 50%)");
        for color in [
            rgb(12, 200, 99),
            rgb(255, 0, 255),
            rgb(1, 2, 3),
            rgb(0, 0, 0),
        ] {
            let back = color.to_hsl().to_rgba(1.0);
            assert_eq!(back, color);
        }
    }

    #[test]
    fn oklch_matches_css_reference_values() {
        // Reference values from the CSS Color 4 conversion code.
        let red = rgb(255, 0, 0).to_oklch();
        assert!((red.l - 0.627_96).abs() < 1e-3, "{red:?}");
        assert!((red.c - 0.257_68).abs() < 1e-3, "{red:?}");
        assert!((red.h - 29.23).abs() < 0.05, "{red:?}");
        assert_eq!(
            rgb(255, 0, 0).to_oklch_string(),
            "oklch(62.8% 0.2577 29.23)"
        );
        assert_eq!(rgb(255, 255, 255).to_oklch_string(), "oklch(100% 0 0)");
        assert_eq!(rgb(0, 0, 0).to_oklch_string(), "oklch(0% 0 0)");
        let translucent = Rgba {
            a: 0.5,
            ..rgb(0, 0, 0)
        };
        assert_eq!(translucent.to_oklch_string(), "oklch(0% 0 0 / 0.5)");
    }

    #[test]
    fn oklch_round_trips_and_parses() {
        for color in [
            rgb(255, 128, 0),
            rgb(12, 200, 99),
            rgb(1, 2, 3),
            rgb(255, 255, 255),
        ] {
            assert_eq!(color.to_oklch().to_rgba(1.0), color);
        }
        assert_eq!(parse("oklch(62.8% 0.2577 29.23)").unwrap(), rgb(255, 0, 0));
        assert_eq!(
            parse("OKLCH(0.628 0.2577 29.23deg)").unwrap(),
            rgb(255, 0, 0)
        );
        assert_eq!(parse("oklch(1 0 0 / 50%)").unwrap().a, 0.5);
        // Out of gamut: clipped.
        assert_eq!(parse("oklch(0.9 0.4 145)").unwrap().g, 255);
        assert!(parse("oklch(120% 0.1 0)").is_err());
        assert!(parse("oklch(0.5 -0.1 0)").is_err());
        assert!(parse("oklch(0.5 0.1)").is_err());
    }

    #[test]
    fn errors() {
        assert!(parse("").is_err());
        assert!(parse("#12345").is_err());
        assert!(parse("#gggggg").is_err());
        assert!(parse("rgb(1, 2)").is_err());
        assert!(parse("rgb(256, 0, 0)").unwrap_err().contains("0–255"));
        assert!(parse("hsl(0, 120%, 50%)").is_err());
        assert!(parse("rgb(a, b, c)").is_err());
        assert!(parse("rgb(1, 2, 3").is_err());
    }
}
