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
