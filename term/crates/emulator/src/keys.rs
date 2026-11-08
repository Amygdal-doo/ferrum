//! Key specs (`Enter`, `C-c`, `M-f`, `S-Tab`, `C-Up`, `F5`, `x`) → xterm bytes.
//! Shared by `fterm send-key`, `fterm attach` and the GUI (which sends specs,
//! so the server encodes them against the program's current modes).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Mods {
    shift: bool,
    alt: bool,
    ctrl: bool,
}

impl Mods {
    /// xterm modifier parameter (1 = none).
    fn param(self) -> u8 {
        1 + u8::from(self.shift) + 2 * u8::from(self.alt) + 4 * u8::from(self.ctrl)
    }

    fn any(self) -> bool {
        self.shift || self.alt || self.ctrl
    }
}

fn split_mods(spec: &str) -> (Mods, &str) {
    let mut m = Mods::default();
    let mut rest = spec;
    loop {
        if rest.len() <= 2 {
            return (m, rest);
        }
        match &rest[..2] {
            "C-" | "c-" => m.ctrl = true,
            "M-" | "m-" | "A-" | "a-" => m.alt = true,
            "S-" | "s-" => m.shift = true,
            _ => return (m, rest),
        }
        rest = &rest[2..];
    }
}

/// CSI sequence with optional modifier parameter.
fn csi_letter(letter: char, m: Mods, app_cursor: bool) -> Vec<u8> {
    if m.any() {
        format!("\x1b[1;{}{letter}", m.param()).into_bytes()
    } else if app_cursor {
        format!("\x1bO{letter}").into_bytes()
    } else {
        format!("\x1b[{letter}").into_bytes()
    }
}

fn csi_tilde(n: u8, m: Mods) -> Vec<u8> {
    if m.any() {
        format!("\x1b[{n};{}~", m.param()).into_bytes()
    } else {
        format!("\x1b[{n}~").into_bytes()
    }
}

fn ctrl_char(c: char) -> Option<u8> {
    match c {
        'a'..='z' => Some(c as u8 - b'a' + 1),
        'A'..='Z' => Some(c as u8 - b'A' + 1),
        '@' | ' ' | '2' => Some(0),
        '[' | '3' => Some(0x1b),
        '\\' | '4' => Some(0x1c),
        ']' | '5' => Some(0x1d),
        '^' | '6' => Some(0x1e),
        '_' | '/' | '7' => Some(0x1f),
        '?' | '8' => Some(0x7f),
        _ => None,
    }
}

/// Encodes one key spec. `None` for unknown keys.
#[must_use]
pub fn encode(spec: &str, app_cursor: bool) -> Option<Vec<u8>> {
    let (m, key) = split_mods(spec);
    let lower = key.to_ascii_lowercase();
    let base: Vec<u8> = match lower.as_str() {
        "enter" | "return" | "cr" => b"\r".to_vec(),
        "tab" if m.shift => return Some(b"\x1b[Z".to_vec()),
        "tab" => b"\t".to_vec(),
        "btab" => return Some(b"\x1b[Z".to_vec()),
        "escape" | "esc" => b"\x1b".to_vec(),
        "space" => b" ".to_vec(),
        "backspace" | "bspace" | "bs" => b"\x7f".to_vec(),
        "up" => return Some(csi_letter('A', m, app_cursor)),
        "down" => return Some(csi_letter('B', m, app_cursor)),
        "right" => return Some(csi_letter('C', m, app_cursor)),
        "left" => return Some(csi_letter('D', m, app_cursor)),
        "home" => return Some(csi_letter('H', m, app_cursor)),
        "end" => return Some(csi_letter('F', m, app_cursor)),
        "insert" | "ic" => return Some(csi_tilde(2, m)),
        "delete" | "dc" | "del" => return Some(csi_tilde(3, m)),
        "pageup" | "pgup" | "ppage" => return Some(csi_tilde(5, m)),
        "pagedown" | "pgdn" | "npage" => return Some(csi_tilde(6, m)),
        f if f.starts_with('f') && f.len() > 1 && f[1..].chars().all(|c| c.is_ascii_digit()) => {
            let n: u8 = f[1..].parse().ok()?;
            return match n {
                1..=4 => {
                    let letter = (b'P' + n - 1) as char;
                    Some(if m.any() {
                        format!("\x1b[1;{}{letter}", m.param()).into_bytes()
                    } else {
                        format!("\x1bO{letter}").into_bytes()
                    })
                }
                5 => Some(csi_tilde(15, m)),
                6..=10 => Some(csi_tilde(n + 11, m)),
                11 | 12 => Some(csi_tilde(n + 12, m)),
                _ => None,
            };
        }
        _ => {
            let mut chars = key.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            if m.ctrl {
                vec![ctrl_char(c)?]
            } else if m.shift {
                c.to_uppercase().collect::<String>().into_bytes()
            } else {
                c.to_string().into_bytes()
            }
        }
    };
    let base = if m.ctrl && lower == "space" {
        vec![0]
    } else {
        base
    };
    Some(if m.alt {
        [b"\x1b".as_slice(), &base].concat()
    } else {
        base
    })
}

#[cfg(test)]
mod tests {
    use super::encode;

    fn e(s: &str) -> Vec<u8> {
        encode(s, false).unwrap_or_else(|| panic!("{s}"))
    }

    #[test]
    fn named_keys() {
        assert_eq!(e("Enter"), b"\r");
        assert_eq!(e("Escape"), b"\x1b");
        assert_eq!(e("S-Tab"), b"\x1b[Z");
        assert_eq!(e("BSpace"), b"\x7f");
        assert_eq!(e("PageUp"), b"\x1b[5~");
        assert_eq!(e("F1"), b"\x1bOP");
        assert_eq!(e("F5"), b"\x1b[15~");
        assert_eq!(e("F12"), b"\x1b[24~");
    }

    #[test]
    fn modifiers() {
        assert_eq!(e("C-c"), [3]);
        assert_eq!(e("C-["), [0x1b]);
        assert_eq!(e("M-f"), b"\x1bf");
        assert_eq!(e("C-Up"), b"\x1b[1;5A");
        assert_eq!(e("S-Right"), b"\x1b[1;2C");
        assert_eq!(e("C-M-x"), [0x1b, 24]);
        assert_eq!(e("C-Delete"), b"\x1b[3;5~");
        assert_eq!(e("x"), b"x");
        assert_eq!(e("C-Space"), [0]);
    }

    #[test]
    fn app_cursor_and_unknown() {
        assert_eq!(encode("Left", true), Some(b"\x1bOD".to_vec()));
        assert_eq!(encode("C-Left", true), Some(b"\x1b[1;5D".to_vec()));
        assert_eq!(encode("Hyper-q", false), None);
        assert_eq!(encode("F99", false), None);
    }
}
