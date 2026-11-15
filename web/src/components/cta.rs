//! Closing call to action: a heading, one line and up to two buttons.
//! Classes: `.cta`, `.cta__copy`, `.cta__title`, `.cta__actions`.

use maud::{Markup, html};

use super::button::{self, Variant};
use crate::site;

/// `id` labels the section (`{id}-title`).
pub fn band(id: &str, title: &str, text: &str, actions: Markup) -> Markup {
    let title_id = format!("{id}-title");
    html! {
        section class="cta" aria-labelledby=(title_id) {
            div class="container cta__inner" {
                div class="cta__copy" {
                    h2 class="cta__title" id=(title_id) { (title) }
                    p { (text) }
                }
                div class="cta__actions" { (actions) }
            }
        }
    }
}

/// The standard close of docs and info pages: get the launcher.
pub fn get_launcher() -> Markup {
    band(
        "get-ferrum",
        "Install FerrumLauncher",
        "One free download for macOS and Linux. The launcher installs and updates every app, and all of it is open source.",
        html! {
            (button::link(site::DOWNLOAD_LABEL, site::DOWNLOAD_PATH, Variant::Primary))
            (button::link("Read the install guide", "/docs/install", Variant::Secondary))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_cta_links_download_and_guide() {
        let html = get_launcher().into_string();
        assert!(html.contains(r#"href="/download""#));
        assert!(html.contains(r#"href="/docs/install""#));
        assert!(html.contains(r#"aria-labelledby="get-ferrum-title""#));
    }
}
