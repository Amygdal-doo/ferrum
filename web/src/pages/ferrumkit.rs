//! FerrumKit product page. Claims follow the FerrumKit README
//! ("What works today" / "Not built yet") and nothing beyond it.

use maud::{Markup, html};

use super::product_sections::{self as sections, BulletGroup, ProductPage};
use crate::products::FERRUMKIT;

const FEATURES: &[(&str, &str)] = &[
    (
        "JSON, YAML and TOML",
        "Format, minify and validate JSON with the exact line and column of an error, and \
         convert between JSON, YAML and TOML.",
    ),
    (
        "JWT decoder",
        "Header and payload pretty-printed, with exp, iat and nbf shown as dates and an \
         expired warning. Decoding only: signatures aren't verified.",
    ),
    (
        "Encoders",
        "Base64 (standard and URL-safe), URL encode and decode, and a URL parser that lays out \
         the query parameters as a table.",
    ),
    (
        "Generators",
        "UUID v4 and v7, ULID, and MD5, SHA-1, SHA-256 and SHA-512 hashes of any text.",
    ),
    (
        "Regex tester",
        "Matches highlighted as you type and capture groups listed, using Rust regex syntax.",
    ),
    (
        "Converters",
        "Unix timestamps and dates, number bases, colours (hex, RGB, HSL), text case, line \
         tools and a text diff.",
    ),
];

const WORKS_TODAY: &[&str] = &[
    "A searchable sidebar of tools, with ⌘K to search",
    "JSON format, minify, validate and JSON ⇄ YAML ⇄ TOML",
    "JWT decode, Base64, URL encode and parse",
    "Timestamps, UUIDs and ULIDs, hashes",
    "Regex tester, text diff, number base, colour and case converters",
    "Copy buttons on every output",
];

const NOT_BUILT_YET: &[&str] = &[
    "Saved inputs between launches",
    "A global shortcut to open a tool from anywhere",
    "JWT signature verification",
    "Wrapping long lines (they scroll sideways) and arrow keys in the tool list",
];

const PAGE: ProductPage = ProductPage {
    product: FERRUMKIT,
    meta_description: "FerrumKit is a native Rust app with everyday developer utilities: JSON, \
                       JWT, Base64, regex, hashes, UUIDs and timestamps, all offline.",
    intro: "The small tools you otherwise look up on random websites, in one native window. \
            Everything runs on your Mac, so tokens and payloads never leave it.",
    features_lead: "Every tool's logic is a plain Rust function covered by unit tests; the \
                    window is new in this alpha.",
    features: FEATURES,
    screenshots: &[
        "the JSON formatter",
        "the JWT decoder with an expired token",
    ],
    status_summary: "FerrumKit is in alpha. The tools are tested; the GPUI window is new and \
                     still being polished.",
    status_groups: &[
        BulletGroup {
            heading: "Works today",
            items: WORKS_TODAY,
        },
        BulletGroup {
            heading: "Not built yet",
            items: NOT_BUILT_YET,
        },
    ],
    specs: &[
        ("Install", "Ferrum Toolbox (free, see Download)"),
        ("Network", "None: every tool runs offline"),
        (
            "Builds",
            "macOS (Apple Silicon and Intel), Linux x86_64 and Windows",
        ),
        ("Built with", "Rust, GPUI, serde, regex"),
        ("Tested on", "macOS"),
    ],
    specs_note: "The JWT decoder shows what a token says; it doesn't prove who signed it.",
};

pub fn render(site_url: &str) -> Markup {
    sections::render(site_url, &PAGE, html! {})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_title_status_and_demo() {
        let html = render("https://example.test").into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains(">FerrumKit</h1>"));
        assert!(html.contains("Animated preview of FerrumKit"));
        assert!(html.contains("/assets/img/brand/kit.svg"));
        assert!(html.contains("href=\"/download\""));
    }
}
