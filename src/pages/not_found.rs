//! 404 page: "This page has rusted away". Marked noindex via `PageMeta::noindex`.

use maud::{Markup, html};

use crate::components::button::{self, Variant};
use crate::layout::{PageMeta, page};
use crate::products;

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(
        Some("Page not found"),
        "This page has rusted away. Head back to the Ferrum home page or explore \
         FerrumData, FerrumAPI and Ferrum IDE.",
        "/404",
        "page-404",
    )
    .noindex();
    page(
        site_url,
        &meta,
        html! {
            section class="section notfound" aria-labelledby="notfound-title" {
                div class="container notfound__inner" {
                    div class="notfound__tile" aria-hidden="true" {
                        span class="notfound__num mono" { "404" }
                        span class="notfound__sym" { "Fe" }
                        span class="notfound__formula mono" { "Fe" sub { "2" } "O" sub { "3" } }
                    }
                    div class="notfound__copy" {
                        p class="eyebrow" { "Error 404 · oxidation detected" }
                        h1 id="notfound-title" { "This page has rusted away" }
                        p class="lead" {
                            "Whatever used to be here has oxidised into a fine orange dust. \
                             The link may be old, or the page may never have been forged."
                        }
                        div class="notfound__ctas" {
                            (button::link("Back to home", "/", Variant::Primary))
                        }
                    }
                }
            }
            section class="section section--tight" aria-labelledby="notfound-products" {
                div class="container" {
                    h2 id="notfound-products" class="notfound__subhead" {
                        "Still solid metal"
                    }
                    ul class="notfound__links" {
                        @for p in products::ALL {
                            li {
                                a class="notfound__link" href=(p.path()) {
                                    span class="notfound__link-sym mono" aria-hidden="true" { (p.symbol) }
                                    span { (p.name) span class="muted" { " · " (p.kind) } }
                                }
                            }
                        }
                    }
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_h1_home_link_and_products() {
        let html = render("https://example.test").into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains("This page has rusted away"));
        assert!(html.contains("href=\"/\""));
        for p in products::ALL {
            assert!(html.contains(&format!("href=\"{}\"", p.path())));
        }
    }

    #[test]
    fn is_not_indexed() {
        let html = render("https://example.test").into_string();
        assert!(html.contains(r#"<meta name="robots" content="noindex">"#));
    }
}
