//! Product card and the periodic-table "element tile" motif.
//!
//! Classes: `.card`, `.card--product`, `.element-tile` (+ `--lg`, `--brand`),
//! `.feature-list`, `.feature`.

use maud::{Markup, html};

use crate::components::{badge, icons};
use crate::products::Product;

/// Periodic-table style tile, e.g. `element_tile("Fd", 1, "Database IDE")`.
/// `number` is shown top-left like an atomic number; decorative only.
pub fn element_tile(symbol: &str, number: u8, caption: &str, extra_class: &str) -> Markup {
    html! {
        span class={ "element-tile " (extra_class) } aria-hidden="true" {
            span class="element-tile__num" { (number) }
            span class="element-tile__sym" { (symbol) }
            span class="element-tile__cap" { (caption) }
        }
    }
}

/// Card for one product: symbol tile, name, kind, status badge, tagline, "Learn more".
pub fn product_card(p: &Product) -> Markup {
    let number = crate::products::ALL
        .iter()
        .position(|x| x.slug == p.slug)
        .map_or(26, |i| 27 + i as u8);
    let heading_id = format!("card-{}", p.slug);
    html! {
        article class="card card--product" aria-labelledby=(heading_id) {
            div class="card__top" {
                (element_tile(p.symbol, number, p.kind, ""))
                (badge::status(p.status))
            }
            h3 class="card__title" id=(heading_id) { (p.name) }
            p class="card__kind" { (p.kind) }
            p class="card__body" { (p.tagline) }
            a class="card__link" href=(p.path()) {
                "Learn more" span class="visually-hidden" { " about " (p.name) }
                (icons::arrow())
            }
        }
    }
}

/// One feature tile: icon + title + text. Wrap several in `ul.feature-list`.
pub fn feature(icon: Markup, title: &str, text: &str) -> Markup {
    html! {
        li class="feature" {
            span class="feature__icon" { (icon) }
            h3 class="feature__title" { (title) }
            p class="feature__text" { (text) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::products;

    #[test]
    fn card_links_to_detail_page() {
        let html = product_card(&products::FERRUMDATA).into_string();
        assert!(html.contains(r#"href="/ferrumdata""#));
        assert!(html.contains("FerrumData"));
        assert!(html.contains("badge--alpha"));
        assert!(html.contains(">Fd<"));
    }
}
