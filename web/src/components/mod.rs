//! Shared UI components. Owned by the home/components area.
//!
//! Public API (stable, used by product/about/download/404 pages):
//! - `button::link(label, href, Variant)` / `button::disabled(label, note)`
//! - `badge::status(Status)`
//! - `card::product_card(&Product)`, `card::app_logo(&Product, class)`,
//!   `card::element_tile(symbol, number, caption, class)`, `card::feature(..)`
//! - `card::steps(&[(title, body)])`: numbered how-to list
//! - `nav::render(path)`, `footer::render()`

pub mod badge;
pub mod button;
pub mod card;
pub mod footer;
pub mod icons;
pub mod nav;

use maud::{Markup, html};

use crate::routes::assets;

/// The core Ferrum mark as an `<img>`; decorative unless `alt` is non-empty.
pub fn ferrum_mark(class: &str, alt: &str) -> Markup {
    html! {
        img class=(class) src=(assets::url("img/brand/ferrum.svg")) alt=(alt)
            width="40" height="40" decoding="async";
    }
}
