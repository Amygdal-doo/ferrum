//! File and folder icons from the active icon theme, as GPUI elements.
//! Used by the tree view, tabs, the file finder and find in files.

use std::path::Path;
use std::sync::OnceLock;

use ferrumeditor_core::icons::{EntryKind, Icon, IconTheme};
use gpui::{AnyElement, App, IntoElement, Styled, px, svg};

use crate::settings::Settings;

/// Rendered icon size.
pub const ICON_SIZE: f32 = 14.0;

fn builtin() -> &'static IconTheme {
    static BUILTIN: OnceLock<IconTheme> = OnceLock::new();
    BUILTIN.get_or_init(IconTheme::builtin)
}

/// The icon for `path` (the active theme, falling back to the built-in
/// set), or `None` when icons are hidden.
pub fn icon_for(path: &Path, kind: EntryKind, settings: &Settings) -> Option<Icon> {
    if !settings.config.show_file_icons {
        return None;
    }
    settings
        .icon_theme
        .resolve_path(path, kind)
        .or_else(|| builtin().resolve_path(path, kind))
        .cloned()
}

/// Shorthand for files.
pub fn file_kind(is_dir: bool, expanded: bool) -> EntryKind {
    if is_dir {
        EntryKind::Folder { open: expanded }
    } else {
        EntryKind::File
    }
}

/// The icon element for `path`, tinted by the icon theme (or the theme's
/// muted text colour); `None` when icons are hidden.
pub fn icon_element(path: &Path, kind: EntryKind, cx: &App) -> Option<AnyElement> {
    let settings = cx.global::<Settings>();
    let icon = icon_for(path, kind, settings)?;
    let color = icon
        .color
        .unwrap_or_else(|| settings.theme.ui("text_muted"));
    Some(
        svg()
            .data(&icon.svg)
            .flex_none()
            .size(px(ICON_SIZE))
            .text_color(crate::theme_apply::hsla(color))
            .into_any_element(),
    )
}
