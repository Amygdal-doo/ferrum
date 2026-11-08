//! Download page: how to get Ferrum Toolbox while builds are private, then
//! how the toolbox installs and updates the apps.

use maud::{Markup, html};

use crate::components::{
    button::{self, Variant},
    card,
};
use crate::layout::{PageMeta, page};
use crate::{products, site};

const DESCRIPTION: &str = "Download Ferrum Toolbox and install every Ferrum app. Builds are \
     private GitHub Releases for now: request access, sign in with gh, install the toolbox.";

fn get_toolbox_steps() -> [(&'static str, Markup); 4] {
    [
        (
            "Request access",
            html! {
                p {
                    "Ferrum builds are published as GitHub Releases in the private "
                    code { (site::REPO_SLUG) } " repository. Ask a maintainer to add your \
                    GitHub account with read access."
                }
            },
        ),
        (
            "Sign in with the GitHub CLI",
            html! {
                p { "Install " a href="https://cli.github.com" rel="noopener" { "gh" } ", then sign in once:" }
                pre class="code-block" { code { "gh auth login" } }
            },
        ),
        (
            "Download the toolbox",
            html! {
                p {
                    "Open the "
                    a href=(site::RELEASES_URL) rel="noopener" { "Ferrum releases" }
                    " (you must be signed in to GitHub), find the newest "
                    code { "toolbox-v…" } " release and download "
                    code { "FerrumToolbox-<version>-<arch>.dmg" }
                    " for Apple Silicon or Intel. Or, from a clone of the repository:"
                }
                pre class="code-block" { code { "scripts/install-toolbox.sh" } }
                p class="muted" {
                    "The script fetches the right DMG for your Mac with gh and installs it."
                }
            },
        ),
        (
            "Open Ferrum Toolbox",
            html! {
                p {
                    "Drag " strong { "Ferrum Toolbox.app" } " to Applications and open it. It \
                    picks up your gh login automatically, or you can paste a "
                    a href=(site::TOKEN_SETTINGS_URL) rel="noopener" { "fine-grained token" }
                    " with " em { "Contents: Read" } " on " code { (site::REPO_SLUG) } "."
                }
            },
        ),
    ]
}

fn use_toolbox_steps() -> [(&'static str, Markup); 3] {
    [
        (
            "Install any app",
            html! {
                p {
                    "Click " strong { "Install" } ". Apps are installed into "
                    code { "~/Applications/Ferrum" } ", so Launchpad and Spotlight find them, and "
                    "their command-line tools are linked into " code { "~/.ferrum/bin" } "."
                }
            },
        ),
        (
            "Stay up to date",
            html! {
                p {
                    "The toolbox checks for new releases on startup and every few hours, and "
                    "shows an update badge next to each app."
                }
            },
        ),
        (
            "Roll back or uninstall",
            html! {
                p {
                    "Every release is listed with its notes. Activate an older version, or "
                    "remove the ones you don't need; the last three are kept by default."
                }
            },
        ),
    ]
}

pub fn render(site_url: &str) -> Markup {
    let toolbox = products::FERRUM_TOOLBOX;
    let meta = PageMeta::new(
        Some("Download"),
        DESCRIPTION,
        site::DOWNLOAD_PATH,
        "page-download",
    );
    page(
        site_url,
        &meta,
        html! {
            section class="download-hero" style=(toolbox.accent_style()) aria-labelledby="download-title" {
                div class="container download-hero__inner" {
                    (card::app_logo(&toolbox, "app-logo--hero", false))
                    div {
                        p class="eyebrow" { "Download" }
                        h1 id="download-title" { "Get Ferrum Toolbox" }
                        p class="lead" {
                            "Install the toolbox once, then install, update and roll back every "
                            "Ferrum app from one window."
                        }
                        div class="callout" role="note" {
                            p {
                                strong { "Builds are private for now." }
                                " Ferrum releases are available to the team only, so you need "
                                "access to " code { (site::REPO_SLUG) } " on GitHub."
                            }
                        }
                    }
                }
            }
            section class="section" aria-labelledby="get-title" {
                div class="container" {
                    p class="eyebrow" { "Step by step" }
                    h2 id="get-title" { "Install Ferrum Toolbox" }
                    (card::steps(&get_toolbox_steps()))
                }
            }
            section class="section section--tight" aria-labelledby="gatekeeper-title" {
                div class="container" {
                    div class="callout callout--warn" {
                        h2 id="gatekeeper-title" { "First launch on macOS" }
                        p {
                            "Ferrum builds are ad-hoc signed but not notarized yet, so macOS "
                            "Gatekeeper blocks the first launch. Right-click the app in Finder "
                            "and choose " strong { "Open" } ", then confirm. Or clear the "
                            "quarantine flag:"
                        }
                        pre class="code-block" {
                            code { "xattr -dr com.apple.quarantine \"/Applications/Ferrum Toolbox.app\"" }
                        }
                        p class="muted" {
                            "You only need to do this once per app. Apps installed by the "
                            "toolbox may ask the same the first time you open them."
                        }
                    }
                }
            }
            section class="section" aria-labelledby="use-title" {
                div class="container" {
                    p class="eyebrow" { "Then" }
                    h2 id="use-title" { "Install the apps" }
                    (card::steps(&use_toolbox_steps()))
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
            section class="section section--tight" aria-labelledby="platforms-title" {
                div class="container" {
                    h2 id="platforms-title" { "Platforms" }
                    dl class="spec-sheet" {
                        div class="spec-sheet__row" {
                            dt class="mono" { "macOS" }
                            dd { "Apple Silicon and Intel. The toolbox ships as a DMG; apps install as .app bundles." }
                        }
                        div class="spec-sheet__row" {
                            dt class="mono" { "Linux" }
                            dd { "x86_64 builds as .tar.gz archives." }
                        }
                        div class="spec-sheet__row" {
                            dt class="mono" { "Windows" }
                            dd { "No builds yet." }
                        }
                    }
                    div class="download-actions" {
                        (button::link("Open Ferrum releases on GitHub", site::RELEASES_URL, Variant::Primary))
                        (button::link("About Ferrum Toolbox", &toolbox.path(), Variant::Secondary))
                    }
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered() -> String {
        render("https://example.test").into_string()
    }

    #[test]
    fn explains_private_access_and_the_steps() {
        let html = rendered();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains("Builds are private for now."));
        assert!(html.contains("gh auth login"));
        assert!(html.contains(site::RELEASES_URL));
        assert!(html.contains("scripts/install-toolbox.sh"));
        assert!(html.contains("com.apple.quarantine"));
    }

    #[test]
    fn links_every_managed_app() {
        let html = rendered();
        for p in products::apps() {
            assert!(html.contains(&format!(r#"href="{}""#, p.path())), "{}", p.name);
        }
    }
}
