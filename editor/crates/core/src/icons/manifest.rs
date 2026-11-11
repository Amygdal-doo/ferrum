//! Icon theme folders: `icon-themes/<name>/theme.toml` maps file names,
//! extensions, folder names and language ids to SVG files in that folder:
//!
//! ```toml
//! name = "Mono"                  # optional; defaults to the folder name
//! [defaults]
//! file = "file.svg"
//! folder = "folder.svg"
//! folder_open = "folder-open.svg"
//! [file_names]                   # exact, case-insensitive
//! "Cargo.toml" = "cargo.svg"
//! [extensions]                   # without the dot; "d.ts" beats "ts"
//! rs = "rust.svg"
//! [languages]                    # language ids (rust, python, shell, …)
//! python = "python.svg"
//! [folder_names]
//! src = "folder-src.svg"
//! [folder_names_open]
//! src = "folder-src-open.svg"
//! [colors]                       # optional tint per SVG file
//! "rust.svg" = "#dea584"
//! ```
//!
//! SVGs are painted as single-colour masks, tinted by `[colors]` (or the
//! theme's muted text colour). Paths are relative to the theme folder and
//! may not leave it.

use std::collections::HashMap;
use std::io;
use std::path::{Component, Path};
use std::sync::Arc;

use super::{Icon, IconTheme};
use crate::theme::Color;

pub const MANIFEST: &str = "theme.toml";
/// SVGs above this size are refused.
const MAX_SVG_BYTES: u64 = 256 * 1024;

const SECTIONS: &[&str] = &[
    "name",
    "defaults",
    "file_names",
    "extensions",
    "languages",
    "folder_names",
    "folder_names_open",
    "colors",
];

/// Reads `dir/theme.toml` and the SVGs it names.
pub fn load_dir(dir: &Path) -> Result<(IconTheme, Vec<String>), String> {
    let source = std::fs::read_to_string(dir.join(MANIFEST)).map_err(|e| e.to_string())?;
    let table: toml::Table = toml::from_str(&source).map_err(|e| e.message().to_string())?;
    let mut warnings: Vec<String> = table
        .keys()
        .filter(|k| !SECTIONS.contains(&k.as_str()))
        .map(|k| format!("unknown key `{k}`"))
        .collect();
    let folder_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "icons".into());
    let name = table
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map_or(folder_name, str::to_string);
    let mut colors: HashMap<String, Color> = HashMap::new();
    for (file, value) in section(&table, "colors", &mut warnings) {
        match Color::parse(&value) {
            Ok(color) => {
                colors.insert(file, color);
            }
            Err(error) => warnings.push(format!("colors.{file}: {error}")),
        }
    }
    let mut theme = IconTheme {
        name,
        dir: Some(dir.to_path_buf()),
        ..IconTheme::default()
    };
    let mut load = |file: &str, warnings: &mut Vec<String>| -> Option<String> {
        if theme.icons.contains_key(file) {
            return Some(file.to_string());
        }
        match read_svg(dir, file) {
            Ok(svg) => {
                let icon = Icon {
                    id: file.to_string(),
                    svg,
                    color: colors.get(file).copied(),
                };
                theme.icons.insert(file.to_string(), icon);
                Some(file.to_string())
            }
            Err(error) => {
                warnings.push(format!("`{file}`: {error}"));
                None
            }
        }
    };
    let mut maps: [HashMap<String, String>; 5] = Default::default();
    for (map, key) in maps.iter_mut().zip([
        "file_names",
        "extensions",
        "languages",
        "folder_names",
        "folder_names_open",
    ]) {
        for (entry, file) in section(&table, key, &mut warnings) {
            let entry = entry.to_lowercase();
            let entry = if key == "extensions" {
                entry.trim_start_matches('.').to_string()
            } else {
                entry
            };
            if let Some(id) = load(&file, &mut warnings) {
                map.insert(entry, id);
            }
        }
    }
    let mut defaults = HashMap::new();
    for (which, file) in section(&table, "defaults", &mut warnings) {
        if !matches!(which.as_str(), "file" | "folder" | "folder_open") {
            warnings.push(format!("unknown default `{which}`"));
            continue;
        }
        if let Some(id) = load(&file, &mut warnings) {
            defaults.insert(which, id);
        }
    }
    let [
        file_names,
        extensions,
        languages,
        folder_names,
        folder_names_open,
    ] = maps;
    theme.file_names = file_names;
    theme.extensions = extensions;
    theme.languages = languages;
    theme.folder_names = folder_names;
    theme.folder_names_open = folder_names_open;
    theme.default_file = defaults.remove("file");
    theme.default_folder = defaults.remove("folder");
    theme.default_folder_open = defaults.remove("folder_open");
    Ok((theme, warnings))
}

