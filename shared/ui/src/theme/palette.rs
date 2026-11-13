//! Ferrum's built-in dark and light themes, derived from the base tones in
//! [`super::tones`].
//!
//! The tones are raw; [`Theme::make_legible`] then nudges every text colour
//! to its contrast target and derives the status tints.

use gpui::{FontStyle, FontWeight, HighlightStyle, Hsla, transparent_black};

use super::colors::{PlayerColor, PlayerColors, StatusColors, SyntaxTheme, ThemeColors};
use super::tones::{Palette, ferrum_dark, ferrum_light};
use super::{Theme, ThemeStyles};

fn theme_colors(p: &Palette) -> ThemeColors {
    let blue = p.hues[0];
    let [
        black,
        a_red,
        a_green,
        a_yellow,
        a_blue,
        magenta,
        cyan,
        white,
    ] = p.ansi;
    let [
        b_black,
        b_red,
        b_green,
        b_yellow,
        b_blue,
        b_magenta,
        b_cyan,
        b_white,
    ] = p.ansi_bright;
    let [
        d_black,
        d_red,
        d_green,
        d_yellow,
        d_blue,
        d_magenta,
        d_cyan,
        d_white,
    ] = p.ansi_dim;
    let clear = transparent_black();
    ThemeColors {
        background: p.background,
        surface_background: p.background,
        elevated_surface_background: p.surface,
        panel_background: p.background,
        panel_focused_border: blue,
        pane_focused_border: blue,
        pane_group_border: p.border,
        drop_target_background: p.active,
        border: p.border,
        border_variant: p.border_variant,
        border_focused: p.accent,
        border_selected: p.accent,
        border_transparent: clear,
        border_disabled: p.border_disabled,
        element_background: p.element,
        element_hover: p.hover,
        element_active: p.active,
        element_selected: p.selected,
        element_disabled: clear,
        element_selection_background: p.selection.alpha(0.25),
        ghost_element_background: clear,
        ghost_element_hover: p.hover,
        ghost_element_active: p.active,
        ghost_element_selected: p.selected,
        ghost_element_disabled: clear,
        text: p.text,
        text_muted: p.text_muted,
        text_placeholder: p.text_placeholder,
        text_disabled: p.text_placeholder,
        text_accent: p.accent,
        link_text_hover: blue,
        icon: p.icon,
        icon_muted: p.icon_muted,
        icon_disabled: p.text_placeholder,
        icon_placeholder: p.text_placeholder,
        icon_accent: blue,
        status_bar_background: p.background,
        title_bar_background: p.background,
        title_bar_inactive_background: p.background,
        toolbar_background: p.editor,
        tab_bar_background: p.background,
        tab_inactive_background: p.background,
        tab_active_background: p.editor,
        search_match_background: p.background,
        search_active_match_background: p.background,
        scrollbar_thumb_background: clear,
        scrollbar_thumb_hover_background: p.hover,
        scrollbar_thumb_border: p.border_variant,
        scrollbar_track_background: clear,
        scrollbar_track_border: p.border_variant,
        editor_background: p.editor,
        editor_foreground: p.text,
        editor_gutter_background: p.editor,
        editor_subheader_background: p.background,
        editor_active_line_background: p.active_line,
        editor_highlighted_line_background: blue.alpha(0.1),
        editor_line_number: p.line_number,
        editor_active_line_number: p.text_muted,
        editor_invisible: p.line_number,
        editor_wrap_guide: p.border_variant,
        editor_indent_guide: p.border_variant,
        editor_indent_guide_active: p.border,
        version_control_added: p.vcs_added,
        version_control_deleted: p.vcs_deleted,
        version_control_modified: p.vcs_modified,
        version_control_renamed: p.vcs_modified,
        version_control_conflict: p.hues[4],
        version_control_ignored: p.hues[7],
        terminal_background: p.background,
        terminal_foreground: white,
        terminal_bright_foreground: b_white,
        terminal_dim_foreground: d_white,
        terminal_ansi_background: black,
        terminal_ansi_black: black,
        terminal_ansi_red: a_red,
        terminal_ansi_green: a_green,
        terminal_ansi_yellow: a_yellow,
        terminal_ansi_blue: a_blue,
        terminal_ansi_magenta: magenta,
        terminal_ansi_cyan: cyan,
        terminal_ansi_white: white,
        terminal_ansi_bright_black: b_black,
        terminal_ansi_bright_red: b_red,
        terminal_ansi_bright_green: b_green,
        terminal_ansi_bright_yellow: b_yellow,
        terminal_ansi_bright_blue: b_blue,
        terminal_ansi_bright_magenta: b_magenta,
        terminal_ansi_bright_cyan: b_cyan,
        terminal_ansi_bright_white: b_white,
        terminal_ansi_dim_black: d_black,
        terminal_ansi_dim_red: d_red,
        terminal_ansi_dim_green: d_green,
        terminal_ansi_dim_yellow: d_yellow,
        terminal_ansi_dim_blue: d_blue,
        terminal_ansi_dim_magenta: d_magenta,
        terminal_ansi_dim_cyan: d_cyan,
        terminal_ansi_dim_white: d_white,
    }
}

fn status(p: &Palette) -> StatusColors {
    let [blue, red, green, yellow, _, _, _, gray] = p.hues;
    // Tints and borders are derived from the hue by `Theme::make_legible`.
    StatusColors::from_fn(|name| {
        let hue = match name {
            "conflict" | "modified" | "warning" => yellow,
            "created" | "success" => green,
            "deleted" | "error" => red,
            "hint" | "info" | "renamed" => blue,
            _ => gray,
        };
        (hue, hue, hue)
    })
}

fn syntax(p: &Palette) -> SyntaxTheme {
    let [blue, red, green, yellow, orange, purple, teal, gray] = p.hues;
    let color = |c: Hsla| HighlightStyle {
        color: Some(c),
        ..HighlightStyle::default()
    };
    SyntaxTheme::new([
        ("attribute", color(purple)),
        ("boolean", color(orange)),
        ("comment", color(gray)),
        ("constant", color(yellow)),
        ("constructor", color(blue)),
        ("enum", color(teal)),
        ("function", color(blue)),
        ("keyword", color(purple)),
        ("link_text", color(blue)),
        ("number", color(orange)),
        ("preproc", color(purple)),
        ("property", color(red)),
        ("string", color(green)),
        ("string.regex", color(red)),
        ("type", color(teal)),
        ("variable.special", color(red)),
        ("diff.plus", color(green)),
        ("diff.minus", color(red)),
        (
            "emphasis",
            HighlightStyle {
                font_style: Some(FontStyle::Italic),
                ..HighlightStyle::default()
            },
        ),
        (
            "emphasis.strong",
            HighlightStyle {
                font_weight: Some(FontWeight::BOLD),
                ..HighlightStyle::default()
            },
        ),
    ])
}

fn build(name: &str, p: &Palette) -> Theme {
    let blue = p.hues[0];
    let mut theme = Theme {
        name: name.to_owned().into(),
        appearance: p.appearance,
        styles: ThemeStyles {
            colors: theme_colors(p),
            status: status(p),
            player: PlayerColors(vec![PlayerColor {
                cursor: blue,
                background: blue,
                selection: p.selection,
            }]),
            syntax: std::sync::Arc::new(syntax(p)),
        },
    };
    theme.make_legible();
    theme
}

/// The default theme.
pub fn dark() -> Theme {
    build("Ferrum Dark", &ferrum_dark())
}

pub fn light() -> Theme {
    build("Ferrum Light", &ferrum_light())
}
