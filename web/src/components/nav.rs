//! Site header + primary navigation. `current_path` marks the active link.
//!
//! Classes: `.site-header`, `.brand`, `.brand__mark`, `.site-nav`, `.nav-list`,
//! `.theme-toggle`, `.nav-toggle`. Behaviour lives in `assets/js/site.js`.

use maud::{Markup, html};

use crate::components::icons;
use crate::{products, site};

fn nav_link(href: &str, label: &str, current_path: &str) -> Markup {
    let active = current_path == href;
    html! {
        li { a class="nav-link" href=(href) aria-current=[active.then_some("page")] { (label) } }
    }
}

pub fn render(current_path: &str) -> Markup {
    html! {
        header class="site-header" {
            div class="container site-header__inner" {
                a class="brand" href="/" aria-current=[(current_path == "/").then_some("page")] {
                    span class="brand__mark" aria-hidden="true" { "Fe" }
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
                        @for p in products::ALL {
                            (nav_link(&p.path(), p.name, current_path))
                        }
                        (nav_link("/about", "About", current_path))
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
    fn marks_active_link() {
        let html = render("/ferrumapi").into_string();
        assert!(html.contains(r#"href="/ferrumapi" aria-current="page""#));
        assert_eq!(html.matches("aria-current").count(), 1);
    }

    #[test]
    fn has_toggles() {
        let html = render("/").into_string();
        assert!(html.contains(r#"aria-controls="site-nav""#));
        assert!(html.contains("data-theme-toggle"));
        assert!(html.contains(r#"href="/about""#));
    }
}
