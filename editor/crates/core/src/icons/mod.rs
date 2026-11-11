//! File icons for the tree view, tabs, file finder and find-in-files:
//! the built-in `ferrum` set ([`builtin`]), the bundled `material` set
//! ([`material`]) and icon themes loaded from
//! `~/.ferrum/editor/icon-themes/<name>/theme.toml` ([`manifest`]).
//!
//! Lookup order for a file: exact file name, then extension (longest
//! compound extension first: `d.ts` before `ts`), then language id, then
//! the theme's default file icon. Folders: folder name (open variant when
//! expanded), then the default folder icon. A user theme that has no match
//! at all (not even a default) defers to the built-in set.

mod builtin;
pub mod manifest;
mod material;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::language::Language;
use crate::theme::Color;

/// Name of the built-in icon theme.
pub const BUILTIN_ICON_THEME: &str = "ferrum";
/// Name of the bundled Material-style icon theme.
pub const MATERIAL_ICON_THEME: &str = "material";

/// One icon: monochrome SVG data plus its tint (`None` = muted text).
#[derive(Debug, Clone, PartialEq)]
pub struct Icon {
    pub id: String,
    pub svg: Arc<[u8]>,
    pub color: Option<Color>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Folder { open: bool },
}

#[derive(Debug, Clone, Default)]
pub struct IconTheme {
    pub name: String,
    /// The theme's folder (`None` for the built-in set).
    pub dir: Option<PathBuf>,
    icons: HashMap<String, Icon>,
    file_names: HashMap<String, String>,
    extensions: HashMap<String, String>,
    languages: HashMap<String, String>,
    folder_names: HashMap<String, String>,
    folder_names_open: HashMap<String, String>,
    default_file: Option<String>,
    default_folder: Option<String>,
    default_folder_open: Option<String>,
}

fn lower_map(entries: impl IntoIterator<Item = (String, String)>) -> HashMap<String, String> {
    entries
        .into_iter()
        .map(|(k, v)| (k.to_lowercase(), v))
        .collect()
}

impl IconTheme {
    /// The built-in `ferrum` set.
    pub fn builtin() -> Self {
        let pairs = |table: &[(&str, &str)]| {
            lower_map(table.iter().map(|(k, v)| (k.to_string(), v.to_string())))
        };
        Self {
            name: BUILTIN_ICON_THEME.into(),
            dir: None,
            icons: builtin::ICONS
                .iter()
                .map(|(id, svg, color)| {
                    let icon = Icon {
                        id: (*id).to_string(),
                        svg: Arc::from(svg.as_bytes()),
                        color: color.and_then(|c| Color::parse(c).ok()),
                    };
                    ((*id).to_string(), icon)
                })
                .collect(),
            file_names: pairs(builtin::FILE_NAMES),
            extensions: pairs(builtin::EXTENSIONS),
            languages: pairs(builtin::LANGUAGES),
            folder_names: lower_map(
                builtin::FOLDER_NAMES
                    .iter()
                    .map(|(n, closed, _)| (n.to_string(), closed.to_string())),
            ),
            folder_names_open: lower_map(
                builtin::FOLDER_NAMES
                    .iter()
                    .map(|(n, _, open)| (n.to_string(), open.to_string())),
            ),
            default_file: Some("file".into()),
            default_folder: Some("folder".into()),
            default_folder_open: Some("folder-open".into()),
        }
    }

    /// The bundled `material` set: the `ferrum` file tables with solid
    /// Material-style glyphs, plus folders coloured by role (`src`,
    /// `tests`, `docs`, `node_modules`, …).
    pub fn material() -> Self {
        let mut theme = Self::builtin();
        theme.name = MATERIAL_ICON_THEME.into();
        let icon = |id: &str, svg: &str, color: &str| Icon {
            id: id.to_string(),
            svg: Arc::from(svg.as_bytes()),
            color: Color::parse(color).ok(),
        };
        theme.icons = material::ICONS
            .iter()
            .map(|(id, svg, color)| ((*id).to_string(), icon(id, svg, color)))
            .collect();
        theme.extensions.extend(
            material::EXTENSIONS
                .iter()
                .map(|(ext, id)| ((*ext).to_string(), (*id).to_string())),
        );
        let (closed, open) = material::FOLDER_SHAPES;
        for (names, color) in material::FOLDERS {
            let key = names[0].trim_start_matches(['.', '_']);
            let (closed_id, open_id) = (format!("folder-{key}"), format!("folder-{key}-open"));
            theme
                .icons
                .insert(closed_id.clone(), icon(&closed_id, closed, color));
            theme
                .icons
                .insert(open_id.clone(), icon(&open_id, open, color));
            for name in *names {
                theme
                    .folder_names
                    .insert(name.to_string(), closed_id.clone());
                theme
                    .folder_names_open
                    .insert(name.to_string(), open_id.clone());
            }
        }
        theme
    }