/// String entries of a table section; other value types warn.
fn section(table: &toml::Table, key: &str, warnings: &mut Vec<String>) -> Vec<(String, String)> {
    let Some(value) = table.get(key) else {
        return Vec::new();
    };
    let Some(entries) = value.as_table() else {
        warnings.push(format!("`{key}` must be a table"));
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|(k, v)| match v.as_str() {
            Some(s) => Some((k.clone(), s.to_string())),
            None => {
                warnings.push(format!("{key}.{k}: expected a file name string"));
                None
            }
        })
        .collect()
}

fn read_svg(dir: &Path, file: &str) -> Result<Arc<[u8]>, String> {
    let relative = Path::new(file);
    if relative
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("paths must stay inside the theme folder".into());
    }
    if !relative
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
    {
        return Err("icons must be .svg files".into());
    }
    let path = dir.join(relative);
    let size = std::fs::metadata(&path)
        .map_err(|_| "file not found".to_string())?
        .len();
    if size > MAX_SVG_BYTES {
        return Err("SVG larger than 256 KB".into());
    }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    if !String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).contains("<svg") {
        return Err("not an SVG file".into());
    }
    Ok(Arc::from(bytes))
}

const EXAMPLE_MANIFEST: &str = r##"# An example icon theme. Pick it with "Icon Theme: Select"; copy the folder
# to start your own (folder name = theme name unless `name` is set).
# Values are SVG files in this folder. Lookup order for files: exact file
# name > extension > language id > defaults.file. SVGs are painted as a
# single-colour mask, tinted with [colors] (default: the muted text colour).
name = "example"

[defaults]
file = "file.svg"
folder = "folder.svg"
folder_open = "folder-open.svg"

[file_names]
"README.md" = "star.svg"

[extensions]
md = "lines.svg"
txt = "lines.svg"

[languages]
rust = "diamond.svg"

[folder_names]

[folder_names_open]

[colors]
"star.svg" = "#f0b43c"
"diamond.svg" = "#e8743b"
"folder.svg" = "#8fb3d9"
"folder-open.svg" = "#8fb3d9"
"##;

const EXAMPLE_SVGS: &[(&str, &str)] = &[
    (
        "file.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><circle cx="8" cy="8" r="3.5"/></svg>"##,
    ),
    (
        "folder.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><rect x="2" y="4" width="12" height="9" rx="2"/></svg>"##,
    ),
    (
        "folder-open.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M2 5h12v8H2z" fill="none" stroke="#000" stroke-width="2"/></svg>"##,
    ),
    (
        "star.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M8 1.5l2 4.3 4.6.5-3.4 3.1 1 4.6L8 11.6l-4.2 2.4 1-4.6L1.4 6.3 6 5.8z"/></svg>"##,
    ),
    (
        "lines.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M3 4h10M3 8h10M3 12h6" stroke="#000" stroke-width="1.6"/></svg>"##,
    ),
    (
        "diamond.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M8 1.5 14.5 8 8 14.5 1.5 8z"/></svg>"##,
    ),
];

