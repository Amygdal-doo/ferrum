//! Shared template for the three product pages.
//!
//! Each product page fills a [`ProductPage`] with copy and calls [`render`].
//! Every section is static, honest copy; nothing here claims shipped downloads.

use maud::{Markup, html};

use crate::components::{self, card};
use crate::layout::{PageMeta, page};
use crate::products::{self, Product};
use crate::site;

/// A titled list of short bullet points (e.g. "Not built yet").
pub struct BulletGroup {
    pub heading: &'static str,
    pub items: &'static [&'static str],
}

/// Copy for one product page.
pub struct ProductPage {
    pub product: Product,
    /// Unique `<meta name="description">` text (~150 chars).
    pub meta_description: &'static str,
    /// Paragraph under the tagline in the hero.
    pub intro: &'static str,
    /// Lead paragraph above the features grid.
    pub features_lead: &'static str,
    /// (title, body) pairs for the features grid.
    pub features: &'static [(&'static str, &'static str)],
    /// Captions for screenshot placeholders (no images ship yet).
    pub screenshots: &'static [&'static str],
    /// One-paragraph honest status summary.
    pub status_summary: &'static str,
    /// Lists shown in the status section, e.g. "Works today" / "Not built yet".
    pub status_groups: &'static [BulletGroup],
    /// (label, value) rows for the "Requirements & built with" spec sheet.
    pub specs: &'static [(&'static str, &'static str)],
    /// Short note under the spec sheet.
    pub specs_note: &'static str,
}

/// Render a full product page. `extra` is inserted after the hero
/// (used by Ferrum IDE for the upstream credit).
pub fn render(site_url: &str, p: &ProductPage, extra: Markup) -> Markup {
    let prod = &p.product;
    let path = prod.path();
    let meta = PageMeta::new(Some(prod.name), p.meta_description, &path, "page-product");
    page(
        site_url,
        &meta,
        html! {
            (hero(p))
            (extra)
            (features(p))
            (screenshots(p))
            (status(p))
            (specs(p))
            (related(prod))
        },
    )
}

/// Large decorative element tile; numbering matches the home-page cards.
fn hero_tile(prod: &Product) -> Markup {
    let number = products::ALL
        .iter()
        .position(|x| x.slug == prod.slug)
        .map_or(26, |i| 27 + i as u8);
    card::element_tile(
        prod.symbol,
        number,
        prod.kind,
        "element-tile--lg element-tile--brand",
    )
}

fn hero(p: &ProductPage) -> Markup {
    let prod = &p.product;
    html! {
        section class="product-hero" aria-labelledby="product-title" {
            div class="container product-hero__inner" {
                (hero_tile(prod))
                div class="product-hero__copy" {
                    p class="eyebrow" { "Ferrum · " (prod.kind) }
                    div class="product-hero__title-row" {
                        h1 id="product-title" { (prod.name) }
                        (components::badge::status(prod.status))
                    }
                    p class="product-hero__tagline" { (prod.tagline) }
                    p class="lead" { (p.intro) }
                    div class="product-hero__ctas" {
                        (components::button::disabled(site::DOWNLOAD_LABEL, "No installers or downloads are published yet."))
                        (components::placeholder_link(prod.github_url, "Source on GitHub"))
                    }
                }
            }
        }
    }
}

fn features(p: &ProductPage) -> Markup {
    html! {
        section class="section" aria-labelledby="features-title" {
            div class="container" {
                p class="eyebrow" { "Features" }
                h2 id="features-title" { "What " (p.product.name) " does" }
                p class="lead" { (p.features_lead) }
                ul class="feature-list" {
                    @for (title, body) in p.features {
                        li class="feature" {
                            h3 class="feature__title" { (title) }
                            p class="feature__text" { (body) }
                        }
                    }
                }
            }
        }
    }
}

fn screenshots(p: &ProductPage) -> Markup {
    html! {
        section class="section section--tight" aria-labelledby="screens-title" {
            div class="container" {
                h2 id="screens-title" class="visually-hidden" { "Screenshots" }
                div class="grid grid--2" {
                    @for caption in p.screenshots {
                        figure class="product-shot" {
                            div class="screenshot-placeholder" role="img"
                                aria-label={ "Screenshot placeholder: " (caption) } {
                                span class="mono" aria-hidden="true" { "Screenshot placeholder" }
                            }
                            figcaption class="muted" { (caption) }
                        }
                    }
                }
            }
        }
    }
}

fn status(p: &ProductPage) -> Markup {
    html! {
        section class="section product-status" aria-labelledby="status-title" {
            div class="container" {
                p class="eyebrow" { "Status" }
                h2 id="status-title" {
                    "Status: " (p.product.status.label())
                }
                p class="lead" { (p.status_summary) }
                div class="grid grid--2 product-status__groups" {
                    @for group in p.status_groups {
                        div class="card" {
                            h3 { (group.heading) }
                            ul class="product-list" {
                                @for item in group.items { li { (item) } }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn specs(p: &ProductPage) -> Markup {
    html! {
        section class="section" aria-labelledby="specs-title" {
            div class="container" {
                p class="eyebrow" { "Requirements" }
                h2 id="specs-title" { "Requirements & built with" }
                dl class="spec-sheet" {
                    @for (label, value) in p.specs {
                        div class="spec-sheet__row" {
                            dt class="mono" { (label) }
                            dd { (value) }
                        }
                    }
                }
                p class="muted spec-sheet__note" { (p.specs_note) }
            }
        }
    }
}

fn related(current: &Product) -> Markup {
    html! {
        section class="section product-related" aria-labelledby="related-title" {
            div class="container" {
                p class="eyebrow" { "The Ferrum family" }
                h2 id="related-title" { "Other Ferrum tools" }
                div class="grid grid--2" {
                    @for other in products::ALL.iter().filter(|o| o.slug != current.slug) {
                        (card::product_card(other))
                    }
                }
            }
        }
    }
}
