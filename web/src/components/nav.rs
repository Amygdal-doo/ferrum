//! Site header + primary navigation. `current_path` marks the active link.
//!
//! Classes: `.site-header`, `.brand`, `.brand__logo`, `.site-nav`, `.nav-list`,
//! `.github-link`, `.nav-download`, `.theme-toggle`, `.nav-toggle`. Behaviour lives in `assets/js/site.js`.

use maud::{Markup, html};

use crate::components::{ferrum_mark, icons};
use crate::{products, site, updates};

const APPS_HREF: &str = crate::pages::apps::PATH;
const FEATURES_HREF: &str = crate::pages::features::PATH;
const DOCS_HREF: &str = crate::pages::docs::PATH;
/// Served by the accounts service through `routes::accounts_proxy`.
const SIGN_IN_HREF: &str = "/login";

fn nav_link(href: &str, label: &str, active: bool) -> Markup {
    html! {
        li {
            a class="nav-link" href=(href) aria-current=[active.then_some("page")] {
                (label)
            }
        }
    }
}

/// True for `/apps` and the product pages managed by the launcher.
fn is_app_page(path: &str) -> bool {
    path == APPS_HREF || products::apps().any(|p| p.path() == path)
}

pub fn render(current_path: &str) -> Markup {
    // FerrumLauncher is how every app is installed: the header offers it as
    // the download, so its page marks Download as current.
    let on_download =
        current_path == site::DOWNLOAD_PATH || current_path == products::FERRUM_LAUNCHER.path();
    html! {
        header class="site-header" {
            div class="container site-header__inner" {
                a class="brand" href="/" aria-current=[(current_path == "/").then_some("page")] {
                    (ferrum_mark("brand__logo", ""))
                    span class="brand__name" { (site::SITE_NAME) }
                }
                div class="site-header__actions" {
                    a class="icon-btn github-link" href=(site::SOURCE_REPO_URL) rel="noopener" {
                        (icons::github())
                        span class="github-link__label" { "GitHub" }
                        span class="visually-hidden" { ": Ferrum's source code" }
                    }
                    button class="icon-btn theme-toggle" type="button" data-theme-toggle
                        aria-label="Switch to light theme" {
                        (icons::sun())
                        (icons::moon())
                    }
                    a class="nav-signin" href=(SIGN_IN_HREF) { "Sign in" }
                    a class="nav-download" href=(site::DOWNLOAD_PATH)
                        aria-current=[on_download.then_some("page")] { "Download" }
                    button class="icon-btn nav-toggle" type="button" data-nav-toggle
                        aria-expanded="false" aria-controls="site-nav" aria-label="Menu" {
                        (icons::menu())
                    }
                }
                nav class="site-nav" id="site-nav" aria-label="Primary" {
                    ul class="nav-list" {
                        (nav_link(APPS_HREF, "Apps", is_app_page(current_path)))
                        (nav_link(FEATURES_HREF, "Features", current_path == FEATURES_HREF))
                        (nav_link(DOCS_HREF, "Docs",
                            current_path == DOCS_HREF || current_path.starts_with("/docs/")))
                        (nav_link(updates::INDEX_PATH, "Updates",
                            current_path == updates::INDEX_PATH
                                || current_path.starts_with("/updates/")))
                        (nav_link("/about", "About", current_path == "/about"))
                        // Narrow screens drop the header's GitHub mark; the menu has it.
                        li class="nav-list__github" {
                            a class="nav-link" href=(site::SOURCE_REPO_URL) rel="noopener" {
                                "Source on GitHub"
                            }
                        }
                        // Narrow screens move Sign in into the menu too.
                        li class="nav-list__signin" {
                            a class="nav-link" href=(SIGN_IN_HREF) { "Sign in" }
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
    fn app_pages_mark_apps_active() {
        let html = render("/ferrumapi").into_string();
        assert!(html.contains(r#"href="/apps" aria-current="page""#));
        assert_eq!(html.matches("aria-current").count(), 1);
        let index = render("/apps").into_string();
        assert!(index.contains(r#"href="/apps" aria-current="page""#));
        assert_eq!(index.matches("aria-current").count(), 1);
    }

    #[test]
    fn the_launcher_is_the_download() {
        let html = render("/download").into_string();
        assert!(html.contains(r#"href="/download" aria-current="page""#));
        assert!(html.contains(">Download<"));
        assert!(!html.contains(r#"href="/ferrum-launcher""#));
        assert_eq!(html.matches("aria-current").count(), 1);
        let launcher_page = render("/ferrum-launcher").into_string();
        assert!(launcher_page.contains(r#"href="/download" aria-current="page""#));
    }

    #[test]
    fn has_toggles() {
        let html = render("/").into_string();
        assert!(html.contains(r#"aria-controls="site-nav""#));
        assert!(html.contains("data-theme-toggle"));
        assert!(html.contains(r#"href="/about""#));
    }

    #[test]
    fn links_the_source_on_github() {
        let html = render("/").into_string();
        assert!(html.contains(r#"href="https://github.com/Amygdal-doo/ferrum""#));
        assert!(html.contains("icon--github"));
    }

    #[test]
    fn has_a_sign_in_link_in_the_header_and_the_menu() {
        let html = render("/").into_string();
        assert!(html.contains(r#"<a class="nav-signin" href="/login">Sign in</a>"#));
        assert!(html.contains(r#"<li class="nav-list__signin">"#));
    }

    #[test]
    fn docs_link_is_active_on_guides() {
        let html = render("/docs/shortcuts").into_string();
        assert!(html.contains(r#"href="/docs" aria-current="page""#));
        assert_eq!(html.matches("aria-current").count(), 1);
    }

    #[test]
    fn updates_link_is_active_on_posts() {
        let html = render("/updates/ferrum-public-alpha").into_string();
        assert!(html.contains(r#"href="/updates" aria-current="page""#));
        assert_eq!(html.matches("aria-current").count(), 1);
    }
}
