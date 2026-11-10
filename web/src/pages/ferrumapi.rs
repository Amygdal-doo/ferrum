//! FerrumAPI product page. Claims follow the FerrumAPI README: the core is
//! built and tested, the UI is in alpha, not yet hand-tested.

use maud::{Markup, html};

use super::product_sections::{self as sections, BulletGroup, ProductPage};
use crate::products::FERRUMAPI;

const FEATURES: &[(&str, &str)] = &[
    (
        "Request builder",
        "Method, URL, query params and headers, edited side by side with the response.",
    ),
    (
        "Auth",
        "Common authentication schemes configured per request instead of hand-written headers.",
    ),
    (
        "Request bodies",
        "Compose the body your API expects alongside the rest of the request.",
    ),
    (
        "Collections",
        "Group related requests so a project's API lives in one place.",
    ),
    (
        "Environments",
        "Switch between local, staging and production with {{variables}} substituted into \
         URLs, headers and bodies.",
    ),
    ("History", "Look back at requests you have already sent."),
    (
        "Postman and cURL import",
        "Bring in Postman Collection v2.1 files and paste cURL commands to start from what \
         you already have.",
    ),
    (
        "Code generation",
        "Turn a request into a snippet you can drop into code or a terminal.",
    ),
];

const BUILT: &[&str] = &[
    "Request builder: 7 methods, URL/params sync, headers with enable toggles",
    "Bodies: raw, JSON, form-urlencoded, multipart, binary file",
    "Auth: Basic, Bearer, API key, inherited from the collection",
    "Collections with folders, environments and globals with {{vars}}",
    "History, restored tabs, cancel and timeouts",
    "Postman Collection v2.1 import/export, cURL import, cURL/Python/JS snippets",
    "Local mock server per collection, with path params, conditions and delays",
    "Webhook inspector: capture, inspect and replay incoming requests",
    "Load test with requests per second and p50/p90/p99 latency",
];

const NOT_YET: &[&str] = &[
    "OAuth 2.0 and Digest auth",
    "Pre-request and test scripts",
    "Drag-and-drop between folders",
    "Secrets in the OS keychain",
    "WebSocket, GraphQL editor and gRPC",
    "Light theme and soft-wrap in the response view",
    "HTTPS mocks, templated mock bodies and a public tunnel for webhooks",
];

const PRINCIPLES: &[&str] = &[
    "Native Rust desktop app, no Electron",
    "Local-first: collections and environments stay on your machine",
    "Rendered with GPUI and the shared Ferrum UI, like every Ferrum app",
];

const PAGE: ProductPage = ProductPage {
    product: FERRUMAPI,
    meta_description: "FerrumAPI is a native, local-first desktop API client written in Rust with \
                       GPUI. Alpha: request builder, collections, environments, Postman import.",
    intro: "A desktop API client written in Rust, focused on the Postman basics: build a \
            request, send it, organise it. Native GPUI app and local-first, with no Electron \
            shell and no webview.",
    features_lead: "These are the features in the alpha. The request engine, storage \
                    and import/export are covered by automated tests; the GPUI interface has been \
                    launch-tested and is still being polished.",
    features: FEATURES,
    screenshots: &["Request builder with the response panel"],
    status_summary: "FerrumAPI is in alpha. The Postman basics are in place; below is \
                     what is built today and what is not yet.",
    status_groups: &[
        BulletGroup {
            heading: "Built in the preview",
            items: BUILT,
        },
        BulletGroup {
            heading: "Not built yet",
            items: NOT_YET,
        },
        BulletGroup {
            heading: "Approach",
            items: PRINCIPLES,
        },
    ],
    specs: &[
        ("Install", "Ferrum Launcher (free, see Download)"),
        ("Builds", "macOS (Apple Silicon and Intel) and Linux x86_64"),
        ("Built with", "Rust, GPUI, reqwest + rustls, SQLite"),
        (
            "Runtime",
            "Native app, no Electron, no webview, no backend server",
        ),
        ("Data", "Stored locally on your machine"),
    ],
    specs_note: "Builds are free public GitHub Releases, installed through Ferrum \
                 Launcher. Only macOS has been smoke-tested so far.",
};

pub fn render(site_url: &str) -> Markup {
    sections::render(site_url, &PAGE, html! {})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered() -> String {
        render("https://example.test").into_string()
    }

    #[test]
    fn renders_single_h1_and_alpha_status() {
        let html = rendered();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains(">FerrumAPI</h1>"));
        assert!(html.contains("Alpha"));
    }

    #[test]
    fn lists_built_and_missing_features() {
        let html = rendered();
        assert!(html.contains("Built in the preview"));
        assert!(html.contains("Not built yet"));
        assert!(html.contains("Postman Collection v2.1"));
        assert!(html.contains("{{variables}}"));
        assert!(html.contains("no Electron"));
    }

    #[test]
    fn links_to_the_other_products() {
        let html = rendered();
        assert!(html.contains("href=\"/ferrumdata\""));
        assert!(html.contains("href=\"/ferrumgit\""));
    }
}
