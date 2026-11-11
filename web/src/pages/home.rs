//! Home page: the iron-filings hero, the app showcase, why native, install.

use maud::{Markup, html};

use super::feature_demos;
use crate::components::{
    button::{self, Variant},
    card,
    follow::{self, Size},
};
use crate::layout::{PageMeta, page};
use crate::{products, site};

const DESCRIPTION: &str = "Ferrum is a family of native developer tools written in Rust and \
     rendered with GPUI: Ferrum Editor, FerrumAPI, FerrumGIT, FerrumData, FerrumTerm, FerrumKit, \
     FerrumDock, FerrumPorts and FerrumEnv, installed and updated by Ferrum Launcher.";

/// (title, body) pairs for the "native" section.
const NATIVE: [(&str, &str); 4] = [
    (
        "Drawn on the GPU",
        "Every app renders with GPUI, the framework behind Zed, so long lists, big result \
         grids and deep histories scroll without a stutter.",
    ),
    (
        "No browser inside",
        "No Electron, no webview, no JavaScript runtime. Compiled Rust talking straight to \
         your machine, starting in a blink.",
    ),
    (
        "Your work stays local",
        "Requests, queries, repositories and terminal sessions live on your Mac. Nothing \
         needs an account.",
    ),
    (
        "One family",
        "The apps share one UI crate and one theme, and open each other. Most have a project \
         strip on top: switch projects with ⌥⌘O and open the current one in Ferrum Editor, \
         FerrumGIT or FerrumTerm. Every app installs from one launcher.",
    ),
];

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(None, DESCRIPTION, "/", "page-home");
    page(
        site_url,
        &meta,
        html! {
            (hero())
            (showcase())
            (native_section())
            (install_section())
            div class="container" {
                (follow::block(Size::Compact, true))
            }
        },
    )
}

/// The one-line Terminal install with a copy button (js/site.js).
pub fn install_command() -> Markup {
    html! {
        div class="install-cmd" {
            code { (site::MACOS_INSTALL_COMMAND) }
            button class="install-cmd__copy" type="button"
                data-copy=(site::MACOS_INSTALL_COMMAND) { "Copy" }
        }
    }
}

fn hero() -> Markup {
    // The canvas is the iron-filings field drawn by js/filings.js.
    html! {
        section class="hero" aria-labelledby="hero-title" {
            canvas class="hero__field" data-filings aria-hidden="true" {}
            div class="container hero__inner" {
                h1 class="hero__title" id="hero-title" {
                    span class="hero__line" { "Your daily tools," }
                    " "
                    span class="hero__line" { "rebuilt in Rust." }
                }
                p class="hero__lead" {
                    "A hackable code editor with CI and logs built in, API client, Git client, "
                    "database IDE, terminal, container manager and utilities, plus tools for "
                    "ports and environment variables. Native apps drawn on the GPU, installed and "
                    "kept up to date by Ferrum Launcher."
                }
                div class="hero__ctas" {
                    (button::link("Download for macOS", site::MACOS_DOWNLOAD_PATH, Variant::Primary))
                    (button::link("See what's new", super::features::PATH, Variant::Secondary))
                }
                (install_command())
                p class="hero__note" {
                    "Free for Apple Silicon Macs. Signed and notarized by Apple."
                }
            }
        }
    }
}

