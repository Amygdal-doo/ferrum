//! "Every app keeps its own theme" (2026-11-14). Claims follow
//! `shared/ui/src/appearance.rs`, `shared/ui/src/settings/` and
//! `launcher/apps/desktop/src/settings_page.rs`.

use maud::{Markup, html};

use super::product_row;
use crate::products::{
    FERRUM_EDITOR, FERRUM_LAUNCHER, FERRUMAPI, FERRUMCONTAINERS, FERRUMDATA, FERRUMENV, FERRUMGIT,
    FERRUMKIT, FERRUMPORTS, FERRUMTERM, FERRUMVIBE, Product,
};
use crate::updates::Post;

pub const POST: Post = Post {
    slug: "every-app-keeps-its-own-theme",
    title: "Every app keeps its own theme",
    date: "2026-11-14",
    byline: "Ferrum Team",
    summary: "A theme picked in one Ferrum app stays in that app, and Ferrum Launcher has \
              one Settings button with the menu bar switch next to Show in Dock.",
    body,
};

/// The releases with this change: (product, version, one line).
const RELEASES: [(Product, &str, &str); 11] = [
    (
        FERRUM_LAUNCHER,
        "0.12.2",
        "One Settings button; Show in menu bar under Interface › Window.",
    ),
    (FERRUM_EDITOR, "0.8.1-alpha.1", "Its own theme."),
    (FERRUMVIBE, "0.4.2-alpha.1", "Its own theme."),
    (FERRUMGIT, "0.6.1-alpha.1", "Its own theme."),
    (FERRUMTERM, "0.9.3-alpha.1", "Its own theme."),
    (FERRUMCONTAINERS, "0.4.1-alpha.1", "Its own theme."),
    (FERRUMPORTS, "0.3.1-alpha.1", "Its own theme."),
    (FERRUMAPI, "0.5.2-alpha.1", "Its own theme."),
    (FERRUMDATA, "0.5.2-alpha.1", "Its own theme."),
    (FERRUMENV, "0.2.2-alpha.1", "Its own theme."),
    (FERRUMKIT, "0.2.2-alpha.1", "Its own theme."),
];

fn body() -> Markup {
    html! {
        p class="post__intro" {
            "Pick a theme in Ferrum Editor and it stays there, whatever you choose in FerrumAPI."
        }
        ul class="post-apps" {
            @for (p, version, text) in RELEASES {
                (product_row(&p, &format!("{version}. {text}")))
            }
        }

        h2 id="themes" { "A theme per app" }
        p {
            "Until now Settings › Themes changed every Ferrum app at once. Now the colour \
             scheme (Auto, Light or Dark) and the light and dark theme belong to the app you \
             set them in. Interface style, toolbar density, title bar and text size are still \
             shared, so every window keeps the same feel."
        }
        p {
            "Nothing changes when you update: an app you never picked a theme in keeps the \
             one you use today."
        }

        h2 id="launcher" { "One Settings button in Ferrum Launcher" }
        p {
            "The launcher's title row now has a single gear that opens Settings (⌘,). \
             Show in menu bar sits under Interface › Window, right below Show in Dock. Hiding \
             the launcher from the Dock still turns its menu bar icon on, so you can always \
             get back to it. Settings › Launcher keeps update checks and how many versions \
             to keep."
        }

        h2 id="settings-window" { "A see-through Settings window" }
        p {
            "The Settings window in every app drops its dark title strip. With Glass or \
             Gradient it now looks like the app's own windows, and you drag it by the \
             sidebar or the page header."
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
    fn lists_the_releases_with_links() {
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
