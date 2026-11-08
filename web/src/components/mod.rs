//! Shared UI components. Owned by the home/components area.
//!
//! Public API (stable, used by product/about/404 pages):
//! - `button::link(label, href, Variant)` / `button::disabled(label, note)`
//! - `badge::status(Status)`
//! - `card::product_card(&Product)`, `card::element_tile(symbol, name)`
//! - `placeholder_link(url, label)`: link that is visibly marked when the URL is a placeholder
//! - `nav::render(path)`, `footer::render()`

pub mod badge;
pub mod button;
pub mod card;
pub mod footer;
pub mod icons;
pub mod nav;

use maud::{Markup, html};

use crate::site;

/// External link that adds a visible "(placeholder)" marker when
/// `site::is_placeholder(url)` is true. Classes: `.placeholder-link`, `.placeholder-tag`.
pub fn placeholder_link(url: &str, label: &str) -> Markup {
    let placeholder = site::is_placeholder(url);
    html! {
        a href=(url) class=[placeholder.then_some("placeholder-link")] rel="noopener" {
            (label)
            @if placeholder {
                " " span class="placeholder-tag" { "(placeholder)" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_links_are_marked() {
        let html = placeholder_link(site::GITHUB_ORG_URL, "GitHub").into_string();
        assert!(html.contains("(placeholder)"));
        let real = placeholder_link(site::ZED_UPSTREAM_URL, "Zed").into_string();
        assert!(!real.contains("(placeholder)"));
    }
}
