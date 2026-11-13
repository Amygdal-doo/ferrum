//! The colour slots a Ferrum theme fills in.

use gpui::{HighlightStyle, Hsla};

/// Window chrome, element and editor colours.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeColors {
    // Surfaces
    pub background: Hsla,
    pub surface_background: Hsla,
    pub elevated_surface_background: Hsla,
    pub panel_background: Hsla,
    pub panel_focused_border: Hsla,
    pub pane_focused_border: Hsla,
    pub pane_group_border: Hsla,
    pub drop_target_background: Hsla,
    // Borders
    pub border: Hsla,
    pub border_variant: Hsla,
    pub border_focused: Hsla,
    pub border_selected: Hsla,
    pub border_transparent: Hsla,
    pub border_disabled: Hsla,
    // Interactive elements (filled and ghost)
    pub element_background: Hsla,
    pub element_hover: Hsla,
    pub element_active: Hsla,
    pub element_selected: Hsla,
    pub element_disabled: Hsla,
    pub element_selection_background: Hsla,
    pub ghost_element_background: Hsla,
    pub ghost_element_hover: Hsla,
    pub ghost_element_active: Hsla,
    pub ghost_element_selected: Hsla,
    pub ghost_element_disabled: Hsla,
    // Text and icons
    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_placeholder: Hsla,
    pub text_disabled: Hsla,
    pub text_accent: Hsla,
    pub link_text_hover: Hsla,
    pub icon: Hsla,
    pub icon_muted: Hsla,
    pub icon_disabled: Hsla,
    pub icon_placeholder: Hsla,
    pub icon_accent: Hsla,
    // Bars
    pub status_bar_background: Hsla,
    pub title_bar_background: Hsla,
    pub title_bar_inactive_background: Hsla,
    pub toolbar_background: Hsla,
    pub tab_bar_background: Hsla,
    pub tab_inactive_background: Hsla,
    pub tab_active_background: Hsla,
    pub search_match_background: Hsla,
    pub search_active_match_background: Hsla,
    pub scrollbar_thumb_background: Hsla,
    pub scrollbar_thumb_hover_background: Hsla,
    pub scrollbar_thumb_border: Hsla,
    pub scrollbar_track_background: Hsla,
    pub scrollbar_track_border: Hsla,
    // Editor
    pub editor_background: Hsla,
    pub editor_foreground: Hsla,
    pub editor_gutter_background: Hsla,
    pub editor_subheader_background: Hsla,
    pub editor_active_line_background: Hsla,
    pub editor_highlighted_line_background: Hsla,
    pub editor_line_number: Hsla,
    pub editor_active_line_number: Hsla,
    pub editor_invisible: Hsla,
    pub editor_wrap_guide: Hsla,
    pub editor_indent_guide: Hsla,
    pub editor_indent_guide_active: Hsla,
    // Version control
    pub version_control_added: Hsla,
    pub version_control_deleted: Hsla,
    pub version_control_modified: Hsla,
    pub version_control_renamed: Hsla,
    pub version_control_conflict: Hsla,
    pub version_control_ignored: Hsla,
    // Terminal
    pub terminal_background: Hsla,
    pub terminal_foreground: Hsla,
    pub terminal_bright_foreground: Hsla,
    pub terminal_dim_foreground: Hsla,
    pub terminal_ansi_background: Hsla,
    pub terminal_ansi_black: Hsla,
    pub terminal_ansi_red: Hsla,
    pub terminal_ansi_green: Hsla,
    pub terminal_ansi_yellow: Hsla,
    pub terminal_ansi_blue: Hsla,
    pub terminal_ansi_magenta: Hsla,
    pub terminal_ansi_cyan: Hsla,
    pub terminal_ansi_white: Hsla,
    pub terminal_ansi_bright_black: Hsla,
    pub terminal_ansi_bright_red: Hsla,
    pub terminal_ansi_bright_green: Hsla,
    pub terminal_ansi_bright_yellow: Hsla,
    pub terminal_ansi_bright_blue: Hsla,
    pub terminal_ansi_bright_magenta: Hsla,
    pub terminal_ansi_bright_cyan: Hsla,
    pub terminal_ansi_bright_white: Hsla,
    pub terminal_ansi_dim_black: Hsla,
    pub terminal_ansi_dim_red: Hsla,
    pub terminal_ansi_dim_green: Hsla,
    pub terminal_ansi_dim_yellow: Hsla,
    pub terminal_ansi_dim_blue: Hsla,
    pub terminal_ansi_dim_magenta: Hsla,
    pub terminal_ansi_dim_cyan: Hsla,
    pub terminal_ansi_dim_white: Hsla,
}

