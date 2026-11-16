//! Shared UI components: the maud half of the design system (the CSS half
//! is `assets/css/components.css`; see `DESIGN.md`).
//!
//! Page structure
//! - `page_header::PageHeader`: breadcrumb + the page's one `<h1>` + lead/actions/media
//! - `section::Section`: a labelled `<section>` with `<h2>` and optional lead
//! - `breadcrumb::{Crumb, render}`: trail after Home (also feeds JSON-LD)
//! - `cta::band(..)`, `cta::get_launcher()`: closing call to action
//! - `nav::render(path)`, `footer::render()`, `follow::block(..)`
//!
//! Content
//! - `feature_grid::render(&[(title, text)])`
//! - `card::product_card(&Product)` (the app card), `card::app_logo(..)`,
//!   `card::app_strip(apps, with_kind)`, `card::steps(&[(title, body)])`
//! - `callout::render(Kind, title, body)`: note / warn / security
//! - `ai_logo::logo(name)`, `ai_logo::strips(groups)`: agent and model-maker logos
//! - `code::block(text)`, `code::command(cmd)`, `code::keys("⌥⌘O")`
//! - `button::link(label, href, Variant)` / `button::disabled(..)`, `badge::status(..)`

pub mod ai_logo;
pub mod badge;
pub mod breadcrumb;
pub mod button;
pub mod callout;
pub mod card;
pub mod code;
pub mod cta;
pub mod feature_grid;
pub mod follow;
pub mod footer;
pub mod icons;
pub mod nav;
pub mod page_header;
pub mod section;

use maud::{Markup, html};

use crate::routes::assets;

/// The core Ferrum mark as an `<img>`; decorative unless `alt` is non-empty.
pub fn ferrum_mark(class: &str, alt: &str) -> Markup {
    html! {
        img class=(class) src=(assets::url("img/brand/ferrum.svg")) alt=(alt)
            width="40" height="40" decoding="async";
    }
}
