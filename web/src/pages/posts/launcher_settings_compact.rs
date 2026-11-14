//! "Launcher settings in one place, and a real Compact mode" (2026-11-14).
//! Claims follow `launcher/apps/desktop/src/settings_page.rs` and
//! `shared/ui/src/window_chrome.rs`.

use maud::{Markup, html};

use super::product_row;
use crate::products::{
    FERRUM_EDITOR, FERRUM_LAUNCHER, FERRUMAGENTIC, FERRUMAPI, FERRUMCONTAINERS, FERRUMDATA,
    FERRUMENV, FERRUMGIT, FERRUMKIT, FERRUMPORTS, FERRUMTERM, Product,
};
use crate::updates::Post;

pub const POST: Post = Post {
    slug: "launcher-settings-and-compact-mode",
    title: "Launcher settings in one place, and a real Compact mode",
    date: "2026-11-14",
    byline: "Ferrum Team",
    summary: "Ferrum Launcher's options move into the Settings window, and Compact makes \
              every app's title rows, sidebar rows and tabs smaller.",
    body,
};

/// Today's releases: (product, version, one line).
const RELEASES: [(Product, &str, &str); 11] = [
    (
        FERRUM_LAUNCHER,
        "0.12.1",
        "Settings › Launcher: menu bar, Dock, update checks.",
    ),
    (FERRUM_EDITOR, "0.8.1-alpha.1", "Compact mode."),
    (FERRUMAGENTIC, "0.4.1-alpha.1", "Compact mode."),
    (FERRUMGIT, "0.6.1-alpha.1", "Compact mode."),
    (FERRUMTERM, "0.9.3-alpha.1", "Compact mode."),
    (FERRUMCONTAINERS, "0.4.1-alpha.1", "Compact mode."),
    (FERRUMPORTS, "0.3.1-alpha.1", "Compact mode."),
    (FERRUMAPI, "0.5.2-alpha.1", "Compact mode."),
    (FERRUMDATA, "0.5.2-alpha.1", "Compact mode."),
    (FERRUMENV, "0.2.2-alpha.1", "Compact mode."),
    (FERRUMKIT, "0.2.2-alpha.1", "Compact mode."),
];

fn body() -> Markup {
    html! {
        p class="post__intro" {
            "Two small changes you'll notice in every app."
        }
        ul class="post-apps" {
            @for (p, version, text) in RELEASES {
                (product_row(&p, &format!("{version}. {text}")))
            }
        }

        h2 id="launcher" { "Settings › Launcher" }
        p {
            "Ferrum Launcher's own settings now live in the Settings window (⌘,) next to \
             Interface and Themes: show the launcher in the menu bar, hide it from the Dock, \
             how often to check for updates and how many versions to keep. The extra settings \
             button next to Check for updates is gone. Turning the menu bar icon off brings \
             the Dock icon back, so the launcher is always reachable."
        }

        h2 id="compact" { "Comfortable and Compact" }
        p {
            "Settings › Interface › Toolbar style works again with the new layout. Compact \
             makes title rows 32 pixels instead of 40, and makes sidebar rows, tabs and the \
             project row smaller; the window buttons move to stay centred. Every Ferrum app \
             switches at once."
        }
        p {
            "Every release is also on the " a href=(crate::pages::changelog::PATH) { "changelog" } "."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_todays_releases_with_links() {
        let html = body().into_string();
        for (p, version, _) in RELEASES {
            assert!(
                html.contains(&format!(r#"href="{}""#, p.path())),
                "{}",
                p.name
            );
            assert!(html.contains(version), "{version}");
        }
    }
}