/// Declares [`StatusColors`]: each status has a foreground, a background
/// and a border colour.
macro_rules! status_colors {
    ($($fg:ident, $bg:ident, $border:ident;)*) => {
        /// Semantic colours (errors, git states, diagnostics...).
        #[derive(Debug, Clone, PartialEq)]
        pub struct StatusColors {
            $(pub $fg: Hsla, pub $bg: Hsla, pub $border: Hsla,)*
        }

        impl StatusColors {
            /// Colours every status by `color(name)`; backgrounds and
            /// borders match the foreground.
            pub(crate) fn from_fn(color: impl Fn(&str) -> Hsla) -> Self {
                $(let $fg = color(stringify!($fg));)*
                Self { $($fg, $bg: $fg, $border: $fg,)* }
            }
        }
    };
}

status_colors! {
    conflict, conflict_background, conflict_border;
    created, created_background, created_border;
    deleted, deleted_background, deleted_border;
    error, error_background, error_border;
    hidden, hidden_background, hidden_border;
    hint, hint_background, hint_border;
    ignored, ignored_background, ignored_border;
    info, info_background, info_border;
    modified, modified_background, modified_border;
    predictive, predictive_background, predictive_border;
    renamed, renamed_background, renamed_border;
    success, success_background, success_border;
    unreachable, unreachable_background, unreachable_border;
    warning, warning_background, warning_border;
}

/// Cursor and selection colours of one collaborator (Ferrum only uses the
/// local one).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerColor {
    pub cursor: Hsla,
    pub background: Hsla,
    pub selection: Hsla,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerColors(pub Vec<PlayerColor>);

impl PlayerColors {
    /// The local user's colours.
    pub fn local(&self) -> PlayerColor {
        self.0.first().copied().unwrap_or(PlayerColor {
            cursor: gpui::blue(),
            background: gpui::blue(),
            selection: gpui::blue().opacity(0.25),
        })
    }
}

/// Highlight styles by token name (`"string"`, `"comment"`, ...).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SyntaxTheme {
    styles: Vec<(String, HighlightStyle)>,
}

impl SyntaxTheme {
    pub fn new(styles: impl IntoIterator<Item = (impl Into<String>, HighlightStyle)>) -> Self {
        Self {
            styles: styles.into_iter().map(|(n, s)| (n.into(), s)).collect(),
        }
    }

    /// The style for `name`, falling back to its dotted parents
    /// (`string.special` → `string`).
    pub fn style_for_name(&self, name: &str) -> Option<HighlightStyle> {
        let mut key = name;
        loop {
            if let Some((_, style)) = self.styles.iter().find(|(n, _)| n == key) {
                return Some(*style);
            }
            key = &key[..key.rfind('.')?];
        }
    }

    /// The colour for `name`, if it has one.
    pub fn color(&self, name: &str) -> Option<Hsla> {
        self.style_for_name(name).and_then(|s| s.color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntax_falls_back_to_parent_scope() {
        let red = gpui::red();
        let theme = SyntaxTheme::new([(
            "string",
            HighlightStyle {
                color: Some(red),
                ..HighlightStyle::default()
            },
        )]);
        assert_eq!(theme.color("string.special.symbol"), Some(red));
        assert_eq!(theme.color("comment"), None);
    }
}
