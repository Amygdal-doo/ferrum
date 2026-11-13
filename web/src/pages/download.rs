//! Download page: the macOS launcher installer (public, no sign-in), then how
//! the launcher installs and updates the apps.

use maud::{Markup, html};

use crate::components::{
    breadcrumb::Crumb,
    button::{self, Variant},
    callout::{self, Kind},
    card, code,
    page_header::PageHeader,
    section::Section,
};
use crate::layout::{PageMeta, page};
use crate::{products, site};

const DESCRIPTION: &str = "Download Ferrum Launcher for macOS (Apple Silicon) and install every \
     Ferrum app from it. Free public builds: no GitHub account, no sign-in.";

/// Per-OS launcher installers: (system, file, how to install, published yet).
/// Only the launcher has installers; it installs the other apps.
const INSTALLERS: [(&str, &str, &str, bool); 3] = [
    (
        "macOS",
        "FerrumLauncher-<version>-aarch64.dmg",
        "open it and drag Ferrum Launcher onto Applications. Apple Silicon only.",
        true,
    ),
    (
        "Windows",
        "FerrumLauncher-<version>-x86_64-setup.exe",
        "coming soon.",
        false,
    ),
    (
        "Linux",
        "ferrum-launcher_<version>_amd64.deb",
        "coming soon (Debian and Ubuntu).",
        false,
    ),
];

