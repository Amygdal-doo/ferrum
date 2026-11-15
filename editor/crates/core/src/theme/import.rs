//! "Theme: Import VS Code Theme": a `.json` colour theme (its `include`
//! chain read from next to it) or a `.vsix` extension (every theme listed
//! under `contributes.themes` in its `package.json`) is converted once to
//! our TOML format ([`super::file`]) and written to the themes folder, so
//! the installed copy no longer depends on the original files.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{Theme, file, jsonc, vscode};

/// Largest theme file (or `.vsix` member) read.
const MAX_BYTES: u64 = 4 * 1024 * 1024;
/// Largest `.vsix` accepted.
const MAX_VSIX_BYTES: u64 = 64 * 1024 * 1024;

/// One theme read from a VS Code file, with import warnings.
#[derive(Debug, Clone)]
pub struct ImportedTheme {
    pub theme: Theme,
    pub warnings: Vec<String>,
}

fn is_vsix(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("vsix"))
}

/// Whether `path` looks like something [`read_vscode_themes`] reads.
pub fn is_importable(path: &Path) -> bool {
    is_vsix(path)
        || path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("json"))
}

/// Imports `source` with `type` set from `ui_theme` when the file has none
/// (themes in extensions often rely on `uiTheme` alone).
fn import_json(
    source: &str,
    fallback_name: &str,
    ui_theme: Option<&str>,
    include: &dyn Fn(&str) -> Option<String>,
) -> Result<ImportedTheme, String> {
    let mut value = jsonc::parse(source)?;
    if let (Some(object), Some(ui_theme)) = (value.as_object_mut(), ui_theme)
        && !object.contains_key("type")
    {
        let light = matches!(ui_theme, "vs" | "hc-light");
        object.insert(
            "type".into(),
            Value::String(if light { "light" } else { "dark" }.into()),
        );
    }
    let (theme, warnings) = vscode::import(&value.to_string(), fallback_name, include)?;
    Ok(ImportedTheme { theme, warnings })
}

/// `base` joined with a relative `path` (`./`, `..` resolved), as used for
/// paths inside a `.vsix`.
fn join_relative(base: &str, path: &str) -> String {
    let mut parts: Vec<&str> = base.split('/').filter(|p| !p.is_empty()).collect();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// Every theme in a `.json` theme or a `.vsix` extension.
pub fn read_vscode_themes(path: &Path) -> Result<Vec<ImportedTheme>, String> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if is_vsix(path) {
        if size > MAX_VSIX_BYTES {
            return Err("extension too large".into());
        }
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        return read_vsix(file);
    }
    if size > MAX_BYTES {
        return Err("file too large for a theme".into());
    }
    let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("theme")
        .to_string();
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let include = move |name: &str| std::fs::read_to_string(dir.join(name)).ok();
    Ok(vec![import_json(&source, &stem, None, &include)?])
}

/// The themes of a VS Code extension package (a zip).
pub fn read_vsix(reader: impl Read + std::io::Seek) -> Result<Vec<ImportedTheme>, String> {
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| format!("not a .vsix extension: {e}"))?;
    // Every JSON member, so `include` can be followed inside the package.
    let mut members: HashMap<String, String> = HashMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        let wanted = name.to_ascii_lowercase().ends_with(".json")
            || name.to_ascii_lowercase().ends_with(".jsonc");
        if !entry.is_file() || !wanted || entry.size() > MAX_BYTES {
            continue;
        }
        let mut text = String::new();
        if entry.read_to_string(&mut text).is_ok() {
            members.insert(join_relative("", &name), text);
        }
    }
    let manifest = members
        .get("extension/package.json")
        .ok_or("the extension has no extension/package.json")?;
    let manifest = jsonc::parse(manifest).map_err(|e| format!("package.json: {e}"))?;
    let contributed = manifest["contributes"]["themes"]
        .as_array()
        .filter(|t| !t.is_empty())
        .ok_or("the extension contributes no colour themes")?;
    let mut themes = Vec::new();
    let mut problems = Vec::new();
    for entry in contributed {
        let Some(relative) = entry["path"].as_str() else {
            continue;
        };
        let member = join_relative("extension", relative);
        let label = entry["label"]
            .as_str()
            .or(entry["id"].as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        let Some(source) = members.get(&member) else {
            problems.push(format!("{relative}: not in the extension"));
            continue;
        };
        let dir = parent_dir(&member).to_string();
        let include = |name: &str| members.get(&join_relative(&dir, name)).cloned();
        let stem = Path::new(relative)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("theme");
        match import_json(source, stem, entry["uiTheme"].as_str(), &include) {
            Ok(mut imported) => {
                // VS Code lists a theme by its label.
                if !label.is_empty() {
                    imported.theme.name = label;
                }
                themes.push(imported);
            }
            Err(error) => problems.push(format!("{relative}: {error}")),
        }
    }
    if themes.is_empty() {
        return Err(problems
            .first()
            .cloned()
            .unwrap_or_else(|| "no usable theme in the extension".into()));
    }
    if let Some(first) = themes.first_mut() {
        first.warnings.extend(problems);
    }
    Ok(themes)
}

