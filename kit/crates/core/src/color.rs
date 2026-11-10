//! Colour parsing and conversion between hex, RGB and HSL.

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
/// `rgb()/rgba()` and `hsl()/hsla()` with comma or space separators.
pub fn parse(input: &str) -> Result<Rgba, String> {
    let input = input.trim().to_lowercase();
    if input.is_empty() {
        return Err("Enter a colour".into());
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