fn get_launcher_steps() -> [(&'static str, Markup); 3] {
    [
        (
            "Download the launcher",
            html! {
                p {
                    "Click " strong { "Download for macOS" } " to get the newest "
                    code { "FerrumLauncher-<version>-aarch64.dmg" } ". It needs a Mac with Apple \
                    Silicon. Every build is also listed in the "
                    a href=(site::RELEASES_URL) rel="noopener" { "public releases" }
                    " on GitHub."
                }
            },
        ),
        (
            "Install it",
            html! {
                p { "Open the DMG and drag " strong { "Ferrum Launcher.app" } " to Applications." }
            },
        ),
        (
            "Open Ferrum Launcher",
            html! {
                p {
                    "Open it from Applications. There is nothing to sign in to: it downloads \
                    every app straight from the public releases."
                    @if !site::MACOS_NOTARIZED {
                        " The current build needs one extra click the first time, see below."
                    }
                }
            },
        ),
    ]
}

/// Gatekeeper workaround for builds that are not notarized yet.
/// Hidden once [`site::MACOS_NOTARIZED`] is `true`.
fn gatekeeper_section() -> Markup {
    if site::MACOS_NOTARIZED {
        return html! {};
    }
    html! {
        section class="section section--tight" aria-labelledby="gatekeeper-title" {
            div class="container" {
                div class="callout callout--warn" {
                    h2 id="gatekeeper-title" { "First launch: this build isn't notarized yet" }
                    p {
                        "The current macOS build is ad-hoc signed but not yet notarized by \
                        Apple, so a DMG downloaded in the browser is blocked on first launch \
                        with a \"could not verify\" malware warning. The easiest way around \
                        it is to install from Terminal instead; nothing downloaded this way is \
                        quarantined, so the launcher opens straight away:"
                    }
                    pre class="code-block" {
                        code { (site::MACOS_INSTALL_COMMAND) }
                    }
                    p {
                        "Already downloaded the DMG? Click Done on the warning, then open "
                        strong { "System Settings → Privacy & Security" } " and click "
                        strong { "Open Anyway" } ". (On macOS 14 and older, right-click the \
                        app and choose Open.) Or clear the quarantine flag:"
                    }
                    pre class="code-block" {
                        code { "xattr -dr com.apple.quarantine \"/Applications/Ferrum Launcher.app\"" }
                    }
                    p class="muted" {
                        "You only need to do this once. Apps the launcher installs, and the \
                        launcher's own updates, are downloaded without the quarantine flag, \
                        so they open without asking. Signed and notarized builds are on the way."
                    }
                }
            }
        }
    }
}

fn use_launcher_steps() -> [(&'static str, Markup); 3] {
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
                    "The launcher checks for new releases on startup and every few hours and "
                    "shows an update badge next to each app. It updates itself in place too: "
                    "download, replace, restart."
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
    let launcher = products::FERRUM_LAUNCHER;
    let meta = PageMeta::new(
        Some("Download Ferrum Launcher for macOS"),
        DESCRIPTION,
        site::DOWNLOAD_PATH,
        "page-download",
    )
    .trail(vec![Crumb::new("Download", site::DOWNLOAD_PATH)]);
    page(
        site_url,
        &meta,
        html! {
            (PageHeader::new(&meta.trail, "Get Ferrum Launcher")
                .accent(launcher.accent_style())
                .media(card::app_logo(&launcher, "app-logo--hero", false))
                .lead(html! {
                    p {
                        "Install the launcher once, then install, update and roll back every \
                         Ferrum app from one window. Free for Apple Silicon Macs, no sign-in."
                    }
                })
                .actions(html! {
                    (button::link("Download for macOS", site::MACOS_DOWNLOAD_PATH, Variant::Primary))
                    (button::disabled("Windows", "Coming soon"))
                    (button::disabled("Linux", "Coming soon"))
                })
                .render())
            (Section::new("terminal", "Or install from Terminal")
                .tight()
                .lead(html! {
                    p {
                        "One line downloads the newest launcher, checks its checksum and opens \
                         it. The "
                        a href="/docs/install" { "install guide" } " explains every step."
                    }
                })
                .render(html! {
                    (code::command(site::MACOS_INSTALL_COMMAND))
                    div class="download-note" {
                        (callout::render(Kind::Note, Some("Public builds"), html! {
                            p {
                                "Releases are published in the public "
                                code { (site::REPO_SLUG) } " repository on GitHub. You don't \
                                 need a GitHub account, and the launcher installs every other app \
                                 without signing in."
                            }
                        }))
                    }
                }))
            (Section::new("get", "Install Ferrum Launcher")
                .render(card::steps(&get_launcher_steps())))
            (gatekeeper_section())
            (Section::new("use", "Install the apps")
                .band()
                .render(html! {
                    (card::steps(&use_launcher_steps()))
                    (card::app_strip(products::apps(), false))
                }))
            (Section::new("platforms", "Launcher installers")
                .tight()
                .lead(html! {
                    p {
                        "The launcher is the only thing you download by hand. Ferrum Editor, \
                         FerrumAgentic, FerrumAPI, FerrumGIT, FerrumData, FerrumTerm, FerrumKit, \
                         FerrumContainers, FerrumPorts and FerrumEnv have no separate installers: \
                         install, update and remove them from the launcher."
                    }
                })
                .render(html! {
                    dl class="spec-sheet" {
                        @for (os, file, how, published) in INSTALLERS {
                            div class="spec-sheet__row" {
                                dt { (os) }
                                dd {
                                    @if published { code { (file) } ": " }
                                    (how)
                                }
                            }
                        }
                    }
                    p class="muted" { "FerrumTerm isn't available on Windows yet." }
                    div class="download-actions" {
                        (button::link("All releases on GitHub", site::RELEASES_URL, Variant::Secondary))
                        (button::link("Read the changelog", "/changelog", Variant::Secondary))
                        (button::link("About Ferrum Launcher", &launcher.path(), Variant::Ghost))
                    }
                }))
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
    fn offers_a_public_macos_download() {
        let html = rendered();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains(r#"href="/download/macos""#));
        assert!(html.contains("Apple Silicon"));
        assert!(html.contains(site::RELEASES_URL));
        assert!(html.contains("Amygdal-doo/ferrum-releases"));
        assert_eq!(html.matches(r#"class="btn__note">Coming soon"#).count(), 2);
        for gone in [
            "private",
            "gh auth login",
            "request access",
            "fine-grained token",
        ] {
            assert!(!html.to_lowercase().contains(gone), "{gone}");
        }
    }

    #[test]
    fn explains_gatekeeper_until_notarized() {
        let html = rendered();
        assert_eq!(
            html.contains("com.apple.quarantine"),
            !site::MACOS_NOTARIZED
        );
        assert_eq!(html.contains("Open Anyway"), !site::MACOS_NOTARIZED);
        assert!(html.contains("ferrum-releases/main/install.sh | bash"));
    }

    #[test]
    fn lists_an_installer_per_os() {
        let html = rendered();
        assert!(html.contains("Launcher installers"));
        assert!(html.contains("FerrumLauncher-&lt;version&gt;-aarch64.dmg"));
        assert!(!html.contains("x86_64.dmg"));
        for os in ["macOS", "Windows", "Linux"] {
            assert!(html.contains(&format!(r#"<dt>{os}</dt>"#)), "{os}");
        }
    }

    #[test]
    fn links_every_managed_app() {
        let html = rendered();
        for p in products::apps() {
            assert!(
                html.contains(&format!(r#"href="{}""#, p.path())),
                "{}",
                p.name
            );
        }
    }
}