/// A file name for a theme name: `One Dark Pro` → `one-dark-pro`.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "theme".into() } else { out }
}

/// One installed theme: where it was written and its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub path: PathBuf,
    pub name: String,
    pub warnings: Vec<String>,
}

/// Converts the themes in `source` (`.json` or `.vsix`) and writes each as
/// `<dir>/<name>.toml` (replacing an earlier import of the same name).
pub fn install_vscode(source: &Path, dir: &Path) -> Result<Vec<Installed>, String> {
    if !is_importable(source) {
        return Err("pick a VS Code theme (.json) or extension (.vsix)".into());
    }
    let themes = read_vscode_themes(source)?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let origin = source
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut installed = Vec::new();
    for ImportedTheme { theme, warnings } in themes {
        let path = dir.join(format!("{}.toml", slug(&theme.name)));
        let header = format!(
            "# Imported from the VS Code theme `{origin}` by FerrumIDE.\n\
             # Edit freely; importing it again replaces this file.\n"
        );
        std::fs::write(&path, file::to_toml_with_header(&theme, &header))
            .map_err(|e| e.to_string())?;
        installed.push(Installed {
            path,
            name: theme.name,
            warnings,
        });
    }
    Ok(installed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::registry::load_file;
    use std::io::Write;

    const PARENT: &str = r##"{
        // Shared base of the light and dark variants.
        "colors": { "sideBar.background": "#eeeeee", "statusBar.background": "#dddddd", },
        "tokenColors": [{ "scope": "keyword.control", "settings": { "foreground": "#aa00aa" } }],
    }"##;

    const CHILD: &str = r##"{
        "name": "Fixture Light",
        "include": "./base.json",
        "colors": {
            "editor.background": "#fdfdfd",
            "editor.foreground": "#222222",
            "editorLineNumber.foreground": "#999999",
            "editorLineNumber.activeForeground": "#111111",
            "editor.selectionBackground": "#add6ff80",
            "activityBar.background": "#e0e0e0",
            "tab.activeBackground": "#ffffff",
            "tab.inactiveBackground": "#ececec",
            "terminal.ansiBlue": "#0451a5",
        },
        "tokenColors": [
            { "scope": ["string", "string.quoted.double"], "settings": { "foreground": "#a31515" } },
            { "scope": "entity.name.type", "settings": { "foreground": "#267f99" } },
        ],
    }"##;

    fn vsix(files: &[(&str, &str)]) -> Vec<u8> {
        let mut buffer = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buffer);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (name, text) in files {
                zip.start_file(*name, options).expect("start");
                zip.write_all(text.as_bytes()).expect("write");
            }
            zip.finish().expect("finish");
        }
        buffer.into_inner()
    }

    #[test]
    fn json_with_include_is_converted_and_installed() {
        let from = tempfile::tempdir().expect("tempdir");
        let to = tempfile::tempdir().expect("tempdir");
        std::fs::write(from.path().join("base.json"), PARENT).expect("write");
        let source = from.path().join("fixture-light-color-theme.json");
        std::fs::write(
            &source,
            CHILD.replace("\"name\"", "\"type\": \"light\", \"name\""),
        )
        .expect("write");
        let installed = install_vscode(&source, to.path()).expect("installs");
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].name, "Fixture Light");
        assert_eq!(installed[0].path, to.path().join("fixture-light.toml"));
        // The written TOML stands alone: no include needed any more.
        let (theme, warnings) = load_file(&installed[0].path).expect("loads");
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(!theme.dark);
        assert_eq!(theme.ui("editor").to_hex(), "#fdfdfd");
        assert_eq!(theme.ui("line_number").to_hex(), "#999999");
        assert_eq!(theme.ui("line_number_active").to_hex(), "#111111");
        assert_eq!(theme.ui("selection").to_hex(), "#add6ff80");
        assert_eq!(theme.ui("background").to_hex(), "#eeeeee");
        assert_eq!(theme.ui("status_bar").to_hex(), "#dddddd");
        assert_eq!(theme.ui("tab_active").to_hex(), "#ffffff");
        assert_eq!(theme.terminal("blue").to_hex(), "#0451a5");
        let hex = |n: &str| theme.syntax(n).map(super::super::Color::to_hex);
        assert_eq!(hex("keyword").as_deref(), Some("#aa00aa"));
        assert_eq!(hex("string").as_deref(), Some("#a31515"));
        assert_eq!(hex("type").as_deref(), Some("#267f99"));
        assert!(install_vscode(&from.path().join("x.toml"), to.path()).is_err());
    }

    #[test]
    fn vsix_lists_every_contributed_theme() {
        let package = r##"{
            "name": "fixture-themes",
            "contributes": { "themes": [
                { "label": "Fixture Light", "uiTheme": "vs", "path": "./themes/light.json" },
                { "label": "Fixture Dark", "uiTheme": "vs-dark", "path": "./themes/dark.json" },
                { "label": "Missing", "uiTheme": "vs-dark", "path": "./themes/missing.json" },
            ] },
        }"##;
        let dark = r##"{ "colors": { "editor.background": "#101010" } }"##;
        let bytes = vsix(&[
            ("extension.vsixmanifest", "<xml/>"),
            ("extension/package.json", package),
            ("extension/themes/base.json", PARENT),
            ("extension/themes/light.json", CHILD),
            ("extension/themes/dark.json", dark),
        ]);
        let themes = read_vsix(std::io::Cursor::new(bytes.clone())).expect("reads");
        let names: Vec<&str> = themes.iter().map(|t| t.theme.name.as_str()).collect();
        assert_eq!(names, ["Fixture Light", "Fixture Dark"]);
        // `uiTheme: vs` makes a theme without `type` light.
        assert!(!themes[0].theme.dark);
        assert!(themes[1].theme.dark);
        assert_eq!(themes[0].theme.ui("background").to_hex(), "#eeeeee");
        assert!(
            themes[0]
                .warnings
                .iter()
                .any(|w| w.contains("missing.json"))
        );

        let from = tempfile::tempdir().expect("tempdir");
        let to = tempfile::tempdir().expect("tempdir");
        let source = from.path().join("fixture.vsix");
        std::fs::write(&source, bytes).expect("write");
        let installed = install_vscode(&source, to.path()).expect("installs");
        assert_eq!(installed.len(), 2);
        assert!(to.path().join("fixture-dark.toml").is_file());
        let (theme, _) = load_file(&to.path().join("fixture-dark.toml")).expect("loads");
        assert_eq!(theme.name, "Fixture Dark");

        let empty = vsix(&[("extension/package.json", r#"{"contributes": {}}"#)]);
        assert!(read_vsix(std::io::Cursor::new(empty)).is_err());
        assert!(read_vsix(std::io::Cursor::new(b"not a zip".to_vec())).is_err());
    }

    #[test]
    fn slugs_and_relative_paths() {
        assert_eq!(slug("One Dark Pro"), "one-dark-pro");
        assert_eq!(slug("  Ayu (Mirage)!"), "ayu-mirage");
        assert_eq!(slug("✨"), "theme");
        assert_eq!(
            join_relative("extension", "./themes/a.json"),
            "extension/themes/a.json"
        );
        assert_eq!(
            join_relative("extension/themes", "../base.json"),
            "extension/base.json"
        );
    }
}
