//! The bundled Material colour themes (Default, Darker, Palenight, Ocean,
//! Deep Ocean, Lighter): one small palette per variant — surfaces, text,
//! accent and the Material syntax hues — from which every UI, syntax and
//! terminal token is derived, so the six stay consistent.

use super::{Color, Section, Theme};

/// What distinguishes one Material variant from another.
struct Palette {
    name: &'static str,
    dark: bool,
    /// Editor background.
    background: &'static str,
    /// Sidebar, tabs, status bar (a shade off the editor).
    contrast: &'static str,
    /// Popups and inputs.
    second: &'static str,
    foreground: &'static str,
    /// Secondary text (comments use their own colour).
    text: &'static str,
    selection: &'static str,
    highlight: &'static str,
    line_numbers: &'static str,
    border: &'static str,
    comment: &'static str,
    accent: &'static str,
    syntax: &'static Hues,
}

/// The Material syntax hues.
struct Hues {
    red: &'static str,
    orange: &'static str,
    yellow: &'static str,
    green: &'static str,
    cyan: &'static str,
    blue: &'static str,
    purple: &'static str,
    brown: &'static str,
}

const DARK_HUES: Hues = Hues {
    red: "#f07178",
    orange: "#f78c6c",
    yellow: "#ffcb6b",
    green: "#c3e88d",
    cyan: "#89ddff",
    blue: "#82aaff",
    purple: "#c792ea",
    brown: "#c17e70",
};

const LIGHT_HUES: Hues = Hues {
    red: "#e53935",
    orange: "#f76d47",
    yellow: "#e2931d",
    green: "#91b859",
    cyan: "#39adb5",
    blue: "#6182b8",
    purple: "#7c4dff",
    brown: "#8c6c62",
};

const PALETTES: &[Palette] = &[
    Palette {
        name: "Material Default",
        dark: true,
        background: "#263238",
        contrast: "#1e272c",
        second: "#2e3c43",
        foreground: "#eeffff",
        text: "#b0bec5",
        selection: "#80cbc433",
        highlight: "#425b67",
        line_numbers: "#37474f",
        border: "#2a373e",
        comment: "#546e7a",
        accent: "#80cbc4",
        syntax: &DARK_HUES,
    },
    Palette {
        name: "Material Darker",
        dark: true,
        background: "#212121",
        contrast: "#1a1a1a",
        second: "#292929",
        foreground: "#eeffff",
        text: "#b0bec5",
        selection: "#61616166",
        highlight: "#3f3f3f",
        line_numbers: "#424242",
        border: "#292929",
        comment: "#545454",
        accent: "#ff9800",
        syntax: &DARK_HUES,
    },
    Palette {
        name: "Material Palenight",
        dark: true,
        background: "#292d3e",
        contrast: "#202331",
        second: "#34324a",
        foreground: "#a6accd",
        text: "#676e95",
        selection: "#717cb450",
        highlight: "#444267",
        line_numbers: "#3a3f58",
        border: "#2b2a3e",
        comment: "#676e95",
        accent: "#ab47bc",
        syntax: &DARK_HUES,
    },
    Palette {
        name: "Material Ocean",
        dark: true,
        background: "#1a1c25",
        contrast: "#13151d",
        second: "#232634",
        foreground: "#a6accd",
        text: "#717cb4",
        selection: "#717cb450",
        highlight: "#2b2f42",
        line_numbers: "#3b3f51",
        border: "#20222d",
        comment: "#4b526d",
        accent: "#84ffff",
        syntax: &DARK_HUES,
    },
    Palette {
        name: "Material Deep Ocean",
        dark: true,
        background: "#0f111a",
        contrast: "#090b10",
        second: "#1a1c25",
        foreground: "#8f93a2",
        text: "#4b526d",
        selection: "#717cb450",
        highlight: "#1f2233",
        line_numbers: "#3b3f51",
        border: "#0f111a",
        comment: "#464b5d",
        accent: "#84ffff",
        syntax: &DARK_HUES,
    },
    Palette {
        name: "Material Lighter",
        dark: false,
        background: "#fafafa",
        contrast: "#eeeeee",
        second: "#ffffff",
        foreground: "#546e7a",
        text: "#94a7b0",
        selection: "#80cbc440",
        highlight: "#e7e7e8",
        line_numbers: "#cfd8dc",
        border: "#d3e1e8",
        comment: "#90a4ae",
        accent: "#00bcd4",
        syntax: &LIGHT_HUES,
    },
];

/// Names of the bundled Material themes.
pub const MATERIAL_THEMES: [&str; 6] = [
    "Material Default",
    "Material Darker",
    "Material Palenight",
    "Material Ocean",
    "Material Deep Ocean",
    "Material Lighter",
];

fn color(hex: &str) -> Color {
    Color::parse(hex).unwrap_or(Color::from_rgba(0xff00ffff))
}

/// `color` at `alpha` (0–255).
fn alpha(hex: &str, alpha: u8) -> Color {
    Color {
        a: alpha,
        ..color(hex)
    }
}

