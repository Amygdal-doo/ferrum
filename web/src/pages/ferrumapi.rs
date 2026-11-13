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
        "Seven request kinds",
        "REST, GraphQL, SOAP, OData, tRPC, WebSocket and gRPC requests sit side by side in \
         the same collections and folders, each with its own editor.",
    ),
    (
        "GraphQL",
        "Query, variables and operation name, sent as POST or GET. A schema panel built from \
         introspection, and field suggestions under the editor.",
    ),
    (
        "SOAP and OData",
        "SOAP 1.1 and 1.2 envelopes, with WSDL operations and a generated sample envelope. \
         An OData query builder, with entity sets read from $metadata.",
    ),
    (
        "tRPC, WebSocket and gRPC",
        "tRPC queries, mutations and batches. WebSocket text, JSON and binary messages with a \
         live log. Unary and server-streaming gRPC from reflection or a .proto file.",
    ),
    (
        "Auth",
        "Common authentication schemes configured per request instead of hand-written headers.",
    ),
    (
        "Collections in your repo",
        "Collections belong to a project, and Save to repo writes them to .ferrum/api/ so \
         they live next to the code.",
    ),
    (
        "Environments",
        "Switch between local, staging and production with {{variables}} substituted into \
         URLs, headers and bodies.",
    ),
    (
        "Postman and cURL import",
        "Bring in Postman Collection v2.1 files and paste cURL commands to start from what \
         you already have.",
    ),
    (
        "Code generation",
        "Turn a request into cURL, Python or JavaScript, or a grpcurl or websocat command.",
    ),
];

const BUILT: &[&str] = &[
    "Request builder: 7 methods, URL/params sync, headers with enable toggles",
    "Bodies: raw, JSON, form-urlencoded, multipart, binary file",
    "Auth: Basic, Bearer, API key, inherited from the collection",
    "Collections with folders, environments and globals with {{vars}}",
    "History, restored tabs, cancel and timeouts",
    "Postman Collection v2.1 import/export, cURL import, cURL/Python/JS snippets",
    "Request kinds: REST, GraphQL, SOAP, OData, tRPC, WebSocket and gRPC, mixed freely in \
     collections and folders",
    "GraphQL: query, variables, operation name, POST or GET, schema panel from introspection, \
     field suggestions",
    "SOAP 1.1 and 1.2: WSDL operations, generated sample envelopes, coloured XML responses",
    "OData: query builder for $filter, $select, $expand, $orderby, $top, $skip, $search and \
     $count; entity sets from $metadata",
    "tRPC: queries, mutations, batching and a result.data view",
    "WebSocket: ws and wss, text, JSON and binary messages, saved templates, live log",
    "gRPC: unary and server streaming, server reflection or a .proto file, grpcurl command",
    "Collections per project, saved to and loaded from .ferrum/api/ in the repo",
    "Local mock server per collection, with path params, conditions and delays",
    "Webhook inspector: capture, inspect, replay and save captures to a collection",
    "Load test with requests per second and p50/p90/p99 latency",
];

const NOT_YET: &[&str] = &[
    "OAuth 2.0 and Digest auth",
    "Pre-request and test scripts",
    "Drag-and-drop between folders",
    "Secrets in the OS keychain",
    "GraphQL subscriptions; WSDL 2.0; OData $batch",
    "gRPC client and bidirectional streaming; WebSocket auto-reconnect",
    "Automatic sync of repo collections (Save to repo and Load are on demand)",
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
                       GPUI. Alpha: REST, GraphQL, SOAP, OData, tRPC, WebSocket and gRPC requests \
                       in one collection.",
    intro: "A desktop API client written in Rust: build a request, send it, organise it. REST, \
            GraphQL, SOAP, OData, tRPC, WebSocket and gRPC in one app. Native GPUI app and \
            local-first, with no Electron shell and no webview.",
    features_lead: "These are the features in the alpha. The request engine, every request \
                    kind, storage and import/export are covered by automated tests; the GPUI \
                    interface has been launch-tested but not yet clicked through by hand.",
    features: FEATURES,
    screenshots: &["Request builder with the response panel"],
    status_summary: "FerrumAPI is in alpha. The Postman basics and six request kinds beyond \
                     REST are in place; below is what is built today and what is not yet.",
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
        (
            "Request kinds",
            "REST, GraphQL, SOAP 1.1/1.2, OData, tRPC, WebSocket, gRPC",
        ),
        (
            "Built with",
            "Rust, GPUI, reqwest + rustls, tokio-tungstenite, prost-reflect, SQLite",
        ),
        (
            "Runtime",
            "Native app, no Electron, no webview, no backend server",
        ),
        (
            "Data",
            "Stored locally on your machine; collections can be saved to .ferrum/api/ in a repo",
        ),
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
    fn lists_every_request_kind() {
        let html = rendered();
        for kind in [
            "GraphQL",
            "SOAP",
            "OData",
            "tRPC",
            "WebSocket",
            "gRPC",
            ".ferrum/api/",
        ] {
            assert!(html.contains(kind), "{kind}");
        }
        assert!(html.contains("not yet clicked through"));
    }

    #[test]
    fn links_to_the_other_products() {
        let html = rendered();
        assert!(html.contains("href=\"/ferrumdata\""));
        assert!(html.contains("href=\"/ferrumgit\""));
    }
}
