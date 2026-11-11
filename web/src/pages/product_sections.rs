//! Shared template for the product pages.
//!
//! Each product page fills a [`ProductPage`] with copy and calls [`render`].
//! Every section is static, honest copy that follows each app's README.

use maud::{Markup, html};

use super::feature_demos;
use crate::components::{
    self,
    button::{self, Variant},
    card,
};
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
    /// What the preview shows: the demo's text alternative, or placeholder captions.
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
/// (e.g. an upstream credit).
pub fn render(site_url: &str, p: &ProductPage, extra: Markup) -> Markup {
    let prod = &p.product;
    let path = prod.path();
    let meta = PageMeta::new(Some(prod.name), p.meta_description, &path, "page-product");
    page(
        site_url,
        &meta,
        html! {
            (hero(p))
            (screenshots(p))
            (extra)
            (features(p))
            (status(p))
            (specs(p))
            (related(prod))
        },
    )
}

fn hero(p: &ProductPage) -> Markup {
    let prod = &p.product;
    html! {
        section class="product-hero" style=(prod.accent_style()) aria-labelledby="product-title" {
            div class="container product-hero__inner" {
                (card::app_logo(prod, "app-logo--hero", false))
                div class="product-hero__copy" {
                    p class="eyebrow" { "Ferrum · " (prod.kind) }
                    div class="product-hero__title-row" {
                        h1 id="product-title" { (prod.name) }
                        (components::badge::status(prod.status))
                    }
                    p class="product-hero__tagline" { (prod.tagline) }
                    p class="lead" { (p.intro) }
                    div class="product-hero__ctas" {
                        (button::link(site::DOWNLOAD_LABEL, site::DOWNLOAD_PATH, Variant::Primary))
                        @if prod.slug != products::FERRUM_LAUNCHER.slug {
                            (button::link("Install with Ferrum Launcher",
                                &products::FERRUM_LAUNCHER.path(), Variant::Secondary))
                        }
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

/// The app's animated demo window, or screenshot placeholders for an app
/// without one.
fn screenshots(p: &ProductPage) -> Markup {
    let demo = feature_demos::for_slug(p.product.slug);
    html! {
        section class="section section--tight product-preview" aria-labelledby="screens-title" {
            div class="container" {
                h2 id="screens-title" class="visually-hidden" { "Preview" }
                @if let Some(demo) = demo {
                    div class="product-preview__demo" role="img"
                        aria-label={ "Animated preview of " (p.product.name) ": " (p.screenshots.join(", ")) } {
                        (demo)
                    }
                } @else {
                    div class="grid grid--2" {
                        @for caption in p.screenshots {
                            figure class="product-shot" {
                                div class="screenshot-placeholder" role="img"
                                    aria-label={ "Screenshot placeholder: " (caption) } {
                                    span aria-hidden="true" { "Screenshot placeholder" }
                                }
                                figcaption class="muted" { (caption) }
                            }
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
                div class="grid grid--3" {
                    @for other in products::ALL.iter().filter(|o| o.slug != current.slug) {
                        (card::product_card(other))
                    }
                }
            }
        }
    }
}
