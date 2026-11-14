//! Home page: the iron-filings hero, the four main apps, why native, install.

use maud::{Markup, html};

use super::{apps, feature_demos};
use crate::components::{
    button::{self, Variant},
    card, code,
    follow::{self, Size},
};
use crate::layout::{PageMeta, page};
use crate::{seo, site};

const DESCRIPTION: &str = "Native developer tools for macOS, written in Rust: a code editor, \
     API client, Git client, database IDE, terminal and more, installed by one free launcher.";

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(None, DESCRIPTION, "/", "page-home")
        .json_ld(seo::organization(site_url))
        .json_ld(seo::website(site_url));
    page(
        site_url,
        &meta,
        html! {
            (hero())
            (showcase())
            (native_section())
            (install_section())
        },
    )
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
                (code::command(site::MACOS_INSTALL_COMMAND))
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
                    h2 id="apps-title" { "Write it, commit it, run it, query it" }
                    p class="lead" {
                        "Four apps share one project bar: switch projects with ⌥⌘O, and open \
                         the current one in the editor, the Git client or the terminal."
                    }
                }
                div class="showcase__body" data-showcase {
                    div class="showcase__tabs" role="tablist" aria-label="Main Ferrum apps" {
                        @for (i, e) in apps::MAIN.iter().enumerate() {
                            @let p = &e.product;
                            button class="showcase__tab" type="button" role="tab"
                                id={ "tab-" (p.slug) } aria-controls={ "panel-" (p.slug) }
                                aria-selected=(if i == 0 { "true" } else { "false" })
                                tabindex=(if i == 0 { "0" } else { "-1" })
                                style=(p.accent_style()) {
                                (card::app_logo(p, "app-logo--sm", true))
                                span class="showcase__name" { (p.name) }
                                span class="showcase__kind" { (p.kind) }
                            }
                        }
                    }
                    @for (i, e) in apps::MAIN.iter().enumerate() {
                        @let p = &e.product;
                        div class="showcase__panel" role="tabpanel" id={ "panel-" (p.slug) }
                            aria-labelledby={ "tab-" (p.slug) } hidden[i > 0]
                            style=(p.accent_style()) {
                            div class="showcase__demo" role="img"
                                aria-label={ "Animated preview of " (p.name) } {
                                @if let Some(demo) = feature_demos::for_slug(p.slug) { (demo) }
                            }
                            div class="showcase__info" {
                                p class="showcase__summary" { (e.summary) }
                                ul class="showcase__features" {
                                    @for f in e.features.iter().take(4) { li { (f) } }
                                }
                                a class="showcase__more" href=(p.path()) {
                                    "More about " (p.name)
                                }
                            }
                        }
                    }
                }
                (more_apps())
            }
        }
    }
}

/// The other seven apps as one compact row, ending in the full list.
fn more_apps() -> Markup {
    let others = apps::MORE.iter().chain(std::iter::once(&apps::LAUNCHER));
    html! {
        nav class="more-apps" aria-labelledby="more-apps-title" {
            h3 class="more-apps__title" id="more-apps-title" { "Also in the launcher" }
            (card::app_strip(others.map(|e| &e.product), false))
            a class="more-apps__all" href=(apps::PATH) { "All eleven apps" }
        }
    }
}

/// Layers between a keystroke and the screen, top to bottom.
const FERRUM_STACK: [&str; 3] = ["The app, in Rust", "GPUI", "Metal, on the GPU"];
const BROWSER_STACK: [&str; 4] = [
    "The app, in JavaScript",
    "A UI framework",
    "Node.js and V8",
    "Chromium",
];

