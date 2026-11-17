//! Google Analytics (GA4), only after the visitor accepts cookies.
//!
//! With `GA_MEASUREMENT_ID` set, every page carries the ID in a `<meta>` and
//! loads `js/consent.js`. That script shows the cookie banner and loads
//! Google's tag only once the visitor clicks Accept; Decline keeps the page
//! free of Google Analytics. The choice stays in local storage and the
//! footer's "Cookie settings" opens the banner again. Unset: nothing at all.

use std::sync::OnceLock;

use maud::{Markup, html};

use crate::routes::assets;

/// `<meta name>` carrying the measurement ID for `consent.js`.
pub const META_NAME: &str = "ga-measurement-id";

static MEASUREMENT_ID: OnceLock<String> = OnceLock::new();

/// A GA4 measurement ID: `G-` and 4–16 of `A-Z 0-9`.
pub fn parse_measurement_id(raw: &str) -> Option<String> {
    let id = raw.trim().to_ascii_uppercase();
    let rest = id.strip_prefix("G-")?;
    ((4..=16).contains(&rest.len()) && rest.bytes().all(|b| b.is_ascii_alphanumeric()))
        .then_some(id)
}

/// Turns analytics on for this process. The first ID wins.
pub fn enable(id: &str) {
    let _ = MEASUREMENT_ID.set(id.to_owned());
}

/// The configured measurement ID, if analytics is on.
pub fn measurement_id() -> Option<&'static str> {
    MEASUREMENT_ID.get().map(String::as_str)
}

/// The `<head>` tags for `id`: nothing when analytics is off.
pub fn head_tags(id: Option<&str>) -> Markup {
    html! {
        @if let Some(id) = id {
            meta name=(META_NAME) content=(id);
            script src=(assets::url("js/consent.js")) defer {}
        }
    }
}

/// The footer's button that opens the cookie banner again.
pub fn settings_button(id: Option<&str>) -> Markup {
    html! {
        @if id.is_some() {
            button class="footer-cookies" type="button" data-consent-open { "Cookie settings" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ga4_ids_are_accepted() {
        assert_eq!(
            parse_measurement_id(" g-abc123xyz ").as_deref(),
            Some("G-ABC123XYZ")
        );
        for bad in [
            "",
            "G-",
            "G-12",
            "UA-1234-1",
            "G-ABC 123",
            "G-ABC\"><script>",
        ] {
            assert_eq!(parse_measurement_id(bad), None, "{bad}");
        }
    }

    #[test]
    fn nothing_is_emitted_without_an_id() {
        assert!(head_tags(None).into_string().is_empty());
        assert!(settings_button(None).into_string().is_empty());
        let head = head_tags(Some("G-ABC123")).into_string();
        assert!(head.contains(r#"<meta name="ga-measurement-id" content="G-ABC123">"#));
        assert!(head.contains("/assets/js/consent.js?v="));
        assert!(
            settings_button(Some("G-ABC123"))
                .into_string()
                .contains("data-consent-open")
        );
    }
}
