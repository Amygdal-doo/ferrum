//! Bridges the editor's token theme to the GPUI/`ui` theme so the window
//! chrome (panels, tabs, buttons, inputs, terminal) follows the active
//! theme and the user's overrides.

use std::sync::Arc;

use ferrum_ui::theme::{ActiveTheme, Appearance, GlobalTheme};
use ferrumeditor_core::theme::{Color, Theme};
use gpui::{App, Hsla, Rgba};

pub fn hsla(color: Color) -> Hsla {
    Rgba {
        r: f32::from(color.r) / 255.0,
        g: f32::from(color.g) / 255.0,
        b: f32::from(color.b) / 255.0,
        a: f32::from(color.a) / 255.0,
    }
    .into()
}

/// A UI token as GPUI colour.
pub fn ui(theme: &Theme, token: &str) -> Hsla {
    hsla(theme.ui(token))
}

pub fn apply(theme: &Theme, cx: &mut App) {
    let mut base = (**cx.theme()).clone();
    let t = |token: &str| ui(theme, token);
    base.appearance = if theme.dark {
        Appearance::Dark
    } else {
        Appearance::Light
    };
    base.name = theme.name.clone().into();
    let c = &mut base.styles.colors;
    c.background = t("background");
    c.surface_background = t("panel");
    c.elevated_surface_background = t("surface");
    c.panel_background = t("panel");
    c.editor_background = t("editor");
    c.editor_foreground = t("text");
    c.editor_gutter_background = t("gutter");
    c.editor_line_number = t("line_number");
    c.editor_active_line_number = t("line_number_active");
    c.editor_active_line_background = t("current_line");
    c.toolbar_background = t("editor");
    c.text = t("text");
    c.text_muted = t("text_muted");
    c.text_placeholder = t("text_placeholder");
    c.text_disabled = t("text_placeholder");
    c.text_accent = t("accent");
    c.icon = t("text_muted");
    c.icon_muted = t("text_placeholder");
    c.icon_accent = t("accent");
    c.border = t("border");
    c.border_variant = t("border");
    c.border_focused = t("border_focused");
    c.border_selected = t("accent");
    c.panel_focused_border = t("border_focused");
    c.pane_focused_border = t("border_focused");
    c.element_background = t("element_hover");
    c.element_hover = t("element_hover");
    c.element_active = t("element_selected");
    c.element_selected = t("element_selected");
    c.ghost_element_hover = t("element_hover");
    c.ghost_element_active = t("element_selected");
    c.ghost_element_selected = t("element_selected");
    c.title_bar_background = t("title_bar");
    c.title_bar_inactive_background = t("title_bar");
    c.status_bar_background = t("status_bar");
    c.tab_bar_background = t("tab_bar");
    c.tab_active_background = t("tab_active");
    c.tab_inactive_background = t("tab_inactive");
    c.scrollbar_thumb_background = t("scrollbar");
    c.search_match_background = t("find_match");
    c.version_control_added = t("git_added");
    c.version_control_modified = t("git_modified");
    c.version_control_deleted = t("git_deleted");
    c.element_selection_background = t("selection");
    let term = |token: &str| hsla(theme.terminal(token));
    c.terminal_background = term("background");
    c.terminal_ansi_background = term("background");
    c.terminal_foreground = term("foreground");
    c.terminal_bright_foreground = term("bright_white");
    c.terminal_ansi_black = term("black");
    c.terminal_ansi_red = term("red");
    c.terminal_ansi_green = term("green");
    c.terminal_ansi_yellow = term("yellow");
    c.terminal_ansi_blue = term("blue");
    c.terminal_ansi_magenta = term("magenta");
    c.terminal_ansi_cyan = term("cyan");
    c.terminal_ansi_white = term("white");
    c.terminal_ansi_bright_black = term("bright_black");
    c.terminal_ansi_bright_red = term("bright_red");
    c.terminal_ansi_bright_green = term("bright_green");
    c.terminal_ansi_bright_yellow = term("bright_yellow");
    c.terminal_ansi_bright_blue = term("bright_blue");
    c.terminal_ansi_bright_magenta = term("bright_magenta");
    c.terminal_ansi_bright_cyan = term("bright_cyan");
    c.terminal_ansi_bright_white = term("bright_white");
    let status = &mut base.styles.status;
    status.error = t("error");
    status.warning = t("warning");
    status.success = t("success");
    if let Some(player) = base.styles.player.0.first_mut() {
        player.cursor = t("cursor");
        player.selection = t("selection");
    }
    // Keep text readable whatever the token theme says, and rebuild the
    // status tints for this theme's background.
    base.make_legible();
    GlobalTheme::update_theme(cx, Arc::new(base));
    cx.refresh_windows();
}

/// Switches between the built-in `ferrum-dark` / `ferrum-light` to match
/// the suite's appearance, unless the user configured another theme.
pub fn follow_appearance(appearance: Appearance, cx: &mut App) {
    let settings = cx.global_mut::<crate::settings::Settings>();
    let configured = settings.config.theme.as_str();
    if !matches!(configured, "ferrum-dark" | "ferrum-light") {
        return;
    }
    let wanted = if appearance.is_light() {
        "ferrum-light"
    } else {
        "ferrum-dark"
    };
    if settings.theme_name() != wanted && settings.set_theme(wanted) {
        let theme = settings.theme.clone();
        apply(&theme, cx);
    }
}
