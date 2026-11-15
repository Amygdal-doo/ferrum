//! Ferrum Launcher product page. Claims follow the launcher README.

use maud::Markup;

use super::product_sections::{self as sections, BulletGroup, ProductPage};
use crate::components::{card, section::Section};
use crate::products::{self, FERRUM_LAUNCHER};

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
        "Automatic updates",
        "Checks on startup and every few hours. The launcher updates itself in place: it \
         downloads the new version, replaces the app and restarts.",
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
    "Ferrum Editor, FerrumVibe, FerrumAPI, FerrumGIT, FerrumData, FerrumTerm, FerrumKit, \
     FerrumContainers, FerrumPorts and FerrumEnv",
    "Itself: new launcher versions install in place and restart the app",
    "Keeps the last 3 versions of each app by default",
    "Shell commands like `fee .` to open Ferrum Editor on the current folder, `feg .` for \
     FerrumGIT or `fet` for FerrumTerm, `fev` for FerrumVibe, `fes` for Ferrum System, and `fep` and `fen` for \
     FerrumPorts and FerrumEnv, each turned on per app",
    "Global shortcuts, a menu bar icon and hiding any app from the Dock (macOS)",
];

const AUTH: &[&str] = &[
    "No sign-in: apps download from public GitHub Releases",
    "An optional GitHub token only for private forks or higher API rate limits",
    "A token, if set, is never written to the launcher state file or to logs",
];

const PAGE: ProductPage = ProductPage {
    product: FERRUM_LAUNCHER,
    meta_description: "Ferrum Launcher installs, updates, rolls back and launches every Ferrum \
                       app from one native GPUI window.",
    intro: "One app to manage the whole Ferrum family, in the spirit of an IDE launcher. Every \
            version of every app is a GitHub Release; the launcher installs the one you pick \
            and keeps the others a click away.",
    features_lead: "Install the launcher once; it takes care of the rest.",
    features: FEATURES,
    screenshots: &["App list with update badges and the version history of one app"],
    status_summary: "Ferrum Launcher is in alpha. Its install, checksum, unpacking and \
                     version-pruning logic is covered by automated tests against a local fake \
                     release server.",
    status_groups: &[
        BulletGroup {
            heading: "What it manages",
            items: MANAGES,
        },
        BulletGroup {
            heading: "No sign-in",
            items: AUTH,
        },
    ],
    specs: &[
        ("Get it", "Free DMG for Apple Silicon Macs (see Download)"),
        ("Installers", "macOS DMG and Linux .deb"),
        (
            "Releases",
            "Tagged <app>-v<semver> in Amygdal-doo/ferrum-releases",
        ),
        ("Built with", "Rust, GPUI, reqwest + rustls"),
    ],
    specs_note: "The macOS build is signed with a Developer ID and notarized by Apple, so it \
                 opens without a Gatekeeper warning.",
};

/// The managed apps, shown right after the preview.
fn managed_apps() -> Markup {
    Section::new("managed", "Apps it installs")
        .tight()
        .render(card::app_strip(products::apps(), true))
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
        assert!(html.contains(">Ferrum Launcher</h1>"));
        for p in products::apps() {
            assert!(
                html.contains(&format!("href=\"{}\"", p.path())),
                "{}",
                p.name
            );
        }
    }

    #[test]
    fn explains_public_downloads_and_self_update() {
        let html = rendered();
        assert!(html.contains("No sign-in"));
        assert!(html.contains("updates itself in place"));
        assert!(html.contains("Amygdal-doo/ferrum-releases"));
        assert!(!html.contains("private GitHub Releases"));
        assert!(!html.contains("gh CLI"));
        assert!(html.contains("href=\"/download\""));
    }
}
