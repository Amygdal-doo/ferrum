//! Static inline SVG icons (authored here, so `PreEscaped` is safe).
//! All icons are decorative (`aria-hidden`) and use `currentColor`.

use maud::{Markup, PreEscaped};

const SUN: &str = r#"<svg class="icon icon--sun" viewBox="0 0 24 24" width="20" height="20" aria-hidden="true" focusable="false" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="12" cy="12" r="4.5"/><path d="M12 2v2.5M12 19.5V22M2 12h2.5M19.5 12H22M4.9 4.9l1.8 1.8M17.3 17.3l1.8 1.8M4.9 19.1l1.8-1.8M17.3 6.7l1.8-1.8"/></svg>"#;
const MOON: &str = r#"<svg class="icon icon--moon" viewBox="0 0 24 24" width="20" height="20" aria-hidden="true" focusable="false" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="M20.5 14.5A8.5 8.5 0 0 1 9.5 3.5a8.5 8.5 0 1 0 11 11z"/></svg>"#;
const MENU: &str = r#"<svg class="icon icon--menu" viewBox="0 0 24 24" width="22" height="22" aria-hidden="true" focusable="false" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path class="icon-menu__top" d="M4 7h16"/><path class="icon-menu__mid" d="M4 12h16"/><path class="icon-menu__bot" d="M4 17h16"/></svg>"#;
const ARROW: &str = r#"<svg class="icon icon--arrow" viewBox="0 0 24 24" width="16" height="16" aria-hidden="true" focusable="false" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14M13 6l6 6-6 6"/></svg>"#;
const SHIELD: &str = r#"<svg class="icon" viewBox="0 0 24 24" width="24" height="24" aria-hidden="true" focusable="false" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linejoin="round"><path d="M12 3l8 3v6c0 4.5-3.4 8.2-8 9-4.6-.8-8-4.5-8-9V6z"/><path d="M8.5 12l2.5 2.5 4.5-5" stroke-linecap="round"/></svg>"#;
const BOLT: &str = r#"<svg class="icon" viewBox="0 0 24 24" width="24" height="24" aria-hidden="true" focusable="false" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linejoin="round"><path d="M13 2L4 14h7l-1 8 9-12h-7z"/></svg>"#;
const CUBE: &str = r#"<svg class="icon" viewBox="0 0 24 24" width="24" height="24" aria-hidden="true" focusable="false" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linejoin="round"><path d="M12 2l9 5v10l-9 5-9-5V7z"/><path d="M3 7l9 5 9-5M12 12v10"/></svg>"#;
const STREAMS: &str = r#"<svg class="icon" viewBox="0 0 24 24" width="24" height="24" aria-hidden="true" focusable="false" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round"><path d="M3 6h11M3 12h15M3 18h8"/><path d="M17 4l3 2-3 2M20 10l1.5 2-1.5 2M14 16l3 2-3 2"/></svg>"#;

const LAYERS: &str = r#"<svg class="icon" viewBox="0 0 24 24" width="24" height="24" aria-hidden="true" focusable="false" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linejoin="round"><path d="M12 3l9 5-9 5-9-5z"/><path d="M3 13l9 5 9-5"/><path d="M3 17.5l9 5 9-5" opacity="0.6"/></svg>"#;
const DOWNLOAD: &str = r#"<svg class="icon" viewBox="0 0 24 24" width="24" height="24" aria-hidden="true" focusable="false" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v12M7 10l5 5 5-5"/><path d="M4 19h16"/></svg>"#;
const ROLLBACK: &str = r#"<svg class="icon" viewBox="0 0 24 24" width="24" height="24" aria-hidden="true" focusable="false" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d="M4 12a8 8 0 1 0 2.4-5.7"/><path d="M4 4v5h5"/></svg>"#;

/// The GitHub mark (Octicons `mark-github`, 16px grid), filled with currentColor.
const GITHUB: &str = r#"<svg class="icon icon--github" viewBox="0 0 16 16" width="18" height="18" aria-hidden="true" focusable="false" fill="currentColor"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/></svg>"#;

pub fn github() -> Markup {
    PreEscaped(GITHUB.to_owned())
}
pub fn sun() -> Markup {
    PreEscaped(SUN.to_owned())
}
pub fn moon() -> Markup {
    PreEscaped(MOON.to_owned())
}
pub fn menu() -> Markup {
    PreEscaped(MENU.to_owned())
}
pub fn arrow() -> Markup {
    PreEscaped(ARROW.to_owned())
}
pub fn shield() -> Markup {
    PreEscaped(SHIELD.to_owned())
}
pub fn bolt() -> Markup {
    PreEscaped(BOLT.to_owned())
}
pub fn cube() -> Markup {
    PreEscaped(CUBE.to_owned())
}
pub fn streams() -> Markup {
    PreEscaped(STREAMS.to_owned())
}
pub fn layers() -> Markup {
    PreEscaped(LAYERS.to_owned())
}
pub fn download() -> Markup {
    PreEscaped(DOWNLOAD.to_owned())
}
pub fn rollback() -> Markup {
    PreEscaped(ROLLBACK.to_owned())
}
