//! The user directory `~/.ferrum/editor/` and its files. Missing files are
//! created with documented defaults on first launch; existing ones are
//! never overwritten.

use std::io;
use std::path::{Path, PathBuf};

use crate::commands::CommandInfo;

/// Overrides the user directory (tests, portable installs).
pub const HOME_ENV: &str = "FERRUM_EDITOR_HOME";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserFiles {
    pub dir: PathBuf,
    pub config: PathBuf,
    pub keymap: PathBuf,
    pub snippets: PathBuf,
    pub theme: PathBuf,
    pub session: PathBuf,
    /// Debugger breakpoints per project.
    pub breakpoints: PathBuf,
    /// Whole themes (`*.toml`, VS Code `*.json`).
    pub themes_dir: PathBuf,
    /// Icon themes, one folder each with a `theme.toml`.
    pub icon_themes_dir: PathBuf,
    /// Language servers (and Node.js / Go) installed by the editor.
    pub servers_dir: PathBuf,
}

impl UserFiles {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        Self {
            config: dir.join("config.toml"),
            keymap: dir.join("keymap.toml"),
            snippets: dir.join("snippets.toml"),
            theme: dir.join("theme.toml"),
            session: dir.join("session.json"),
            breakpoints: dir.join("breakpoints.json"),
            themes_dir: dir.join("themes"),
            icon_themes_dir: dir.join("icon-themes"),
            servers_dir: dir.join("servers"),
            dir,
        }
    }

    /// `$FERRUM_EDITOR_HOME`, else `~/.ferrum/editor`.
    pub fn locate() -> Option<Self> {
        if let Some(dir) = std::env::var_os(HOME_ENV).filter(|v| !v.is_empty()) {
            return Some(Self::new(PathBuf::from(dir)));
        }
        let home = directories::BaseDirs::new()?.home_dir().to_path_buf();
        Some(Self::new(home.join(".ferrum").join("editor")))
    }

    /// Writes any missing user file with its documented default.
    pub fn ensure_defaults(&self, commands: &[CommandInfo], mac: bool) -> io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let keymap = crate::keymap::default_keymap_document(commands, mac);
        let files: [(&Path, &str); 4] = [
            (&self.config, crate::config::DEFAULT_CONFIG_TOML),
            (&self.keymap, &keymap),
            (&self.snippets, crate::snippets::DEFAULT_SNIPPETS_TOML),
            (&self.theme, crate::theme::DEFAULT_THEME_TOML),
        ];
        for (path, contents) in files {
            if !path.exists() {
                std::fs::write(path, contents)?;
            }
        }
        crate::theme::registry::write_examples(&self.themes_dir)?;
        if !self.icon_themes_dir.exists() {
            std::fs::create_dir_all(&self.icon_themes_dir)?;
            crate::icons::manifest::write_example(&self.icon_themes_dir)?;
        }
        Ok(())
    }

    /// Whether a change at `path` affects loaded themes or icon themes.
    pub fn is_theme_path(&self, path: &Path) -> bool {
        path.starts_with(&self.themes_dir) || path.starts_with(&self.icon_themes_dir)
    }

    /// Whether `path` is one of the watched settings files.
    pub fn is_settings_file(&self, path: &Path) -> bool {
        [&self.config, &self.keymap, &self.snippets, &self.theme]
            .iter()
            .any(|p| p.as_path() == path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::builtin_commands;

    #[test]
    fn writes_defaults_once() {
        let dir = tempfile::tempdir().expect("tempdir");
        let files = UserFiles::new(dir.path().join("editor"));
        files
            .ensure_defaults(&builtin_commands(), true)
            .expect("write");
        for path in [&files.config, &files.keymap, &files.snippets, &files.theme] {
            assert!(path.exists(), "{}", path.display());
        }
        std::fs::write(&files.config, "font_size = 20").expect("write");
        files
            .ensure_defaults(&builtin_commands(), true)
            .expect("write");
        assert_eq!(
            std::fs::read_to_string(&files.config).expect("read"),
            "font_size = 20"
        );
        assert!(files.is_settings_file(&files.theme));
        assert!(!files.is_settings_file(&files.session));
        assert!(files.themes_dir.join("ferrum-dark.example.toml").exists());
        assert!(files.icon_themes_dir.join("example/theme.toml").exists());
        assert!(files.is_theme_path(&files.themes_dir.join("x.json")));
        assert!(!files.is_theme_path(&files.config));
    }

    #[test]
    fn default_snippets_parse() {
        let set = crate::snippets::SnippetSet::parse(crate::snippets::DEFAULT_SNIPPETS_TOML)
            .expect("valid");
        assert!(!set.is_empty());
    }
}
