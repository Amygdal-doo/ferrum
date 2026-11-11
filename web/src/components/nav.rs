//! Site header + primary navigation. `current_path` marks the active link.
//!
//! Classes: `.site-header`, `.brand`, `.brand__logo`, `.site-nav`, `.nav-list`,
//! `.nav-link--cta`, `.theme-toggle`, `.nav-toggle`. Behaviour lives in `assets/js/site.js`.

use maud::{Markup, html};

use crate::components::{ferrum_mark, icons};
use crate::{products, site, updates};

const APPS_HREF: &str = "/#apps";
const FEATURES_HREF: &str = crate::pages::features::PATH;

fn nav_link(href: &str, label: &str, active: bool, extra_class: &str) -> Markup {
    html! {
        li {
            a class={ "nav-link " (extra_class) } href=(href) aria-current=[active.then_some("page")] {
                (label)
            }
        }
    }
}

/// True for product pages managed by the launcher (they live under "Apps").
fn is_app_page(path: &str) -> bool {
    products::apps().any(|p| p.path() == path)
}

pub fn render(current_path: &str) -> Markup {
    html! {
        header class="site-header" {
            div class="container site-header__inner" {
                a class="brand" href="/" aria-current=[(current_path == "/").then_some("page")] {
                    (ferrum_mark("brand__logo", ""))
                    span class="brand__name" { (site::SITE_NAME) }
                }
                div class="site-header__actions" {
                    button class="icon-btn theme-toggle" type="button" data-theme-toggle
                        aria-label="Switch to light theme" {
                        (icons::sun())
                        (icons::moon())
                    }
                    button class="icon-btn nav-toggle" type="button" data-nav-toggle
                        aria-expanded="false" aria-controls="site-nav" aria-label="Menu" {
                        (icons::menu())
                    }
                }
                nav class="site-nav" id="site-nav" aria-label="Primary" {
                    ul class="nav-list" {
                        (nav_link(APPS_HREF, "Apps", is_app_page(current_path), ""))
                        (nav_link(FEATURES_HREF, "Features", current_path == FEATURES_HREF, ""))
                        (nav_link(updates::INDEX_PATH, "Updates",
                            current_path == updates::INDEX_PATH
                                || current_path.starts_with("/updates/"), ""))
                        (nav_link("/about", "About", current_path == "/about", ""))
                        // Ferrum Launcher is how every app is installed: the menu
                        // offers it as the download.
                        (nav_link(site::DOWNLOAD_PATH, site::DOWNLOAD_LABEL,
                            current_path == site::DOWNLOAD_PATH
                                || current_path == products::FERRUM_LAUNCHER.path(),
                            "nav-link--cta"))
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
        assert!(html.contains(r##"href="/#apps" aria-current="page""##));
        assert_eq!(html.matches("aria-current").count(), 1);
    }

    #[test]
    fn the_launcher_is_the_download() {
        let html = render("/download").into_string();
        assert!(html.contains(r#"href="/download" aria-current="page""#));
        assert!(html.contains(">Download Launcher<"));
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
    fn updates_link_is_active_on_posts() {
        let html = render("/updates/ferrum-public-alpha").into_string();
        assert!(html.contains(r#"href="/updates" aria-current="page""#));
        assert_eq!(html.matches("aria-current").count(), 1);
    }
}
