//! Buttons: link-styled CTAs and disabled "Coming soon" CTAs.
//!
//! Classes: `.btn`, `.btn--primary`, `.btn--secondary`, `.btn--ghost`, `.btn--disabled`.

use maud::{Markup, html};

/// Visual weight of a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Primary,
    Secondary,
    Ghost,
}

impl Variant {
    pub fn css_class(self) -> &'static str {
        match self {
            Variant::Primary => "btn btn--primary",
            Variant::Secondary => "btn btn--secondary",
            Variant::Ghost => "btn btn--ghost",
        }
    }
}

/// An `<a>` styled as a button.
pub fn link(label: &str, href: &str, variant: Variant) -> Markup {
    html! {
        a class=(variant.css_class()) href=(href) { (label) }
    }
}

/// A primary button that opens a short list of links, e.g. one app's
/// installers. A `<details>` element, so it opens without JS; site.js closes
/// it on Escape or a click elsewhere. `items` are `(label, note, href)`.
/// The links are full page loads (downloads), not htmx page changes.
pub fn menu(label: &str, items: &[(&str, &str, String)]) -> Markup {
    html! {
        details class="btn-menu" data-menu {
            summary class="btn btn--primary" {
                (label)
                svg class="btn-menu__caret" viewBox="0 0 12 12" width="12" height="12" aria-hidden="true" {
                    path d="M2.5 4.5 6 8l3.5-3.5" fill="none" stroke="currentColor" stroke-width="1.8"
                        stroke-linecap="round" stroke-linejoin="round" {}
                }
            }
            ul class="btn-menu__list" hx-boost="false" {
                @for (item, note, href) in items {
                    li {
                        a href=(href) {
                            span class="btn-menu__label" { (item) }
                            span class="btn-menu__note" { (note) }
                        }
                    }
                }
            }
        }
    }
}

/// A non-clickable CTA, e.g. `disabled(site::DOWNLOAD_LABEL, "Downloads are not available yet")`.
/// Rendered as a `<span>` with `aria-disabled` and the note as visible small text.
pub fn disabled(label: &str, note: &str) -> Markup {
    html! {
        span class="btn btn--disabled" aria-disabled="true" role="link" title=(note) {
            (label)
            @if !note.is_empty() {
                span class="btn__note" { (note) }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_has_variant_class() {
        let html = link("Go", "/x", Variant::Secondary).into_string();
        assert!(html.contains(r#"class="btn btn--secondary""#));
        assert!(html.contains(r#"href="/x""#));
    }

    #[test]
    fn disabled_is_not_a_link() {
        let html = disabled("Coming soon", "No downloads yet").into_string();
        assert!(html.contains(r#"aria-disabled="true""#));
        assert!(!html.contains("href"));
        assert!(html.contains("No downloads yet"));
    }
}