fn showcase() -> Markup {
    html! {
        section class="section showcase" id="apps" aria-labelledby="apps-title" {
            div class="container" {
                div class="section-header" {
                    h2 id="apps-title" { "Ten apps that work as one" }
                    p class="lead" {
                        "Pick an app to see it at work. Each one is a native desktop app with the \
                         same keyboard-first feel."
                    }
                }
                div class="showcase__body" data-showcase {
                    div class="showcase__tabs" role="tablist" aria-label="Ferrum apps"
                        aria-orientation="vertical" {
                        @for (i, p) in products::ALL.iter().enumerate() {
                            button class="showcase__tab" type="button" role="tab"
                                id={ "tab-" (p.slug) } aria-controls={ "panel-" (p.slug) }
                                aria-selected=(if i == 0 { "true" } else { "false" })
                                tabindex=(if i == 0 { "0" } else { "-1" })
                                style=(p.accent_style()) {
                                (card::app_logo(p, "app-logo--xs", true))
                                span class="showcase__name" { (p.name) }
                                span class="showcase__kind" { (p.kind) }
                            }
                        }
                    }
                    @for (i, p) in products::ALL.iter().enumerate() {
                        div class="showcase__panel" role="tabpanel" id={ "panel-" (p.slug) }
                            aria-labelledby={ "tab-" (p.slug) } hidden[i > 0]
                            style=(p.accent_style()) {
                            div class="showcase__demo" role="img"
                                aria-label={ "Animated preview of " (p.name) } {
                                @if let Some(demo) = feature_demos::for_slug(p.slug) { (demo) }
                            }
                            div class="showcase__info" {
                                p { (p.tagline) }
                                a href=(p.path()) { "More about " (p.name) }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn native_section() -> Markup {
    html! {
        section class="section native" aria-labelledby="native-title" {
            div class="container native__inner" {
                h2 id="native-title" { "Native, not a browser in a box" }
                dl class="native__list" {
                    @for (title, body) in NATIVE {
                        div {
                            dt { (title) }
                            dd { (body) }
                        }
                    }
                }
            }
        }
    }
}

fn install_section() -> Markup {
    let steps = [
        (
            "Get Ferrum Launcher",
            html! { p { "Run the command above, or download the DMG and drag it to Applications." } },
        ),
        (
            "Open it",
            html! { p { "No account and no sign-in. The launcher lists every Ferrum app." } },
        ),
        (
            "Install the apps",
            html! { p { "Pick any app and click Install. It lands in Applications, ready to launch." } },
        ),
        (
            "Stay current",
            html! { p { "New versions show up on their own, and the launcher updates itself in place." } },
        ),
    ];
    html! {
        section class="section install-flow" id="install" aria-labelledby="install-title" {
            div class="container" {
                div class="section-header" {
                    h2 id="install-title" { "Every tool in four steps" }
                    p class="lead" {
                        "Every app is in alpha: usable every day, released often, with rough \
                         edges in the interface. Windows and Linux builds are coming."
                    }
                }
                (card::steps(&steps))
                div class="install-flow__cta" {
                    (button::link("Download & install guide", site::DOWNLOAD_PATH, Variant::Secondary))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn showcases_every_app_with_a_demo_and_link() {
        let html = render("https://example.test").into_string();
        for p in products::ALL {
            assert!(html.contains(p.name), "missing {}", p.name);
            assert!(html.contains(&format!(r#"href="{}""#, p.path())));
            assert!(html.contains(&format!("/assets/{}?v=", p.logo_asset())));
            assert!(html.contains(&format!(r#"id="panel-{}""#, p.slug)));
        }
        assert_eq!(html.matches(r#"class="demo""#).count(), products::ALL.len());
        assert_eq!(html.matches(r#"aria-selected="true""#).count(), 1);
        assert!(html.contains(r#"id="apps""#));
        assert_eq!(html.matches("<h1").count(), 1);
    }

    #[test]
    fn hero_offers_download_and_the_install_command() {
        let html = render("https://example.test").into_string();
        assert!(html.contains("data-filings"));
        assert!(html.contains(site::MACOS_DOWNLOAD_PATH));
        assert!(html.contains(r#"data-copy="curl -fsSL"#));
        assert!(html.contains(r#"class="steps""#));
        assert!(html.contains(r#"href="/download""#));
    }

    #[test]
    fn ends_with_the_follow_block() {
        let html = render("https://example.test").into_string();
        assert!(html.contains(r#"id="follow""#));
        assert!(html.contains("follow--compact"));
        assert!(html.contains(r#"href="/updates/feed.xml""#));
        assert!(!html.contains("<form"));
    }
}
