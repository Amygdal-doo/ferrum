//! The user's settings as a GPUI global: config, theme, keymap, snippets
//! and the command catalogue. Reloaded whenever a file in
//! `~/.ferrum/editor/` changes.

use std::sync::Arc;

use ferrumeditor_core::commands::{CommandInfo, builtin_commands};
use ferrumeditor_core::config::Config;
use ferrumeditor_core::keymap::{Keymap, UserKeymap};
use ferrumeditor_core::paths::UserFiles;
use ferrumeditor_core::snippets::SnippetSet;
use ferrumeditor_core::theme::{Theme, ThemeOverrides};
use gpui::Global;

pub const MAC: bool = cfg!(target_os = "macos");

pub struct Settings {
    pub config: Config,
    pub theme: Arc<Theme>,
    pub keymap: Keymap,
    pub snippets: SnippetSet,
    pub commands: Vec<CommandInfo>,
    pub files: Option<UserFiles>,
    /// Problems found in the user files at the last load, for a notice.
    pub problems: Vec<String>,
    /// Runtime font-size adjustment (Cmd+= / Cmd+-), not persisted.
    pub font_size_delta: f32,
    /// Runtime theme choice from the palette, until config.toml changes.
    pub theme_override: Option<String>,
}

impl Global for Settings {}

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
        let read = |path: Option<&std::path::Path>| -> String {
            path.and_then(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_default()
        };
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
        let overrides =
            match ThemeOverrides::parse(&read(files.as_ref().map(|f| f.theme.as_path()))) {
                Ok((overrides, warnings)) => {
                    problems.extend(warnings.into_iter().map(|w| format!("theme.toml: {w}")));
                    overrides
                }
                Err(error) => {
                    problems.push(format!("theme.toml: {error}"));
                    ThemeOverrides::default()
                }
            };
        let theme = resolve_theme(&config.theme, &overrides, &mut problems);
        Self {
            config,
            theme: Arc::new(theme),
            keymap,
            snippets,
            commands,
            files,
            problems,
            font_size_delta: 0.0,
            theme_override: None,
        }
    }

    /// Reloads from the same files, keeping runtime adjustments.
    pub fn reload(&self) -> Self {
        let mut next = Self::load(self.files.clone(), self.commands.clone());
        next.font_size_delta = self.font_size_delta;
        if next.config.theme == self.config.theme
            && let Some(name) = &self.theme_override
        {
            next.set_theme(name);
        }
        next
    }

    /// Switches the base theme at runtime (keeps theme.toml overrides).
    pub fn set_theme(&mut self, name: &str) {
        let overrides = self
            .files
            .as_ref()
            .and_then(|f| std::fs::read_to_string(&f.theme).ok())
            .and_then(|s| ThemeOverrides::parse(&s).ok())
            .map(|(o, _)| o)
            .unwrap_or_default();
        let mut problems = Vec::new();
        self.theme = Arc::new(resolve_theme(name, &overrides, &mut problems));
        self.theme_override = Some(name.to_string());
    }

    pub fn default_commands() -> Vec<CommandInfo> {
        builtin_commands()
    }
}

fn resolve_theme(name: &str, overrides: &ThemeOverrides, problems: &mut Vec<String>) -> Theme {
    let mut theme = Theme::builtin(name).unwrap_or_else(|| {
        problems.push(format!(
            "config.toml: unknown theme `{name}` (use ferrum-dark or ferrum-light)"
        ));
        Theme::dark()
    });
    theme.apply(overrides);
    theme
}
