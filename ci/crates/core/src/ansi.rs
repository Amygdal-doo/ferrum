//! ANSI escape sequences in job logs: SGR colours become [`Span`]s, every
//! other escape is dropped.

/// A colour from an SGR sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnsiColor {
    /// 0–7 normal, 8–15 bright.
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// A run of text drawn the same way.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Span {
    pub text: String,
    pub fg: Option<AnsiColor>,
    pub bold: bool,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
struct Style {
    fg: Option<AnsiColor>,
    bold: bool,
}

fn apply_sgr(params: &str, style: &mut Style) {
    let codes: Vec<u16> = if params.is_empty() {
        vec![0]
    } else {
        params.split(';').map(|p| p.parse().unwrap_or(0)).collect()
    };
    let mut i = 0;
    while i < codes.len() {
        match codes[i] {
            0 => *style = Style::default(),
            1 => style.bold = true,
            22 => style.bold = false,
            c @ 30..=37 => style.fg = Some(AnsiColor::Indexed((c - 30) as u8)),
            c @ 90..=97 => style.fg = Some(AnsiColor::Indexed((c - 90 + 8) as u8)),
            39 => style.fg = None,
            38 => match codes.get(i + 1) {
                Some(5) => {
                    style.fg = codes
                        .get(i + 2)
                        .and_then(|n| u8::try_from(*n).ok())
                        .map(AnsiColor::Indexed);
                    i += 2;
                }
                Some(2) => {
                    let c = |k: usize| codes.get(i + k).and_then(|n| u8::try_from(*n).ok());
                    style.fg = match (c(2), c(3), c(4)) {
                        (Some(r), Some(g), Some(b)) => Some(AnsiColor::Rgb(r, g, b)),
                        _ => None,
                    };
                    i += 4;
                }
                _ => {}
            },
            _ => {}
        }
        i += 1;
    }
}

/// Splits `line` into styled spans; escapes never reach the text.
#[must_use]
pub fn parse(line: &str) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let mut style = Style::default();
    let mut text = String::new();
    let mut chars = line.chars().peekable();
    let flush = |text: &mut String, style: Style, spans: &mut Vec<Span>| {
        if !text.is_empty() {
            spans.push(Span {
                text: std::mem::take(text),
                fg: style.fg,
                bold: style.bold,
            });
        }
    };
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            text.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                let mut params = String::new();
                let mut fin = None;
                for p in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&p) {
                        fin = Some(p);
                        break;
                    }
                    params.push(p);
                }
                if fin == Some('m') {
                    let mut next = style;
                    apply_sgr(&params, &mut next);
                    if next != style {
                        flush(&mut text, style, &mut spans);
                        style = next;
                    }
                }
            }
            // OSC: until BEL or ESC \.
            Some(']') => {
                while let Some(p) = chars.next() {
                    if p == '\u{7}' {
                        break;
                    }
                    if p == '\u{1b}' {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    flush(&mut text, style, &mut spans);
    spans
}

/// The text of `line` without escapes.
#[must_use]
pub fn strip(line: &str) -> String {
    parse(line).into_iter().map(|s| s.text).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_and_resets() {
        let spans = parse("\u{1b}[1;31merror\u{1b}[0m: bad \u{1b}[38;5;10mok\u{1b}[39m!");
        assert_eq!(spans.len(), 4);
        assert_eq!(spans[0].text, "error");
        assert_eq!(spans[0].fg, Some(AnsiColor::Indexed(1)));
        assert!(spans[0].bold);
        assert_eq!(spans[1].text, ": bad ");
        assert_eq!(spans[1].fg, None);
        assert_eq!(spans[2].fg, Some(AnsiColor::Indexed(10)));
        assert_eq!(spans[3].text, "!");
        assert_eq!(
            parse("\u{1b}[38;2;1;2;3mx")[0].fg,
            Some(AnsiColor::Rgb(1, 2, 3))
        );
        assert_eq!(parse("\u{1b}[94mx")[0].fg, Some(AnsiColor::Indexed(12)));
    }

    #[test]
    fn strips_other_escapes() {
        assert_eq!(strip("a\u{1b}[2Kb\u{1b}]0;title\u{7}c\u{1b}[m"), "abc");
        assert_eq!(strip("plain"), "plain");
        assert!(parse("").is_empty());
        assert_eq!(strip("trailing\u{1b}"), "trailing");
    }
}