/// A bundled Material theme by name (case-insensitive).
pub fn material(name: &str) -> Option<Theme> {
    let p = PALETTES
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name.trim()))?;
    let h = p.syntax;
    let mut theme = Theme::base(p.name, p.dark);
    let bg = color(p.background);
    let fg = color(p.foreground);
    let ui: &[(&str, Color)] = &[
        ("background", color(p.contrast)),
        ("panel", color(p.contrast)),
        ("surface", color(p.second)),
        ("editor", bg),
        ("gutter", bg),
        ("border", color(p.border)),
        ("border_focused", color(p.accent)),
        ("text", fg),
        ("text_muted", color(p.text)),
        ("text_placeholder", color(p.comment)),
        ("accent", color(p.accent)),
        ("element_hover", color(p.highlight)),
        (
            "element_selected",
            color(p.highlight).mix(color(p.accent), 0.15),
        ),
        ("tab_active", bg),
        ("tab_inactive", color(p.contrast)),
        ("tab_bar", color(p.contrast)),
        ("status_bar", color(p.contrast)),
        ("title_bar", color(p.contrast)),
        ("scrollbar", alpha(p.text, 0x33)),
        ("selection", color(p.selection)),
        ("cursor", color(if p.dark { "#ffcc00" } else { "#272727" })),
        (
            "current_line",
            alpha(if p.dark { "#000000" } else { "#cccccc" }, 0x40),
        ),
        ("line_number", color(p.line_numbers)),
        ("line_number_active", color(p.text)),
        ("indent_guide", alpha(p.line_numbers, 0x70)),
        ("indent_guide_active", color(p.line_numbers)),
        ("bracket_match", color(p.accent)),
        ("find_match", alpha(p.accent, 0x30)),
        ("find_match_active", alpha(p.accent, 0x70)),
        ("git_added", color(h.green)),
        ("git_modified", color(h.blue)),
        ("git_deleted", color(h.red)),
        ("error", color(if p.dark { "#ff5370" } else { "#e53935" })),
        ("warning", color(h.yellow)),
        ("success", color(h.green)),
    ];
    for (token, value) in ui {
        theme.set(Section::Ui, token, *value);
    }
    let syntax: &[(&str, &str)] = &[
        ("attribute", h.yellow),
        ("boolean", h.orange),
        ("character", h.green),
        ("comment", p.comment),
        ("constant", h.orange),
        ("constructor", h.yellow),
        ("embedded", p.foreground),
        ("escape", h.cyan),
        ("function", h.blue),
        ("function.macro", h.cyan),
        ("keyword", h.purple),
        ("label", h.purple),
        ("module", h.yellow),
        ("number", h.orange),
        ("operator", h.cyan),
        ("property", h.red),
        ("punctuation", h.cyan),
        ("punctuation.special", h.cyan),
        ("string", h.green),
        ("string.special", h.cyan),
        ("tag", h.red),
        ("type", h.yellow),
        ("variable", p.foreground),
        ("variable.builtin", h.red),
        ("variable.parameter", h.orange),
        ("text.title", h.cyan),
        ("markup.heading", h.cyan),
        ("text.literal", h.green),
        ("markup.raw", h.green),
        ("text.uri", h.blue),
        ("markup.link", h.blue),
        ("text.reference", h.purple),
        ("text.emphasis", h.purple),
        ("markup.italic", h.purple),
        ("text.strong", h.yellow),
        ("markup.bold", h.yellow),
        ("markup.list", h.brown),
        ("markup.quote", p.comment),
    ];
    for (token, hex) in syntax {
        theme.set(Section::Syntax, token, color(hex));
    }
    let (black, white) = if p.dark {
        ("#546e7a", "#ffffff")
    } else {
        ("#000000", "#ffffff")
    };
    let terminal: &[(&str, Color)] = &[
        ("background", bg),
        ("foreground", fg),
        ("cursor", color(if p.dark { "#ffcc00" } else { "#272727" })),
        ("selection", color(p.selection)),
        ("black", color(black)),
        ("red", color(h.red)),
        ("green", color(h.green)),
        ("yellow", color(h.yellow)),
        ("blue", color(h.blue)),
        ("magenta", color(h.purple)),
        ("cyan", color(h.cyan)),
        ("white", color(white)),
        ("bright_black", color(p.comment)),
        ("bright_red", color(h.red).mix(color(white), 0.2)),
        ("bright_green", color(h.green).mix(color(white), 0.2)),
        ("bright_yellow", color(h.yellow).mix(color(white), 0.2)),
        ("bright_blue", color(h.blue).mix(color(white), 0.2)),
        ("bright_magenta", color(h.purple).mix(color(white), 0.2)),
        ("bright_cyan", color(h.cyan).mix(color(white), 0.2)),
        ("bright_white", color(white)),
    ];
    for (token, value) in terminal {
        theme.set(Section::Terminal, token, *value);
    }
    Some(theme)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{TERMINAL_TOKENS, UI_TOKENS};

    #[test]
    fn every_variant_defines_every_token() {
        assert_eq!(PALETTES.len(), MATERIAL_THEMES.len());
        for name in MATERIAL_THEMES {
            let theme = material(name).expect("material theme");
            assert_eq!(theme.name, name);
            for token in UI_TOKENS {
                assert_ne!(
                    theme.ui(token),
                    Color::from_rgba(0xff00ffff),
                    "{name}.{token}"
                );
            }
            for token in TERMINAL_TOKENS {
                assert!(theme.tokens(Section::Terminal).any(|(t, _)| t == *token));
            }
            assert!(theme.syntax("keyword").is_some());
        }
        assert!(material("material lighter").is_some_and(|t| !t.dark));
        assert!(material("Material Palenight").is_some_and(|t| t.dark));
        assert!(material("Material Nope").is_none());
    }

    #[test]
    fn variants_share_hues_but_not_surfaces() {
        let default = material("Material Default").expect("theme");
        let darker = material("Material Darker").expect("theme");
        assert_eq!(default.syntax("keyword"), darker.syntax("keyword"));
        assert_ne!(default.ui("editor"), darker.ui("editor"));
        assert_eq!(default.ui("editor").to_hex(), "#263238");
        assert_eq!(
            default.syntax("string").map(Color::to_hex).as_deref(),
            Some("#c3e88d")
        );
    }
}
