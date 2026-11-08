//! Ferrum Toolbox product page. Claims follow the toolbox README.

use maud::{Markup, html};

use super::product_sections::{self as sections, BulletGroup, ProductPage};
use crate::components::card;
use crate::products::{self, FERRUM_TOOLBOX};

const FEATURES: &[(&str, &str)] = &[
    (
        "Every app in one window",
        "See each Ferrum app with its active version, or \"Not installed\", and an update \
         badge when a newer release exists.",
    ),
    (
        "One-click install and update",
        "Download progress with cancel, several apps at once, and checksum verification \
         before anything is installed.",
    ),
    (
        "Roll back any time",
        "Every release is listed with its date, size and notes. Install, activate or remove \
         any version.",
    ),
    (
        "Automatic update checks",
        "Checks on startup and every few hours, and tells you when new versions are out.",
    ),
    (
        "Command-line links",
        "The active version of each app is linked into ~/.ferrum/bin, so ferrumapi, \
         ferrumgit and the rest run from a shell.",
    ),
    (
        "Safe installs",
        "Archives are unpacked into a staging folder and moved into place, so a failed or \
         cancelled install leaves nothing behind.",
    ),
];

const MANAGES: &[&str] = &[
    "Ferrum IDE, FerrumAPI, FerrumGIT, FerrumData and FerrumTerm",
    "Itself: new toolbox versions show up like any other app",
    "Keeps the last 3 versions of each app by default",
];

const AUTH: &[&str] = &[
    "Builds live in private GitHub Releases, so the toolbox needs read access",
    "Uses your gh CLI login, a GITHUB_TOKEN, or a token saved in the OS keychain",
    "The token is never written to the toolbox state file or to logs",
];

const PAGE: ProductPage = ProductPage {
    product: FERRUM_TOOLBOX,
    meta_description: "Ferrum Toolbox installs, updates, rolls back and launches every Ferrum \
                       app from one native GPUI window.",
    intro: "One app to manage the whole Ferrum family, in the spirit of an IDE toolbox. Every \
            version of every app is a GitHub Release; the toolbox installs the one you pick \
            and keeps the others a click away.",
    features_lead: "Install the toolbox once; it takes care of the rest.",
    features: FEATURES,
    screenshots: &["App list with update badges and the version history of one app"],
    status_summary: "Ferrum Toolbox is an early preview. Its install, checksum, unpacking and \
                     version-pruning logic is covered by automated tests against a local fake \
                     release server.",
    status_groups: &[
        BulletGroup {
            heading: "What it manages",
            items: MANAGES,
        },
        BulletGroup {
            heading: "Access",
            items: AUTH,
        },
    ],
    specs: &[
        ("Get it", "The toolbox DMG from GitHub Releases (team access, see Download)"),
        ("Builds", "macOS (Apple Silicon and Intel) and Linux x86_64"),
        ("Releases", "Tagged <app>-v<semver> in Amygdal-doo/ferrum"),
        ("Built with", "Rust, GPUI, reqwest + rustls"),
    ],
    specs_note: "Builds are ad-hoc signed, not notarized. See the Download page for the \
                 first-launch step on macOS.",
};

/// The five managed apps, shown right after the hero.
fn managed_apps() -> Markup {
    html! {
        section class="section section--tight" aria-labelledby="managed-title" {
            div class="container" {
                h2 id="managed-title" { "Apps it installs" }
                ul class="app-strip" {
                    @for p in products::apps() {
                        li style=(p.accent_style()) {
                            a class="app-strip__link" href=(p.path()) {
                                (card::app_logo(p, "app-logo--sm", true))
                                span { (p.name) }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn render(site_url: &str) -> Markup {
    sections::render(site_url, &PAGE, managed_apps())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered() -> String {
        render("https://example.test").into_string()
    }

    #[test]
    fn renders_single_h1_and_lists_managed_apps() {
        let html = rendered();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains(">Ferrum Toolbox</h1>"));
        for p in products::apps() {
            assert!(html.contains(&format!("href=\"{}\"", p.path())), "{}", p.name);
        }
    }

    #[test]
    fn explains_private_access() {
        let html = rendered();
        assert!(html.contains("private GitHub Releases"));
        assert!(html.contains("href=\"/download\""));
    }
}