/// Writes `dir/example/` (manifest + SVGs) unless it exists.
pub fn write_example(dir: &Path) -> io::Result<()> {
    let folder = dir.join("example");
    if folder.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(&folder)?;
    std::fs::write(folder.join(MANIFEST), EXAMPLE_MANIFEST)?;
    for (name, svg) in EXAMPLE_SVGS {
        std::fs::write(folder.join(name), svg)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{EntryKind, IconThemeRegistry};
    use super::*;

    const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"/>"#;

    fn theme_dir(manifest: &str, svgs: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(MANIFEST), manifest).expect("write");
        for svg in svgs {
            std::fs::write(dir.path().join(svg), SVG).expect("write");
        }
        dir
    }

    #[test]
    fn resolution_priority_is_name_extension_language_default() {
        let dir = theme_dir(
            r#"
            [defaults]
            file = "default.svg"
            [file_names]
            "special.rs" = "name.svg"
            ".gitignore" = "git.svg"
            [extensions]
            rs = "ext.svg"
            [languages]
            rust = "lang.svg"
            shell = "lang.svg"
            "#,
            &["default.svg", "name.svg", "ext.svg", "lang.svg", "git.svg"],
        );
        let (theme, warnings) = load_dir(dir.path()).expect("loads");
        assert!(warnings.is_empty(), "{warnings:?}");
        let id = |name: &str, language: Option<&str>| {
            theme
                .resolve(name, EntryKind::File, language)
                .map(|i| i.id.clone())
        };
        assert_eq!(id("Special.rs", Some("rust")).as_deref(), Some("name.svg"));
        assert_eq!(id("main.rs", Some("rust")).as_deref(), Some("ext.svg"));
        assert_eq!(id(".bashrc", Some("shell")).as_deref(), Some("lang.svg"));
        assert_eq!(id("notes.xyz", None).as_deref(), Some("default.svg"));
        assert_eq!(id(".gitignore", None).as_deref(), Some("git.svg"));
        assert!(
            theme
                .resolve("src", EntryKind::Folder { open: false }, None)
                .is_none()
        );
    }

    #[test]
    fn bad_entries_warn_and_folders_resolve() {
        let dir = theme_dir(
            r##"
            name = "Mono"
            wat = 1
            [defaults]
            folder = "folder.svg"
            folder_open = "open.svg"
            bogus = "folder.svg"
            [folder_names]
            src = "src.svg"
            [extensions]
            rs = "../escape.svg"
            py = "missing.svg"
            md = 3
            [colors]
            "src.svg" = "#ff0000"
            "open.svg" = "orange"
            "##,
            &["folder.svg", "open.svg", "src.svg"],
        );
        let (theme, warnings) = load_dir(dir.path()).expect("loads");
        assert_eq!(theme.name, "Mono");
        assert_eq!(warnings.len(), 6, "{warnings:?}");
        let folder = |name: &str, open: bool| {
            theme
                .resolve(name, EntryKind::Folder { open }, None)
                .map(|i| (i.id.clone(), i.color.map(|c| c.to_hex())))
        };
        assert_eq!(
            folder("SRC", true),
            Some(("src.svg".into(), Some("#ff0000".into())))
        );
        assert_eq!(folder("lib", true), Some(("open.svg".into(), None)));
        assert_eq!(folder("lib", false), Some(("folder.svg".into(), None)));
        assert!(
            theme
                .resolve("main.rs", EntryKind::File, Some("rust"))
                .is_none()
        );
        assert!(load_dir(&dir.path().join("nope")).is_err());
    }

    #[test]
    fn example_theme_loads_into_the_registry() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_example(dir.path()).expect("write");
        let (registry, warnings) = IconThemeRegistry::load(Some(dir.path()));
        assert!(warnings.is_empty(), "{warnings:?}");
        // Bundled `material` comes after the user's themes.
        assert_eq!(
            registry.names(),
            vec![
                "ferrum".to_string(),
                "example".to_string(),
                "material".to_string()
            ]
        );
        let example = registry.get("example").expect("example");
        let readme = example.resolve("readme.md", EntryKind::File, Some("markdown"));
        assert_eq!(readme.map(|i| i.id.as_str()), Some("star.svg"));
    }
}
