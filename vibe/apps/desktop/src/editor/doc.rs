//! What kind of file the Editor column shows, decided before opening it:
//! images are drawn, text is edited, anything else is described.

use std::path::Path;

/// Extensions GPUI draws as images.
const IMAGE_EXTENSIONS: [&str; 9] = [
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "tif", "tiff",
];

/// How a file opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Image,
    /// Text, or something the editor will refuse (binary, not UTF-8, too
    /// big): its error then makes it [`Kind::Binary`].
    Text,
    Binary,
}

#[must_use]
pub fn kind_of(path: &Path) -> Kind {
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    if extension
        .as_deref()
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e))
    {
        Kind::Image
    } else if path.is_file() {
        Kind::Text
    } else {
        Kind::Binary
    }
}

/// `1.2 MB`, `340 B`.
#[must_use]
pub fn format_size(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let f = bytes as f64;
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1_048_576 => format!("{:.1} KB", f / 1024.),
        _ => format!("{:.1} MB", f / 1_048_576.),
    }
}

/// A tab's label: the file name, with its parent folder when another open
/// file has the same name.
#[must_use]
pub fn tab_label(path: &Path, others: &[&Path]) -> String {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into(),
    );
    let clash = others
        .iter()
        .any(|other| *other != path && other.file_name() == path.file_name());
    match path.parent().and_then(Path::file_name).filter(|_| clash) {
        Some(parent) => format!("{name} — {}", parent.to_string_lossy()),
        None => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_sizes_and_labels() {
        let dir = tempfile::tempdir().expect("tempdir");
        let text = dir.path().join("main.rs");
        std::fs::write(&text, "fn main() {}").expect("write");
        assert_eq!(kind_of(&text), Kind::Text);
        assert_eq!(kind_of(&dir.path().join("logo.PNG")), Kind::Image);
        assert_eq!(kind_of(&dir.path().join("icon.svg")), Kind::Image);
        assert_eq!(kind_of(&dir.path().join("gone.txt")), Kind::Binary);
        assert_eq!(kind_of(dir.path()), Kind::Binary);

        assert_eq!(format_size(340), "340 B");
        assert_eq!(format_size(2048), "2.0 KB");
        assert_eq!(format_size(3 * 1_048_576), "3.0 MB");

        let a = Path::new("/p/src/mod.rs");
        let b = Path::new("/p/src/files/mod.rs");
        let c = Path::new("/p/README.md");
        assert_eq!(tab_label(a, &[a, b, c]), "mod.rs — src");
        assert_eq!(tab_label(b, &[a, b, c]), "mod.rs — files");
        assert_eq!(tab_label(c, &[a, b, c]), "README.md");
    }
}