    pub fn icon_count(&self) -> usize {
        self.icons.len()
    }

    /// The icon for an entry named `name` (a file name, not a path).
    pub fn resolve(&self, name: &str, kind: EntryKind, language: Option<&str>) -> Option<&Icon> {
        let lower = name.to_lowercase();
        let id = match kind {
            EntryKind::Folder { open } => {
                let named = if open {
                    self.folder_names_open
                        .get(&lower)
                        .or_else(|| self.folder_names.get(&lower))
                } else {
                    self.folder_names.get(&lower)
                };
                named.or(if open {
                    self.default_folder_open
                        .as_ref()
                        .or(self.default_folder.as_ref())
                } else {
                    self.default_folder.as_ref()
                })
            }
            EntryKind::File => self
                .file_names
                .get(&lower)
                .or_else(|| {
                    lower
                        .match_indices('.')
                        .find_map(|(i, _)| self.extensions.get(&lower[i + 1..]))
                })
                .or_else(|| language.and_then(|l| self.languages.get(&l.to_ascii_lowercase())))
                .or(self.default_file.as_ref()),
        };
        self.icons.get(id?)
    }

    /// [`Self::resolve`] for a path (language detected from the name).
    pub fn resolve_path(&self, path: &Path, kind: EntryKind) -> Option<&Icon> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        let language = match kind {
            EntryKind::File => Language::from_path(path).map(Language::id),
            EntryKind::Folder { .. } => None,
        };
        self.resolve(&name, kind, language)
    }
}

/// The built-in set, every theme in `~/.ferrum/editor/icon-themes/`, then
/// the bundled `material` set.
#[derive(Debug, Clone)]
pub struct IconThemeRegistry {
    themes: Vec<Arc<IconTheme>>,
}

impl Default for IconThemeRegistry {
    fn default() -> Self {
        Self {
            themes: vec![Arc::new(IconTheme::builtin())],
        }
    }
}

