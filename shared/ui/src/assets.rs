//! Bundled icons (Lucide, ISC) and fonts (IBM Plex Sans and Lilex, both
//! SIL OFL 1.1), embedded at compile time. Licence texts live next to the
//! files under `shared/ui/assets/`.

use std::borrow::Cow;

use gpui::{App, AssetSource, SharedString};

use crate::components::icon_name::ICON_FILES;

/// Family name of the bundled UI font.
pub const UI_FONT: &str = "IBM Plex Sans";
/// Family name of the bundled monospace font.
pub const BUFFER_FONT: &str = "Lilex";

macro_rules! fonts {
    ($($file:literal),* $(,)?) => {
        &[$(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts/", $file)),)*]
    };
}

const FONT_FILES: &[&[u8]] = fonts![
    "ibm-plex-sans/IBMPlexSans-Regular.ttf",
    "ibm-plex-sans/IBMPlexSans-Italic.ttf",
    "ibm-plex-sans/IBMPlexSans-SemiBold.ttf",
    "ibm-plex-sans/IBMPlexSans-SemiBoldItalic.ttf",
    "lilex/Lilex-Regular.ttf",
    "lilex/Lilex-Italic.ttf",
    "lilex/Lilex-Bold.ttf",
    "lilex/Lilex-BoldItalic.ttf",
];

/// The asset source Ferrum windows load icons from.
pub struct Assets;

impl Assets {
    /// Registers the bundled fonts with the text system.
    pub fn load_fonts(&self, cx: &App) -> anyhow::Result<()> {
        let fonts = FONT_FILES
            .iter()
            .map(|bytes| Cow::Borrowed(*bytes))
            .collect();
        cx.text_system().add_fonts(fonts)
    }
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(ICON_FILES
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut paths: Vec<SharedString> = ICON_FILES
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::from(*name))
            .collect();
        paths.dedup();
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::IconName;

    #[test]
    fn loads_icons_by_path() {
        let svg = Assets.load(IconName::Check.path()).expect("load");
        assert!(svg.is_some_and(|bytes| !bytes.is_empty()));
        assert!(Assets.load("icons/nope.svg").expect("load").is_none());
        assert!(Assets.list("icons/").expect("list").len() > 50);
    }

    #[test]
    fn bundles_truetype_fonts() {
        for bytes in FONT_FILES {
            assert!(bytes.len() > 10_000);
        }
    }
}
