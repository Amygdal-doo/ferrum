//! ANSI escape handling for container logs: strip everything, or keep SGR
//! colours as styled ranges the UI can render.

use std::ops::Range;

/// One of the 16 basic terminal colours (0–7 normal, 8–15 bright).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnsiColor(pub u8);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AnsiStyle {
    pub fg: Option<AnsiColor>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
}

impl AnsiStyle {
    fn is_plain(&self) -> bool {
        *self == Self::default()
    }

    fn apply(&mut self, params: &[u16]) {
        let params = if params.is_empty() { &[0][..] } else { params };
        let mut i = 0;
        while i < params.len() {
            match params[i] {
                0 => *self = Self::default(),
                1 => self.bold = true,
                2 => self.dim = true,
                3 => self.italic = true,
                4 => self.underline = true,
                22 => {
                    self.bold = false;
                    self.dim = false;
                }
                23 => self.italic = false,
                24 => self.underline = false,
                n @ 30..=37 => self.fg = Some(AnsiColor((n - 30) as u8)),
                39 => self.fg = None,
                n @ 90..=97 => self.fg = Some(AnsiColor((n - 90 + 8) as u8)),
                // 256-colour / true-colour: map the low 16 indexes, drop the rest.
                38 => match params.get(i + 1) {
                    Some(5) => {
                        if let Some(&n) = params.get(i + 2) {
                            self.fg = (n < 16).then_some(AnsiColor(n as u8));
                        }
                        i += 2;
                    }
                    Some(2) => {
                        self.fg = None;
                        i += 4;
                    }
                    _ => {}
                },
                48 => match params.get(i + 1) {
                    Some(5) => i += 2,
                    Some(2) => i += 4,
                    _ => {}
                },
                _ => {}
            }
            i += 1;
        }
    }
}

/// Text without escapes plus byte ranges (into that text) with a non-default style.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyledLine {
    pub text: String,
    pub spans: Vec<(Range<usize>, AnsiStyle)>,
}

/// Parses SGR colour codes and drops every other escape sequence
/// (cursor movement, OSC titles/hyperlinks, carriage returns).
#[must_use]
pub fn parse(input: &str) -> StyledLine {
    let mut out = StyledLine::default();
    let mut style = AnsiStyle::default();
    let mut span_start = 0;
    let mut chars = input.char_indices().peekable();

    while let Some((_, c)) = chars.next() {
        match c {
            '\u{1b}' => match chars.peek().map(|&(_, c)| c) {
                Some('[') => {
                    chars.next();
                    let mut body = String::new();
                    let mut final_byte = None;
                    for (_, c) in chars.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&c) {
                            final_byte = Some(c);
                            break;
                        }
                        body.push(c);
                    }
                    if final_byte == Some('m') {
                        let params: Vec<u16> = body
                            .split([';', ':'])
                            .map(|p| p.parse().unwrap_or(0))
                            .collect();
                        let mut next = style;
                        next.apply(if body.is_empty() { &[] } else { &params });
                        if next != style {
                            close_span(&mut out, style, span_start);
                            span_start = out.text.len();
                            style = next;
                        }
                    }
                }
                Some(']') => {
                    // OSC: until BEL or ESC \.
                    chars.next();
                    while let Some((_, c)) = chars.next() {
                        if c == '\u{7}' {
                            break;
                        }
                        if c == '\u{1b}' && chars.peek().map(|&(_, c)| c) == Some('\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                Some(_) => {
                    chars.next();
                }
                None => {}
            },
            '\r' => {}
            c if c.is_control() && c != '\t' => {}
            c => out.text.push(c),
        }
    }
    close_span(&mut out, style, span_start);
    out
}

fn close_span(out: &mut StyledLine, style: AnsiStyle, start: usize) {
    let end = out.text.len();
    if end > start && !style.is_plain() {
        out.spans.push((start..end, style));
    }
}

/// Text without any escape sequences.
#[must_use]
pub fn strip(input: &str) -> String {
    if !input.contains(['\u{1b}', '\r']) && !input.chars().any(|c| c.is_control() && c != '\t') {
        return input.to_owned();
    }
    parse(input).text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_colours_and_other_escapes() {
        assert_eq!(strip("plain"), "plain");
        assert_eq!(strip("\u{1b}[31mred\u{1b}[0m text"), "red text");
        assert_eq!(strip("\u{1b}[2K\u{1b}[1Gprogress\r"), "progress");
        assert_eq!(
            strip("\u{1b}]8;;https://x.dev\u{1b}\\link\u{1b}]8;;\u{1b}\\"),
            "link"
        );
        assert_eq!(strip("tab\there\u{7}"), "tab\there");
    }

    #[test]
    fn keeps_sgr_styles_as_ranges() {
        let line = parse("\u{1b}[1;32mINFO\u{1b}[0m ready \u{1b}[91merr\u{1b}[39m!");
        assert_eq!(line.text, "INFO ready err!");
        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].0, 0..4);
        assert_eq!(line.spans[0].1.fg, Some(AnsiColor(2)));
        assert!(line.spans[0].1.bold);
        assert_eq!(line.spans[1].0, 11..14);
        assert_eq!(line.spans[1].1.fg, Some(AnsiColor(9)));
    }

    #[test]
    fn handles_extended_colours_and_unterminated_sequences() {
        let line = parse("\u{1b}[38;5;3mamber\u{1b}[38;2;1;2;3mrgb\u{1b}[m done");
        assert_eq!(line.text, "amberrgb done");
        assert_eq!(
            line.spans[0],
            (
                0..5,
                AnsiStyle {
                    fg: Some(AnsiColor(3)),
                    ..AnsiStyle::default()
                }
            )
        );
        assert_eq!(parse("cut \u{1b}[31").text, "cut ");
        assert_eq!(parse("ünïcödé \u{1b}[4mü\u{1b}[24m").spans[0].0, 12..14);
    }
}