impl IconThemeRegistry {
    /// Loads every `<dir>/<name>/theme.toml`; problems become warnings.
    pub fn load(dir: Option<&Path>) -> (Self, Vec<String>) {
        let mut registry = Self::default();
        let mut warnings = Vec::new();
        let Some(dir) = dir else {
            registry.themes.push(Arc::new(IconTheme::material()));
            return (registry, warnings);
        };
        let mut folders: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.join(manifest::MANIFEST).is_file())
            .collect();
        folders.sort();
        for folder in folders {
            let label = folder
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            match manifest::load_dir(&folder) {
                Ok((theme, problems)) => {
                    warnings.extend(
                        problems
                            .into_iter()
                            .map(|w| format!("icon-themes/{label}: {w}")),
                    );
                    if registry.get(&theme.name).is_some() {
                        warnings.push(format!(
                            "icon-themes/{label}: name `{}` is already taken; skipped",
                            theme.name
                        ));
                    } else {
                        registry.themes.push(Arc::new(theme));
                    }
                }
                Err(error) => warnings.push(format!("icon-themes/{label}: {error}")),
            }
        }
        if registry.get(MATERIAL_ICON_THEME).is_none() {
            registry.themes.push(Arc::new(IconTheme::material()));
        }
        (registry, warnings)
    }

    /// A theme by name (case-insensitive).
    pub fn get(&self, name: &str) -> Option<Arc<IconTheme>> {
        let name = name.trim();
        self.themes
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(name))
            .cloned()
    }

    pub fn names(&self) -> Vec<String> {
        self.themes.iter().map(|t| t.name.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id<'a>(theme: &'a IconTheme, name: &str, kind: EntryKind) -> Option<&'a str> {
        theme
            .resolve_path(Path::new(name), kind)
            .map(|i| i.id.as_str())
    }

    #[test]
    fn builtin_resolves_names_extensions_and_folders() {
        let theme = IconTheme::builtin();
        assert_eq!(id(&theme, "Cargo.toml", EntryKind::File), Some("cargo"));
        assert_eq!(id(&theme, "other.toml", EntryKind::File), Some("toml"));
        assert_eq!(id(&theme, "main.rs", EntryKind::File), Some("rust"));
        assert_eq!(
            id(&theme, "index.d.ts", EntryKind::File),
            Some("typescript")
        );
        assert_eq!(id(&theme, "README.md", EntryKind::File), Some("readme"));
        assert_eq!(id(&theme, "LICENSE", EntryKind::File), Some("license"));
        assert_eq!(id(&theme, "Dockerfile", EntryKind::File), Some("docker"));
        assert_eq!(id(&theme, ".gitignore", EntryKind::File), Some("git"));
        assert_eq!(id(&theme, "logo.PNG", EntryKind::File), Some("image"));
        assert_eq!(
            id(&theme, ".bashrc", EntryKind::File),
            Some("shell"),
            "language"
        );
        assert_eq!(id(&theme, "mystery", EntryKind::File), Some("file"));
        assert_eq!(
            id(&theme, "src", EntryKind::Folder { open: false }),
            Some("folder")
        );
        assert_eq!(
            id(&theme, "src", EntryKind::Folder { open: true }),
            Some("folder-open")
        );
        assert_eq!(
            id(&theme, ".git", EntryKind::Folder { open: true }),
            Some("git")
        );
        for (icon_id, _, _) in builtin::ICONS {
            let icon = theme.icons.get(*icon_id).expect("icon");
            assert!(std::str::from_utf8(&icon.svg).is_ok_and(|s| s.starts_with("<svg")));
        }
        for (_, target) in builtin::FILE_NAMES
            .iter()
            .chain(builtin::EXTENSIONS)
            .chain(builtin::LANGUAGES)
        {
            assert!(theme.icons.contains_key(*target), "{target}");
        }
    }

    #[test]
    fn registry_always_has_the_builtin() {
        let (registry, warnings) = IconThemeRegistry::load(None);
        assert!(warnings.is_empty());
        assert_eq!(
            registry.names(),
            vec!["ferrum".to_string(), "material".to_string()]
        );
        assert!(registry.get("FERRUM").is_some());
        assert!(registry.get("Material").is_some());
    }

    #[test]
    fn material_set_colours_folders_by_role() {
        let theme = IconTheme::material();
        assert_eq!(id(&theme, "main.rs", EntryKind::File), Some("rust"));
        assert_eq!(id(&theme, "App.tsx", EntryKind::File), Some("react_ts"));
        assert_eq!(id(&theme, "Cargo.toml", EntryKind::File), Some("cargo"));
        assert_eq!(id(&theme, "mystery", EntryKind::File), Some("file"));
        assert_eq!(
            id(&theme, "src", EntryKind::Folder { open: false }),
            Some("folder-src")
        );
        assert_eq!(
            id(&theme, "Tests", EntryKind::Folder { open: true }),
            Some("folder-test-open")
        );
        assert_eq!(
            id(&theme, "node_modules", EntryKind::Folder { open: false }),
            Some("folder-node_modules")
        );
        assert_eq!(
            id(&theme, ".github", EntryKind::Folder { open: false }),
            Some("folder-github")
        );
        assert_eq!(
            id(&theme, "random", EntryKind::Folder { open: true }),
            Some("folder-open")
        );
        // Every table entry has an icon, every icon is an SVG with a tint.
        for (_, target) in builtin::FILE_NAMES
            .iter()
            .chain(builtin::EXTENSIONS)
            .chain(builtin::LANGUAGES)
            .chain(material::EXTENSIONS)
        {
            assert!(theme.icons.contains_key(*target), "{target}");
        }
        for icon in theme.icons.values() {
            let svg = std::str::from_utf8(&icon.svg).expect("utf-8");
            assert!(
                svg.starts_with("<svg") && svg.ends_with("</svg>"),
                "{}",
                icon.id
            );
            assert!(icon.color.is_some(), "{}", icon.id);
        }
        let src = theme.resolve("src", EntryKind::Folder { open: false }, None);
        let docs = theme.resolve("docs", EntryKind::Folder { open: false }, None);
        assert_ne!(src.map(|i| i.color), docs.map(|i| i.color));
    }
}
