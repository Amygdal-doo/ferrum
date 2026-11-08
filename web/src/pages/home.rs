//! Home page: hero, the six apps, built on GPUI, install flow, honest status.

use maud::{Markup, html};

use crate::components::{
    button::{self, Variant},
    card, icons,
};
use crate::layout::{PageMeta, page};
use crate::routes::assets;
use crate::{products, site};

const DESCRIPTION: &str = "Ferrum is a family of native developer tools written in Rust and \
     rendered with GPUI: Ferrum IDE, FerrumAPI, FerrumGIT, FerrumData and FerrumTerm, \
     installed and updated by Ferrum Toolbox.";

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(None, DESCRIPTION, "/", "page-home");
    page(
        site_url,
        &meta,
        html! {
            (hero())
            (apps_section())
            (gpui_section())
            (install_section())
            (status_strip())
        },
    )
}

fn hero() -> Markup {
    html! {
        section class="hero" aria-labelledby="hero-title" {
            div class="container hero__inner" {
                div class="hero__copy" {
                    img class="hero__wordmark hero__wordmark--dark"
                        src=(assets::url("img/brand/ferrum-wordmark.svg"))
                        alt="Ferrum" width="390" height="160";
                    img class="hero__wordmark hero__wordmark--light"
                        src=(assets::url("img/brand/ferrum-wordmark-light.svg"))
                        alt="Ferrum" width="390" height="160";
                    h1 class="hero__title" id="hero-title" {
                        "One toolbox, " span class="hero__accent" { "every tool." }
                    }
                    p class="lead" {
                        "Ferrum is a family of native developer tools written in Rust: a code "
                        "editor, an API client, a Git client, a database IDE and a terminal. "
                        "Install them all, keep them updated and roll back with Ferrum Toolbox."
                    }
                    div class="hero__ctas" {
                        (button::link("Get Ferrum Toolbox", site::DOWNLOAD_PATH, Variant::Primary))
                        (button::link("Explore the apps", "#apps", Variant::Secondary))
                    }
                }
                (hero_visual())
            }
        }
    }
}

/// Decorative logo cluster: the toolbox in the middle of the five apps.
fn hero_visual() -> Markup {
    html! {
        div class="hero__visual" aria-hidden="true" {
            div class="logo-cluster" {
                @for p in products::ALL {
                    (card::app_logo(&p, "logo-cluster__item", true))
                }
            }
        }
    }
}

fn apps_section() -> Markup {
    html! {
        section class="section" id="apps" aria-labelledby="apps-title" {
            div class="container" {
                div class="section-header" {
                    p class="eyebrow" { "The family" }
                    h2 id="apps-title" { "Six apps, one alloy" }
                    p class="lead" {
                        "Each app is a native desktop application with the same look, the same \
                         keyboard-first feel and the same rule: your work stays on your machine."
                    }
                }
                div class="grid grid--3" {
                    @for p in products::ALL { (card::product_card(&p)) }
                }
            }
        }
    }
}

fn gpui_section() -> Markup {
    html! {
        section class="section built-on" aria-labelledby="gpui-title" {
            div class="container" {
                div class="section-header" {
                    p class="eyebrow" { "Built on GPUI" }
                    h2 id="gpui-title" { "The UI framework behind Zed, in every app" }
                    p class="lead" {
                        "Every Ferrum desktop app renders with "
                        a href=(site::GPUI_URL) rel="noopener" { "GPUI" }
                        ", taken from the Ferrum IDE fork of Zed. One framework, one theme, \
                         one set of components."
                    }
                }
                ul class="feature-list" {
                    (card::feature(icons::bolt(), "GPU-accelerated",
                        "The interface is drawn on the GPU, so long lists, big result grids and \
                         deep commit histories stay smooth."))
                    (card::feature(icons::cube(), "Native, no webview",
                        "No Electron, no bundled browser, no JavaScript runtime: compiled Rust \
                         talking straight to your machine."))
                    (card::feature(icons::layers(), "Shared Ferrum UI",
                        "Text inputs, dialogs, panes, notices and the FerrumIDE theme come from \
                         one shared crate, so the apps look and behave alike."))
                    (card::feature(icons::shield(), "Memory-safe Rust",
                        "Rust's ownership model rules out whole classes of memory bugs at \
                         compile time, without a garbage collector."))
                }
            }
        }
    }
}

fn install_section() -> Markup {
    let steps = [
        (
            "Get Ferrum Toolbox",
            html! { p { "Download the toolbox for your Mac and drag it to Applications." } },
        ),
        (
            "Sign in to GitHub",
            html! { p { "Builds are private for now, so the toolbox uses your GitHub access." } },
        ),
        (
            "Install the apps",
            html! { p { "Pick any app and click Install. It lands in Applications, ready to launch." } },
        ),
        (
            "Update or roll back",
            html! { p { "New versions show up automatically. Switch to any version with one click." } },
        ),
    ];
    html! {
        section class="section install-flow" id="install" aria-labelledby="install-title" {
            div class="container" {
                div class="section-header" {
                    p class="eyebrow" { "Install" }
                    h2 id="install-title" { "From zero to every tool in four steps" }
                }
                (card::steps(&steps))
                div class="install-flow__cta" {
                    (button::link("Download & install guide", site::DOWNLOAD_PATH, Variant::Primary))
                }
            }
        }
    }
}

fn status_strip() -> Markup {
    html! {
        section class="section section--tight" aria-labelledby="status-title" {
            div class="container" {
                div class="status-strip" {
                    div class="status-strip__head" {
                        p class="eyebrow" { "Status" }
                        h2 class="status-strip__title" id="status-title" { "Where things stand" }
                    }
                    ul class="status-list" {
                        li {
                            span class="status-dot status-dot--ok" {}
                            strong { "FerrumData" }
                            " is in alpha with a working PostgreSQL foundation."
                        }
                        li {
                            span class="status-dot" {}
                            "The other apps are early previews: their core logic is tested, and "
                            "their GPUI interfaces have been launch-tested."
                        }
                        li {
                            span class="status-dot" {}
                            "Builds are private GitHub Releases for the team. "
                            a href=(site::DOWNLOAD_PATH) { "How to get access" } "."
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_all_apps_with_links_and_logos() {
        let html = render("https://example.test").into_string();
        for p in products::ALL {
            assert!(html.contains(p.name), "missing {}", p.name);
            assert!(html.contains(&format!(r#"href="{}""#, p.path())));
            assert!(html.contains(&format!("/assets/{}?v=", p.logo_asset())));
        }
        assert!(html.contains(r#"id="apps""#));
        assert_eq!(html.matches("<h1").count(), 1);
    }

    #[test]
    fn has_gpui_and_install_sections_pointing_to_download() {
        let html = render("https://example.test").into_string();
        assert!(html.contains("Built on GPUI"));
        assert!(html.contains(r#"class="steps""#));
        assert!(html.contains(r#"href="/download""#));
        assert!(html.contains(r#"alt="Ferrum""#));
    }
}
