//! A project's `.architecture/` folder: `system.json` (the [`System`]) and
//! `icons/` (custom icons, `<tech id>.png` / `.svg` / `.jpg`, which also
//! replace a catalogue technology's badge).

use std::path::{Path, PathBuf};

use crate::model::System;

pub const DIR: &str = ".architecture";
pub const SYSTEM_FILE: &str = "system.json";
pub const ICONS_DIR: &str = "icons";
const ICON_EXTENSIONS: [&str; 4] = ["png", "svg", "jpg", "jpeg"];

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path} is not a valid system: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("could not write {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("icons must be PNG, SVG or JPEG")]
    IconFormat,
}

pub fn system_file(project: &Path) -> PathBuf {
    project.join(DIR).join(SYSTEM_FILE)
}

/// The project's system, `None` when it has no `.architecture/` yet.
pub fn load(project: &Path) -> Result<Option<System>, StoreError> {
    let path = system_file(project);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(StoreError::Read { path, source }),
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|source| StoreError::Parse { path, source })
}

/// Writes `path` atomically (through `<path>.partial`), making its folder.
fn write(path: PathBuf, text: String) -> Result<(), StoreError> {
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    std::fs::create_dir_all(&dir)
        .and_then(|()| {
            let partial = path.with_extension("partial");
            std::fs::write(&partial, text)?;
            std::fs::rename(&partial, &path)
        })
        .map_err(|source| StoreError::Write { path, source })
}

/// Writes `system` (pretty JSON, atomically) and a short README.
pub fn save(project: &Path, system: &System) -> Result<(), StoreError> {
    let dir = project.join(DIR);
    let json = serde_json::to_string_pretty(system).expect("a System always serializes");
    write(dir.join(SYSTEM_FILE), json + "\n")?;
    let readme = dir.join("README.md");
    if !readme.exists() {
        write(readme, README.to_owned())?;
    }
    Ok(())
}

/// Saves an edit of the project's system into the `system.json` there:
/// only what changed is rewritten, the file's formatting and the fields
/// FerrumSystem does not know are kept (see [`crate::patch`]). Nothing is
/// written when the file already says the same; a missing or unreadable
/// file is written afresh.
pub fn save_edit(project: &Path, system: &System) -> Result<(), StoreError> {
    let path = system_file(project);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return save(project, system),
        Err(source) => return Err(StoreError::Read { path, source }),
    };
    match updated(&text, system) {
        Some(None) => Ok(()),
        Some(Some(new)) => write(path, new),
        None => save(project, system),
    }
}

/// The text of the project's `system.json`, if it can be read.
pub fn read_text(project: &Path) -> Option<String> {
    std::fs::read_to_string(system_file(project)).ok()
}

/// Puts back `text` as the project's `system.json` (undo restores the file
/// exactly as it was, unknown fields of a deleted component included).
pub fn restore_text(project: &Path, text: &str) -> Result<(), StoreError> {
    write(system_file(project), text.to_owned())
}

/// `text` (a `system.json`) rewritten to say `system`: `Some(None)` when it
/// already does, `None` when it cannot be patched.
fn updated(text: &str, system: &System) -> Option<Option<String>> {
    let read: System = serde_json::from_str(text).ok()?;
    let before = serde_json::to_value(&read).ok()?;
    let after = serde_json::to_value(system).ok()?;
    if before == after {
        return Some(None);
    }
    crate::patch::update(text, &before, &after).map(Some)
}

const README: &str = "# Architecture\n\n\
This folder is the system's architecture, edited in FerrumSystem (`fes .`).\n\n\
- `system.json`: layers, components, connections, APIs, sockets, databases, flows, \
reliability notes and where each component runs.\n\
- `icons/`: custom icons, named after the technology id (`my-tech.svg`). An icon \
named like a built-in technology (`postgresql.png`) replaces its badge.\n";

/// The custom icon for technology `id`, if the project has one.
pub fn icon(project: &Path, id: &str) -> Option<PathBuf> {
    let dir = project.join(DIR).join(ICONS_DIR);
    ICON_EXTENSIONS
        .iter()
        .map(|ext| dir.join(format!("{id}.{ext}")))
        .find(|p| p.is_file())
}

/// Copies `source` in as the icon of technology `id` (replacing an older one).
pub fn add_icon(project: &Path, id: &str, source: &Path) -> Result<PathBuf, StoreError> {
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| ICON_EXTENSIONS.contains(&e.as_str()))
        .ok_or(StoreError::IconFormat)?;
    let dir = project.join(DIR).join(ICONS_DIR);
    let target = dir.join(format!("{id}.{ext}"));
    let copy = || -> std::io::Result<()> {
        std::fs::create_dir_all(&dir)?;
        for old in ICON_EXTENSIONS {
            let _ = std::fs::remove_file(dir.join(format!("{id}.{old}")));
        }
        std::fs::copy(source, &target).map(|_| ())
    };
    copy().map_err(|source| StoreError::Write {
        path: target.clone(),
        source,
    })?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_loads_and_keeps_icons() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(load(dir.path()).expect("load").is_none());
        let system = crate::demos::banking();
        save(dir.path(), &system).expect("save");
        assert_eq!(load(dir.path()).expect("load"), Some(system));
        assert!(dir.path().join(".architecture/README.md").is_file());

        let png = dir.path().join("logo.PNG");
        std::fs::write(&png, b"\x89PNG").expect("png");
        let icon_path = add_icon(dir.path(), "acme", &png).expect("icon");
        assert_eq!(icon(dir.path(), "acme"), Some(icon_path));
        assert!(add_icon(dir.path(), "acme", &dir.path().join("x.gif")).is_err());
    }

    #[test]
    fn an_edit_keeps_the_files_formatting_and_unknown_fields() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = system_file(dir.path());
        std::fs::create_dir_all(file.parent().expect("test")).expect("test");
        let text = r#"{
  "name": "Shop",
  "x-owner": "platform-team",
  "layers": [{ "id": "services", "name": "Services", "color": 2278750 }],
  "nodes": [
    { "id": "api", "name": "API", "kind": "service", "layer": "services", "sla": "99.9" },
    { "id": "db", "name": "DB", "kind": "sql-db", "layer": "services" }
  ]
}
"#;
        std::fs::write(&file, text).expect("test");
        let mut system = load(dir.path()).expect("test").expect("test");
        save_edit(dir.path(), &system).expect("test");
        assert_eq!(
            std::fs::read_to_string(&file).expect("test"),
            text,
            "no change, no write"
        );

        system.nodes[1].name = "Orders DB".into();
        system.remove_node("api");
        save_edit(dir.path(), &system).expect("test");
        let saved = std::fs::read_to_string(&file).expect("test");
        assert!(saved.contains(r#""x-owner": "platform-team""#), "{saved}");
        assert!(
            saved.contains(
                r#"{ "id": "db", "name": "Orders DB", "kind": "sql-db", "layer": "services" }"#
            ),
            "{saved}"
        );
        assert!(!saved.contains("sla"), "{saved}");
        assert_eq!(load(dir.path()).expect("test"), Some(system));
        assert!(!dir.path().join(".architecture/README.md").exists());

        std::fs::write(&file, "not json").expect("test");
        let fresh = crate::demos::starter("x");
        save_edit(dir.path(), &fresh).expect("test");
        assert_eq!(load(dir.path()).expect("test"), Some(fresh));
    }
}
