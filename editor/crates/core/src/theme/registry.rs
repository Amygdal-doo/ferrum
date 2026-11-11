//! Every theme the editor can use: the built-ins plus the files in
//! `~/.ferrum/editor/themes/` — `*.toml` in our format ([`super::file`])
//! and `*.json` VS Code themes ([`super::vscode`]). Files ending in
//! `.example.toml` are documentation, not loaded. A file that fails to
//! parse is skipped with a warning.

use std::io;
use std::path::{Path, PathBuf};

use super::{BUILTIN_THEMES, Theme, file, vscode};

/// Theme files above this size are ignored.
const MAX_THEME_BYTES: u64 = 4 * 1024 * 1024;
const EXAMPLE_SUFFIX: &str = ".example.toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeSource {
    Builtin,
    File(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeEntry {
    pub name: String,
    pub dark: bool,
    pub source: ThemeSource,
}

#[derive(Debug, Clone)]
pub struct ThemeRegistry {
    themes: Vec<(ThemeEntry, Theme)>,
}

impl Default for ThemeRegistry {
    fn default() -> Self {
        Self::builtins()
    }
}

/// Whether the registry would try to load `path`.
pub fn is_theme_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    !name.starts_with('.')
        && !name.ends_with(EXAMPLE_SUFFIX)
        && (name.ends_with(".toml") || name.ends_with(".json"))
}

/// Parses one theme file (format chosen by extension).
pub fn load_file(path: &Path) -> Result<(Theme, Vec<String>), String> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_THEME_BYTES {
        return Err("file too large for a theme".into());
    }
    let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("theme")
        .to_string();
    let is_json = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"));
    if is_json {
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let include = move |name: &str| std::fs::read_to_string(dir.join(name)).ok();
        vscode::import(&source, &stem, &include)
    } else {
        file::parse(&source, &stem)
    }
}

impl ThemeRegistry {
    pub fn builtins() -> Self {
        Self {
            themes: BUILTIN_THEMES
                .iter()
                .filter_map(|name| Theme::builtin(name))
                .map(|theme| {
                    let entry = ThemeEntry {
                        name: theme.name.clone(),
                        dark: theme.dark,
                        source: ThemeSource::Builtin,
                    };
                    (entry, theme)
                })
                .collect(),
        }
    }

    /// Built-ins plus every theme file in `dir` (missing dir = none).
    /// Returns warnings prefixed with the file name.
    pub fn load(dir: Option<&Path>) -> (Self, Vec<String>) {
        let mut registry = Self::builtins();
        let mut warnings = Vec::new();
        let Some(dir) = dir else {
            return (registry, warnings);
        };
        let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && is_theme_file(p))
            .collect();
        paths.sort();
        for path in paths {
            let label = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            match load_file(&path) {
                Ok((theme, problems)) => {
                    warnings.extend(problems.into_iter().map(|w| format!("themes/{label}: {w}")));
                    if let Some(w) = registry.add(theme, path) {
                        warnings.push(format!("themes/{label}: {w}"));
                    }
                }
                Err(error) => warnings.push(format!("themes/{label}: {error}")),
            }
        }
        (registry, warnings)
    }

    /// Adds a loaded theme. A name taken by another theme falls back to the
    /// file stem; if that is taken too, the theme is skipped.
    fn add(&mut self, mut theme: Theme, path: PathBuf) -> Option<String> {
        let mut warning = None;
        if self.find(&theme.name).is_some() {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            if stem.is_empty() || self.find(&stem).is_some() {
                return Some(format!(
                    "theme name `{}` is already taken; skipped",
                    theme.name
                ));
            }
            warning = Some(format!(
                "theme name `{}` is already taken; using `{stem}`",
                theme.name
            ));
            theme.name = stem;
        }
        let entry = ThemeEntry {
            name: theme.name.clone(),
            dark: theme.dark,
            source: ThemeSource::File(path),
        };
        self.themes.push((entry, theme));
        warning
    }

    fn find(&self, name: &str) -> Option<&Theme> {
        let name = name.trim();
        self.themes
            .iter()
            .find(|(e, _)| e.name == name)
            .or_else(|| {
                self.themes
                    .iter()
                    .find(|(e, _)| e.name.eq_ignore_ascii_case(name))
            })
            .map(|(_, t)| t)
    }

    /// A theme by name (exact, then case-insensitive; `dark` / `light`
    /// name the built-ins).
    pub fn get(&self, name: &str) -> Option<Theme> {
        self.find(name).cloned().or_else(|| Theme::builtin(name))
    }

    /// Built-ins first, then loaded themes sorted by name.
    pub fn entries(&self) -> Vec<&ThemeEntry> {
        let mut entries: Vec<&ThemeEntry> = self.themes.iter().map(|(e, _)| e).collect();
        entries.sort_by_key(|e| {
            (
                e.source != ThemeSource::Builtin,
                e.name.to_ascii_lowercase(),
            )
        });
        entries
    }
}

