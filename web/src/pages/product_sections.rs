//! Shared template for the product pages.
//!
//! Each product page fills a [`ProductPage`] with copy and calls [`render`].
//! Every section is static, honest copy that follows each app's README.

use maud::{Markup, html};

use super::{compare, docs, feature_demos};
use crate::components::{
    badge,
    breadcrumb::Crumb,
    button::{self, Variant},
    card, feature_grid,
    page_header::PageHeader,
    section::Section,
};
use crate::layout::{PageMeta, page};
use crate::products::{self, Product};
use crate::{seo, site};

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

/// Render a full product page. `extra` is inserted after the preview
/// (e.g. an upstream credit).
pub fn render(site_url: &str, p: &ProductPage, extra: Markup) -> Markup {
    let prod = &p.product;
    let path = prod.path();
    let title = format!("{}: {} for macOS", prod.name, prod.kind);
    let image = format!(
        "{site_url}{}",
        crate::routes::assets::url(&prod.og_image_asset())
    );
    let meta = PageMeta::new(Some(&title), p.meta_description, &path, "page-product")
        .trail(vec![Crumb::new(prod.name, &path)])
        .og_image(
            &prod.og_image_asset(),
            &format!("{} logo and name", prod.name),
        )
        .json_ld(seo::software_application(
            site_url,
            prod,
            p.meta_description,
            &image,
        ));
    page(
        site_url,
        &meta,
        html! {
            (hero(p, &meta.trail))
            (screenshots(p))
            (extra)
            (features(p))
            (status(p))
            (specs(p))
            (related(prod))
        },
    )
}

fn hero(p: &ProductPage, trail: &[Crumb]) -> Markup {
    let prod = &p.product;
    PageHeader::new(trail, prod.name)
        .accent(prod.accent_style())
        .title_suffix(badge::status(prod.status))
        .media(card::app_logo(prod, "app-logo--hero", false))
        .lead(html! {
            p class="page-header__tagline" { (prod.tagline) }
            p { (p.intro) }
        })
        .actions(html! {
            (button::link(site::DOWNLOAD_LABEL, site::DOWNLOAD_PATH, Variant::Primary))
            @if prod.slug != products::FERRUM_LAUNCHER.slug {
                (button::link("Install with Ferrum Launcher",
                    &products::FERRUM_LAUNCHER.path(), Variant::Secondary))
            }
        })
        .render()
}

fn features(p: &ProductPage) -> Markup {
    let title = format!("What {} does", p.product.name);
    Section::new("features", &title)
        .lead(html! { p { (p.features_lead) } })
        .render(feature_grid::render(p.features))
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
    let title = format!("Status: {}", p.product.status.label());
    Section::new("status", &title)
        .band()
        .lead(html! { p { (p.status_summary) } })
        .render(html! {
            div class="grid grid--2 product-status__groups" {
                @for group in p.status_groups {
                    div class="status-group" {
                        h3 { (group.heading) }
                        ul class="product-list" {
                            @for item in group.items { li { (item) } }
                        }
                    }
                }
            }
        })
}

fn specs(p: &ProductPage) -> Markup {
    Section::new("specs", "Requirements & built with").render(html! {
        dl class="spec-sheet" {
            @for (label, value) in p.specs {
                div class="spec-sheet__row" {
                    dt { (label) }
                    dd { (value) }
                }
            }
        }
        p class="muted spec-sheet__note" { (p.specs_note) }
    })
}

/// Guides and comparisons that mention this app.
fn guides(current: &Product) -> Vec<(String, String)> {
    let mut links = Vec::new();
    if docs::shortcuts::APPS
        .iter()
        .any(|(p, _)| p.slug == current.slug)
    {
        links.push((
            format!("{} keyboard shortcuts", current.name),
            format!("{}#{}", docs::shortcuts::PATH, current.slug),
        ));
    }
    if let Some((cmd, _, _)) = docs::shell_commands::COMMANDS
        .iter()
        .find(|(_, p, _)| p.slug == current.slug)
    {
        links.push((
            format!("Open it from a terminal with {cmd}"),
            docs::shell_commands::PATH.to_owned(),
        ));
    }
    for c in compare::COMPARISONS
        .iter()
        .filter(|c| c.product.slug == current.slug)
    {
        links.push((c.title.to_owned(), c.path()));
    }
    links.push((
        "Install and update with Ferrum Launcher".to_owned(),
        docs::install::PATH.to_owned(),
    ));
    links
}

fn related(current: &Product) -> Markup {
    Section::new("related", "Other Ferrum tools").render(html! {
        nav class="guide-links" aria-label={ "Guides for " (current.name) } {
            ul {
                @for (label, href) in guides(current) {
                    li { a href=(href) { (label) } }
                }
            }
        }
        div class="grid grid--3" {
            @for other in products::ALL.iter().filter(|o| o.slug != current.slug) {
                (card::product_card(other))
            }
        }
    })
}
