//! Site footer. Classes: `.site-footer`, `.footer-grid`, `.footer-col`, `.footer-bottom`.

use maud::{Markup, html};

use crate::components::placeholder_link;
use crate::{products, site};

pub fn render() -> Markup {
    html! {
        footer class="site-footer" {
            div class="container" {
                div class="footer-grid" {
                    div class="footer-col footer-col--brand" {
                        a class="brand" href="/" {
                            span class="brand__mark" aria-hidden="true" { "Fe" }
                            span class="brand__name" { (site::SITE_NAME) }
                        }
                        p class="muted" { (site::SITE_TAGLINE) }
                    }
                    nav class="footer-col" aria-labelledby="footer-products" {
                        h2 class="footer-col__title" id="footer-products" { "Products" }
                        ul {
                            @for p in products::ALL {
                                li { a href=(p.path()) { (p.name) } }
                            }
                        }
                    }
                    nav class="footer-col" aria-labelledby="footer-project" {
                        h2 class="footer-col__title" id="footer-project" { "Project" }
                        ul {
                            li { a href="/about" { "About" } }
                            li { (placeholder_link(site::GITHUB_ORG_URL, "GitHub")) }
                        }
                    }
                }
                div class="footer-bottom" {
                    p { "Built in Rust with Axum and maud." }
                    p {
                        "Ferrum IDE is based on the open-source "
                        a href=(site::ZED_UPSTREAM_URL) rel="noopener" { "Zed editor" }
                        "."
                    }
                    p class="footer-copy" { "© 2026 Ferrum" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credits_zed_and_marks_placeholder() {
        let html = render().into_string();
        assert!(html.contains(site::ZED_UPSTREAM_URL));
        assert!(html.contains("(placeholder)"));
        assert!(html.contains("© 2026 Ferrum"));
    }
}
