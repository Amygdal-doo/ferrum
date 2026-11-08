//! Home page: hero, product cards, why Rust, honest status strip.

use maud::{Markup, html};

use crate::components::{
    button::{self, Variant},
    card, icons, placeholder_link,
};
use crate::layout::{PageMeta, page};
use crate::{products, site};

const DESCRIPTION: &str = "Ferrum is a family of fast, native, local-first developer tools written \
     in Rust: FerrumData (database IDE), FerrumAPI (API client) and Ferrum IDE (code editor).";

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(None, DESCRIPTION, "/", "page-home");
    page(
        site_url,
        &meta,
        html! {
            (hero())
            (products_section())
            (why_rust())
            (status_strip())
        },
    )
}

fn hero() -> Markup {
    html! {
        section class="hero" aria-labelledby="hero-title" {
            div class="container hero__inner" {
                div class="hero__copy" {
                    p class="eyebrow" { "Fe · 26 · iron oxide" }
                    h1 class="hero__title" id="hero-title" {
                        "Developer tools, " span class="hero__accent" { "forged in Rust." }
                    }
                    p class="lead" {
                        "Ferrum is a family of fast, native, local-first tools for working with "
                        "databases, APIs and code. No Electron, no bundled browser runtime: "
                        "just compiled Rust talking straight to your machine."
                    }
                    div class="hero__ctas" {
                        (button::link("Explore the tools", "#products", Variant::Primary))
                        a class="btn btn--secondary" href=(site::GITHUB_ORG_URL) rel="noopener" {
                            "GitHub"
                            @if site::is_placeholder(site::GITHUB_ORG_URL) {
                                " " span class="placeholder-tag" { "(placeholder)" }
                            }
                        }
                    }
                }
                (hero_visual())
            }
        }
    }
}

/// Decorative periodic-table cluster: the brand "Fe" tile surrounded by products.
fn hero_visual() -> Markup {
    html! {
        div class="hero__visual" aria-hidden="true" {
            div class="tile-cluster" {
                (card::element_tile("Fe", 26, "Ferrum", "element-tile--lg element-tile--brand"))
                @for (i, p) in products::ALL.iter().enumerate() {
                    (card::element_tile(p.symbol, 27 + i as u8, p.kind, "tile-cluster__item"))
                }
            }
        }
    }
}

fn products_section() -> Markup {
    html! {
        section class="section" id="products" aria-labelledby="products-title" {
            div class="container" {
                div class="section-header" {
                    p class="eyebrow" { "The family" }
                    h2 id="products-title" { "Three tools, one alloy" }
                    p class="lead" {
                        "Each tool is a native desktop app with a shared focus: start fast, stay "
                        "responsive with large data, and keep your work on your machine."
                    }
                }
                div class="grid grid--3" {
                    @for p in products::ALL { (card::product_card(&p)) }
                }
            }
        }
    }
}

fn why_rust() -> Markup {
    html! {
        section class="section why-rust" aria-labelledby="why-title" {
            div class="container" {
                div class="section-header" {
                    p class="eyebrow" { "Why Rust" }
                    h2 id="why-title" { "Built on a stronger metal" }
                }
                ul class="feature-list" {
                    (card::feature(icons::shield(), "Memory safety without a GC",
                        "Rust's ownership model rules out whole classes of memory bugs at compile time, \
                         without a garbage collector pausing your work."))
                    (card::feature(icons::bolt(), "Native speed, modest memory",
                        "Compiled to machine code, so the apps start quickly and use memory for your data, \
                         not for a browser engine."))
                    (card::feature(icons::cube(), "One binary, no web runtime",
                        "No Electron and no bundled Chromium. Each tool ships as a native application."))
                    (card::feature(icons::streams(), "Fearless concurrency",
                        "Async Rust keeps the UI responsive while queries and requests run, so results \
                         can stream in and be cancelled."))
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
                            " has a working PostgreSQL foundation: streaming queries with cancel, "
                            "a virtualized results grid and a table explorer."
                        }
                        li {
                            span class="status-dot" {}
                            strong { "FerrumAPI" } " and " strong { "Ferrum IDE" }
                            " are in early development."
                        }
                        li {
                            span class="status-dot status-dot--off" {}
                            "Downloads: " (button::disabled(site::DOWNLOAD_LABEL, ""))
                        }
                    }
                    p class="status-strip__more muted" {
                        "Follow along on " (placeholder_link(site::GITHUB_ORG_URL, "GitHub")) "."
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
    fn lists_all_products_with_links() {
        let html = render("https://example.test").into_string();
        for p in products::ALL {
            assert!(html.contains(p.name), "missing {}", p.name);
            assert!(html.contains(&format!(r#"href="{}""#, p.path())));
        }
        assert!(html.contains(r#"id="products""#));
        assert!(html.contains("Early preview"));
        assert!(html.contains("Alpha"));
        assert!(html.contains(site::DOWNLOAD_LABEL));
    }

    #[test]
    fn marks_placeholders() {
        let html = render("https://example.test").into_string();
        assert!(html.contains("(placeholder)"));
        assert!(html.contains(r#"class="page-home""#));
    }
}