fn native_section() -> Markup {
    html! {
        section class="section native" aria-labelledby="native-title" {
            div class="container native__inner" {
                div class="native__copy" {
                    h2 id="native-title" { "Native, not a browser in a box" }
                    p class="lead" {
                        "No Electron, no webview, no JavaScript runtime. Each app is compiled \
                         Rust drawn by GPUI, the GPU framework behind Zed, so it opens in a \
                         blink and long lists, result grids and histories scroll without a \
                         stutter."
                    }
                    dl class="native__facts" {
                        div {
                            dt { "Your work stays local" }
                            dd {
                                "Requests, queries, repositories and terminal sessions live on \
                                 your Mac. Nothing needs an account."
                            }
                        }
                        div {
                            dt { "One family" }
                            dd {
                                "One theme, one keyboard-first feel, and apps that open each \
                                 other. Every one installs from the same launcher."
                            }
                        }
                    }
                }
                figure class="stack" {
                    div class="stack__cols" {
                        (stack_col("stack__col--ferrum", "A Ferrum app", &FERRUM_STACK))
                        (stack_col("stack__col--web", "A browser in a box", &BROWSER_STACK))
                    }
                    p class="stack__base" aria-hidden="true" { "macOS" }
                    figcaption {
                        "Fewer layers between your keystroke and the screen. Both run on macOS."
                    }
                }
            }
        }
    }
}

fn stack_col(class: &str, name: &str, layers: &[&str]) -> Markup {
    html! {
        div class={ "stack__col " (class) } {
            p class="stack__name" { (name) }
            ol class="stack__layers" {
                @for l in layers { li { (l) } }
            }
        }
    }
}

fn install_section() -> Markup {
    let steps = [
        (
            "Get Ferrum Launcher",
            "Run the command at the top of this page, or download the DMG and drag it to \
             Applications.",
        ),
        (
            "Open it",
            "No account and no sign-in. The launcher lists every Ferrum app.",
        ),
        (
            "Install the apps",
            "Click Install on any app. It lands in Applications, ready to launch.",
        ),
        (
            "Stay current",
            "New versions show up on their own, and the launcher updates itself in place.",
        ),
    ];
    html! {
        section class="section install-flow" id="install" aria-labelledby="install-title" {
            div class="container" {
                div class="install-flow__head" {
                    h2 id="install-title" { "From one download to every app" }
                    p class="lead" {
                        "Every app is in alpha: usable every day, released often, with rough \
                         edges in the interface. Windows and Linux builds are coming."
                    }
                }
                ol class="track" {
                    @for (title, body) in steps {
                        li class="track__step" {
                            h3 class="track__title" { (title) }
                            p { (body) }
                        }
                    }
                }
                div class="install-flow__cta" {
                    (button::link("Download for macOS", site::MACOS_DOWNLOAD_PATH, Variant::Primary))
                    (button::link("Read the install guide", "/docs/install", Variant::Secondary))
                    (button::link("Shell commands", "/docs/shell-commands", Variant::Ghost))
                }
                (follow::block(Size::Compact, true))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn showcases_the_four_main_apps_and_links_the_rest() {
        let html = render("https://example.test").into_string();
        for e in &apps::MAIN {
            let p = &e.product;
            assert!(
                html.contains(&format!(r#"id="panel-{}""#, p.slug)),
                "{}",
                p.slug
            );
            assert!(html.contains(&format!(r#"href="{}""#, p.path())));
            assert!(html.contains(e.features[0]));
        }
        assert_eq!(html.matches(r#"role="tab""#).count(), 4);
        assert_eq!(html.matches(r#"class="demo""#).count(), 4);
        assert_eq!(html.matches(r#"aria-selected="true""#).count(), 1);
        for e in apps::MORE.iter().chain(std::iter::once(&apps::LAUNCHER)) {
            let p = &e.product;
            assert!(
                !html.contains(&format!(r#"id="panel-{}""#, p.slug)),
                "{}",
                p.slug
            );
            assert!(
                html.contains(&format!(r#"href="{}""#, p.path())),
                "{}",
                p.slug
            );
        }
        assert!(html.contains(r#"href="/apps""#));
        assert!(html.contains(r#"id="apps""#));
        assert_eq!(html.matches("<h1").count(), 1);
        assert_eq!(crate::products::ALL.len(), 12);
    }

    #[test]
    fn hero_offers_download_and_the_install_command() {
        let html = render("https://example.test").into_string();
        assert!(html.contains("data-filings"));
        assert!(html.contains(site::MACOS_DOWNLOAD_PATH));
        assert!(html.contains(r#"data-copy="curl -fsSL"#));
        assert!(html.contains(r#"class="track""#));
        assert_eq!(html.matches(r#"class="track__step""#).count(), 4);
        assert!(html.contains(r#"href="/docs/install""#));
        assert!(html.contains(r#""@type":"Organization""#));
        assert!(html.contains(r#""@type":"WebSite""#));
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
