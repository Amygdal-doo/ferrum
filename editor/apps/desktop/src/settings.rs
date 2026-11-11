//! The user's settings as a GPUI global: config, themes, icon themes,
//! keymap, snippets and the command catalogue. Reloaded whenever a file in
//! `~/.ferrum/editor/` (or its `themes/` / `icon-themes/` folders) changes.

use std::sync::Arc;

use ferrumeditor_core::commands::{CommandInfo, builtin_commands};
use ferrumeditor_core::config::Config;
use ferrumeditor_core::icons::{BUILTIN_ICON_THEME, IconTheme, IconThemeRegistry};
use ferrumeditor_core::keymap::{Keymap, UserKeymap};
use ferrumeditor_core::paths::UserFiles;
use ferrumeditor_core::snippets::SnippetSet;
use ferrumeditor_core::theme::registry::ThemeRegistry;
use ferrumeditor_core::theme::{Theme, ThemeOverrides};
use gpui::Global;

pub const MAC: bool = cfg!(target_os = "macos");

pub struct Settings {
    pub config: Config,
    pub theme: Arc<Theme>,
    pub themes: ThemeRegistry,
    pub icon_theme: Arc<IconTheme>,
    pub icon_themes: IconThemeRegistry,
    pub keymap: Keymap,
    pub snippets: SnippetSet,
    pub commands: Vec<CommandInfo>,
    pub files: Option<UserFiles>,
    /// Problems found in the user files at the last load, for a notice.
    pub problems: Vec<String>,
    /// Runtime font-size adjustment (Cmd+= / Cmd+-), not persisted.
    pub font_size_delta: f32,
    /// Runtime theme choice (palette / preview), until config.toml changes.
    pub theme_override: Option<String>,
    /// Runtime icon theme choice (preview), until config.toml changes.
    pub icon_theme_override: Option<String>,
}

impl Global for Settings {}

fn read(path: Option<&std::path::Path>) -> String {
    path.and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default()
}

impl Settings {
    pub fn font_size(&self) -> f32 {
        (self.config.font_size + self.font_size_delta).clamp(6.0, 72.0)
    }

    /// Loads everything from `files` (creating defaults first).
    pub fn load(files: Option<UserFiles>, commands: Vec<CommandInfo>) -> Self {
        let mut problems = Vec::new();
        if let Some(files) = &files
            && let Err(error) = files.ensure_defaults(&commands, MAC)
        {
            problems.push(format!("Could not write default settings: {error}"));
        }
        let config = match Config::parse(&read(files.as_ref().map(|f| f.config.as_path()))) {
            Ok((config, warnings)) => {
                problems.extend(warnings.into_iter().map(|w| format!("config.toml: {w}")));
                config
            }
            Err(error) => {
                problems.push(format!("config.toml: {error}"));
                Config::default()
            }
        };
        let mut keymap = Keymap::defaults(&commands, MAC);
        match UserKeymap::parse(&read(files.as_ref().map(|f| f.keymap.as_path()))) {
            Ok(user) => {
                let warnings = keymap.apply_user(&user, &commands, MAC);
                problems.extend(warnings.into_iter().map(|w| format!("keymap.toml: {w}")));
            }
            Err(error) => problems.push(format!("keymap.toml: {error}")),
        }
        let snippets = SnippetSet::parse(&read(files.as_ref().map(|f| f.snippets.as_path())))
            .unwrap_or_else(|error| {
                problems.push(format!("snippets.toml: {error}"));
                SnippetSet::default()
            });
        let (themes, warnings) =
            ThemeRegistry::load(files.as_ref().map(|f| f.themes_dir.as_path()));
        problems.extend(warnings);
        let (icon_themes, warnings) =
            IconThemeRegistry::load(files.as_ref().map(|f| f.icon_themes_dir.as_path()));
        problems.extend(warnings);
        let theme = resolve_theme(&themes, &config.theme, &files, &mut problems);
        let icon_theme = icon_themes.get(&config.icon_theme).unwrap_or_else(|| {
            problems.push(format!(
                "config.toml: unknown icon theme `{}` (using {BUILTIN_ICON_THEME})",
                config.icon_theme
            ));
            Arc::new(IconTheme::builtin())
        });
        Self {
            config,
            theme: Arc::new(theme),
            themes,
            icon_theme,
            icon_themes,
            keymap,
            snippets,
            commands,
            files,
            problems,
            font_size_delta: 0.0,
            theme_override: None,
            icon_theme_override: None,
        }
    }

    /// Reloads from the same files, keeping runtime adjustments.
    pub fn reload(&self) -> Self {
        let mut next = Self::load(self.files.clone(), self.commands.clone());
        next.font_size_delta = self.font_size_delta;
        if next.config.theme == self.config.theme
            && let Some(name) = &self.theme_override
            && next.themes.get(name).is_some()
        {
            next.set_theme(name);
        }
        if next.config.icon_theme == self.config.icon_theme
            && let Some(name) = &self.icon_theme_override
        {
            next.set_icon_theme(name);
        }
        next
    }

    /// Name of the theme in use (runtime choice or config).
    pub fn theme_name(&self) -> String {
        self.theme.name.clone()
    }

    pub fn icon_theme_name(&self) -> String {
        self.icon_theme.name.clone()
    }

    /// Switches the theme at runtime (keeps theme.toml overrides). Unknown
    /// names leave the theme unchanged and return false.
    pub fn set_theme(&mut self, name: &str) -> bool {
        if self.themes.get(name).is_none() {
            return false;
        }
        let mut problems = Vec::new();
        self.theme = Arc::new(resolve_theme(
            &self.themes,
            name,
            &self.files,
            &mut problems,
        ));
        self.theme_override = Some(self.theme.name.clone());
        true
    }

    /// Switches the icon theme at runtime; unknown names return false.
    pub fn set_icon_theme(&mut self, name: &str) -> bool {
        match self.icon_themes.get(name) {
            Some(theme) => {
                self.icon_theme_override = Some(theme.name.clone());
                self.icon_theme = theme;
                true
            }
            None => false,
        }
    }

    pub fn default_commands() -> Vec<CommandInfo> {
        builtin_commands()
    }
}

fn resolve_theme(
    themes: &ThemeRegistry,
    name: &str,
    files: &Option<UserFiles>,
    problems: &mut Vec<String>,
) -> Theme {
    let mut theme = themes.get(name).unwrap_or_else(|| {
        problems.push(format!(
            "config.toml: unknown theme `{name}` (see \"Theme: Select\")"
        ));
        Theme::dark()
    });
    let overrides = match ThemeOverrides::parse(&read(files.as_ref().map(|f| f.theme.as_path()))) {
        Ok((overrides, warnings)) => {
            problems.extend(warnings.into_iter().map(|w| format!("theme.toml: {w}")));
            overrides
        }
        Err(error) => {
            problems.push(format!("theme.toml: {error}"));
            ThemeOverrides::default()
        }
    };
    theme.apply(&overrides);
    theme
}
