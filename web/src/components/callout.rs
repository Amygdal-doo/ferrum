//! Callouts: a short aside that needs attention.
//!
//! Classes: `.callout` + `--note` (default), `--warn`, `--security`; `.callout__title`.

use maud::{Markup, html};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Helpful context.
    Note,
    /// Something can go wrong or needs an extra step.
    Warn,
    /// A security property or limit.
    Security,
}

impl Kind {
    fn class(self) -> &'static str {
        match self {
            Kind::Note => "callout callout--note",
            Kind::Warn => "callout callout--warn",
            Kind::Security => "callout callout--security",
        }
    }
}

/// `title` is rendered as a bold first line (not a heading, so callouts
/// never disturb the outline).
pub fn render(kind: Kind, title: Option<&str>, body: Markup) -> Markup {
    html! {
        div class=(kind.class()) role="note" {
            @if let Some(t) = title { p class="callout__title" { (t) } }
            (body)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_sets_the_modifier() {
        let html = render(Kind::Warn, Some("Heads up"), html! { p { "x" } }).into_string();
        assert!(html.contains("callout callout--warn"));
        assert!(html.contains(r#"<p class="callout__title">Heads up</p>"#));
    }
}