/// Writes `ferrum-dark.example.toml` / `ferrum-light.example.toml` into
/// `dir` (creating it) unless they exist.
pub fn write_examples(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for name in BUILTIN_THEMES {
        let path = dir.join(format!("{name}{EXAMPLE_SUFFIX}"));
        if !path.exists()
            && let Some(theme) = Theme::builtin(name)
        {
            std::fs::write(path, file::to_toml(&theme))?;
        }
    }
    Ok(())
}

/// Validates `source` as a theme and copies it into `dir`. Returns the new
/// path and the theme's name.
pub fn install(source: &Path, dir: &Path) -> Result<(PathBuf, String), String> {
    if !is_theme_file(source) {
        return Err("pick a .toml (Ferrum) or .json (VS Code) theme file".into());
    }
    let (theme, _) = load_file(source)?;
    let file_name = source.file_name().ok_or("no file name")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let target = dir.join(file_name);
    if source != target {
        std::fs::copy(source, &target).map_err(|e| e.to_string())?;
    }
    Ok((target, theme.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_files_skips_examples_and_reports_bad_ones() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_examples(dir.path()).expect("examples");
        std::fs::write(
            dir.path().join("ocean.toml"),
            "appearance = \"dark\"\n[ui]\naccent = \"#00aaff\"\n",
        )
        .expect("write");
        std::fs::write(
            dir.path().join("vs.json"),
            r##"{"name": "VS Light", "type": "light"}"##,
        )
        .expect("write");
        std::fs::write(dir.path().join("broken.json"), "{ nope").expect("write");
        std::fs::write(dir.path().join("dupe.toml"), "name = \"ferrum-dark\"").expect("write");
        std::fs::write(dir.path().join("notes.txt"), "ignored").expect("write");
        let (registry, warnings) = ThemeRegistry::load(Some(dir.path()));
        let names: Vec<&str> = registry.entries().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            ["ferrum-dark", "ferrum-light", "dupe", "ocean", "VS Light"]
        );
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings.iter().any(|w| w.starts_with("themes/broken.json")));
        assert_eq!(
            registry
                .get("ocean")
                .map(|t| t.ui("accent").to_hex())
                .as_deref(),
            Some("#00aaff")
        );
        assert!(registry.get("vs light").is_some_and(|t| !t.dark));
        assert!(
            registry
                .get("light")
                .is_some_and(|t| t.name == "ferrum-light")
        );
        assert!(registry.get("missing").is_none());
    }

    #[test]
    fn examples_parse_back_to_the_builtins() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_examples(dir.path()).expect("examples");
        let (theme, warnings) =
            load_file(&dir.path().join("ferrum-light.example.toml")).expect("valid");
        assert!(warnings.is_empty());
        assert_eq!(Some(theme), Theme::builtin("ferrum-light"));
    }

    #[test]
    fn install_validates_and_copies() {
        let from = tempfile::tempdir().expect("tempdir");
        let to = tempfile::tempdir().expect("tempdir");
        let good = from.path().join("mine.json");
        std::fs::write(&good, r##"{"name": "Mine"}"##).expect("write");
        let bad = from.path().join("bad.toml");
        std::fs::write(&bad, "[ui").expect("write");
        let (path, name) = install(&good, to.path()).expect("installs");
        assert_eq!(name, "Mine");
        assert!(path.exists());
        assert!(install(&bad, to.path()).is_err());
        assert!(install(&from.path().join("x.txt"), to.path()).is_err());
        assert!(!to.path().join("bad.toml").exists());
    }
}
