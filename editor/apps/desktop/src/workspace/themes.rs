//! Theme and icon theme commands: the selectors (live preview while the
//! selection moves, Enter applies and saves to config.toml, Escape
//! reverts), installing a theme file, opening the theme folders, reload.

use std::path::{Path, PathBuf};

use ferrum_ui::prelude::*;
use ferrumeditor_core::config::set_string_value;
use ferrumeditor_core::icons::{EntryKind, IconTheme};
use ferrumeditor_core::theme::registry::{self, ThemeSource};
use gpui::PathPromptOptions;

use super::Workspace;
use crate::picker::{PickerItem, PickerKind};
use crate::settings::Settings;

/// Paths whose icons illustrate an icon theme in its selector row.
const SAMPLES: &[(&str, EntryKind)] = &[
    ("src", EntryKind::Folder { open: false }),
    ("main.rs", EntryKind::File),
    ("Cargo.toml", EntryKind::File),
    ("package.json", EntryKind::File),
    ("index.ts", EntryKind::File),
    ("README.md", EntryKind::File),
    ("logo.png", EntryKind::File),
];

impl Workspace {
    /// Items and placeholder for the theme / icon theme selectors.
    pub(super) fn theme_items(kind: PickerKind, cx: &App) -> (Vec<PickerItem>, &'static str) {
        let settings = cx.global::<Settings>();
        if kind == PickerKind::Theme {
            let items = settings
                .themes
                .entries()
                .into_iter()
                .map(|entry| {
                    let appearance = if entry.dark { "dark" } else { "light" };
                    let detail = match &entry.source {
                        ThemeSource::Builtin => format!("built-in · {appearance}"),
                        ThemeSource::File(path) => format!(
                            "{appearance} · {}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ),
                    };
                    PickerItem::new(entry.name.clone(), detail, entry.name.clone())
                })
                .collect();
            return (items, "Select a theme (↑↓ to preview)…");
        }
        let builtin = IconTheme::builtin();
        let items = settings
            .icon_themes
            .names()
            .into_iter()
            .filter_map(|name| settings.icon_themes.get(&name))
            .map(|theme| {
                let detail = match &theme.dir {
                    None => "built-in".to_string(),
                    Some(dir) => format!(
                        "icon-themes/{}",
                        dir.file_name().unwrap_or_default().to_string_lossy()
                    ),
                };
                let mut item = PickerItem::new(theme.name.clone(), detail, theme.name.clone());
                item.samples = SAMPLES
                    .iter()
                    .filter_map(|(name, kind)| {
                        theme
                            .resolve_path(Path::new(name), *kind)
                            .or_else(|| builtin.resolve_path(Path::new(name), *kind))
                            .cloned()
                    })
                    .collect();
                item
            })
            .collect();
        (items, "Select an icon theme (↑↓ to preview)…")
    }

    /// Remembers what to restore if the selector is dismissed.
    pub(super) fn begin_theme_preview(&mut self, kind: PickerKind, cx: &App) -> String {
        let settings = cx.global::<Settings>();
        let current = if kind == PickerKind::Theme {
            settings.theme_name()
        } else {
            settings.icon_theme_name()
        };
        self.theme_preview = Some((kind, current.clone()));
        current
    }

    /// Applies a theme or icon theme without saving it.
    pub(super) fn preview_theme(&mut self, kind: PickerKind, name: &str, cx: &mut Context<Self>) {
        let settings = cx.global_mut::<Settings>();
        match kind {
            PickerKind::Theme if settings.set_theme(name) => {
                let theme = settings.theme.clone();
                crate::theme_apply::apply(&theme, cx);
            }
            PickerKind::IconTheme if settings.set_icon_theme(name) => cx.refresh_windows(),
            _ => {}
        }
        cx.notify();
    }

    /// Escape / click outside: back to the theme in use before.
    pub(super) fn revert_theme_preview(&mut self, cx: &mut Context<Self>) {
        if let Some((kind, original)) = self.theme_preview.take() {
            self.preview_theme(kind, &original, cx);
        }
    }

    /// Enter: apply and save to config.toml.
    pub(super) fn confirm_theme(&mut self, kind: PickerKind, name: &str, cx: &mut Context<Self>) {
        self.theme_preview = None;
        self.preview_theme(kind, name, cx);
        let key = if kind == PickerKind::Theme {
            "theme"
        } else {
            "icon_theme"
        };
        let settings = cx.global_mut::<Settings>();
        let name = if kind == PickerKind::Theme {
            settings.theme_name()
        } else {
            settings.icon_theme_name()
        };
        if kind == PickerKind::Theme {
            settings.config.theme = name.clone();
        } else {
            settings.config.icon_theme = name.clone();
        }
        let Some(path) = settings.files.as_ref().map(|f| f.config.clone()) else {
            return;
        };
        let source = std::fs::read_to_string(&path).unwrap_or_default();
        if let Err(error) = std::fs::write(&path, set_string_value(&source, key, &name)) {
            self.error(format!("Could not save config.toml: {error}"), cx);
        }
    }

    fn user_dir(&mut self, icons: bool, cx: &mut Context<Self>) -> Option<PathBuf> {
        let Some(files) = cx.global::<Settings>().files.clone() else {
            self.error("No home folder: settings live in memory only", cx);
            return None;
        };
        Some(if icons {
            files.icon_themes_dir
        } else {
            files.themes_dir
        })
    }

    pub(super) fn open_theme_folder(&mut self, icons: bool, cx: &mut Context<Self>) {
        let Some(dir) = self.user_dir(icons, cx) else {
            return;
        };
        if let Err(error) = std::fs::create_dir_all(&dir) {
            self.error(format!("Could not create {}: {error}", dir.display()), cx);
            return;
        }
        cx.open_with_system(&dir);
    }

    /// File dialog → [`Self::install_theme_file`].
    pub(super) fn install_theme_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Install Theme".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            if let Some(path) = paths.into_iter().next() {
                this.update(cx, |this, cx| this.install_theme_file(&path, cx))
                    .ok();
            }
        })
        .detach();
    }

    /// Validates and copies a theme into the themes folder, reloads, and
    /// switches to it (saved to config.toml).
    pub fn install_theme_file(&mut self, path: &Path, cx: &mut Context<Self>) {
        let Some(dir) = self.user_dir(false, cx) else {
            return;
        };
        match registry::install(path, &dir) {
            Ok((installed, name)) => {
                self.reload_settings(cx);
                // The registry renames a theme whose name is taken.
                let name = cx
                    .global::<Settings>()
                    .themes
                    .entries()
                    .into_iter()
                    .find(|e| e.source == ThemeSource::File(installed.clone()))
                    .map_or(name, |e| e.name.clone());
                self.confirm_theme(PickerKind::Theme, &name, cx);
                let applied = cx.global::<Settings>().theme_name();
                self.info(format!("Installed theme “{applied}”"), cx);
            }
            Err(error) => self.error(format!("Not a usable theme: {error}"), cx),
        }
    }
}
