//! Site footer. Classes: `.site-footer`, `.footer-credit`, `.footer-grid`, `.footer-col`,
//! `.footer-bottom`.

use maud::{Markup, html};

use crate::components::{
    button::{self, Variant},
    ferrum_mark,
};
use crate::{products, site};

pub fn render() -> Markup {
    html! {
        footer class="site-footer" {
            div class="container" {
                div class="footer-credit" {
                    p class="footer-credit__line" {
                        "Built by "
                        a href=(site::AMYGDAL_URL) rel="noopener" { "amygdal.com" }
                        " for the Rust community."
                    }
                    (button::link(site::DOWNLOAD_LABEL, site::DOWNLOAD_PATH, Variant::Primary))
                }
                div class="footer-grid" {
                    div class="footer-col footer-col--brand" {
                        a class="brand" href="/" {
                            (ferrum_mark("brand__logo", ""))
                            span class="brand__name" { (site::SITE_NAME) }
                        }
                        p class="muted" { (site::SITE_TAGLINE) }
                    }
                    nav class="footer-col" aria-labelledby="footer-products" {
                        h2 class="footer-col__title" id="footer-products" { "Apps" }
                        ul {
                            @for p in products::ALL {
                                li { a href=(p.path()) { (p.name) } }
                            }
                        }
                    }
                    nav class="footer-col" aria-labelledby="footer-project" {
                        h2 class="footer-col__title" id="footer-project" { "Ferrum" }
                        ul {
                            li { a href=(site::DOWNLOAD_PATH) { "Download & install" } }
                            li { a href="/features" { "Features" } }
                            li { a href="/about" { "About" } }
                            li { a href=(site::GPUI_URL) rel="noopener" { "GPUI" } }
                        }
                    }
                }
                div class="footer-bottom" {
                    p { "Desktop apps built in Rust with GPUI. This site: Axum and maud." }
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
    fn credits_zed_and_links_download() {
        let html = render().into_string();
        assert!(html.contains(site::ZED_UPSTREAM_URL));
        assert!(html.contains(r#"href="/download""#));
        assert!(html.contains("© 2026 Ferrum"));
        assert!(html.contains(site::AMYGDAL_URL));
        assert!(html.contains("for the Rust community"));
        for p in products::ALL {
            assert!(html.contains(&format!(r#"href="{}""#, p.path())));
        }
    }
}
