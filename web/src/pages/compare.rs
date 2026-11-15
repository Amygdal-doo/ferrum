//! `/compare` and `/compare/<slug>`: side-by-side notes on a Ferrum app and
//! a well-known tool. Ferrum's column follows the app's README ("works
//! today" / "not built yet"); the other column states only general,
//! long-standing facts about that product, and the page says they can change.

use maud::{Markup, html};

use crate::components::{
    breadcrumb::Crumb,
    button::{self, Variant},
    callout::{self, Kind},
    cta,
    page_header::PageHeader,
    section::Section,
};
use crate::layout::{PageMeta, page};
use crate::products::{self, Product};

pub const PATH: &str = "/compare";

pub struct Comparison {
    pub slug: &'static str,
    pub product: Product,
    pub other: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub summary: &'static str,
    /// (topic, Ferrum app, other product).
    pub rows: &'static [(&'static str, &'static str, &'static str)],
    /// When the other product is the better fit.
    pub other_fits: &'static [&'static str],
    /// When the Ferrum app is the better fit.
    pub ferrum_fits: &'static [&'static str],
}

impl Comparison {
    pub fn path(&self) -> String {
        format!("{PATH}/{}", self.slug)
    }
}

pub const COMPARISONS: [Comparison; 2] = [
    Comparison {
        slug: "ferrumcontainers-vs-docker-desktop",
        product: products::FERRUMCONTAINERS,
        other: "Docker Desktop",
        title: "FerrumContainers vs Docker Desktop",
        description: "FerrumContainers is a native window on the container engine you already run, \
                      Docker Desktop's included. How the two differ, and when to use each.",
        summary: "They aren't alternatives in the usual sense. Docker Desktop ships a container \
                  engine with a dashboard; FerrumContainers ships no engine and talks to the one you \
                  run, Docker Desktop's included.",
        rows: &[
            (
                "Container engine",
                "None included. Connects to Docker Desktop, OrbStack, Colima, or any engine in \
                 DOCKER_HOST such as Podman",
                "Included, running in a virtual machine on macOS",
            ),
            (
                "Containers",
                "Grouped by Compose project, with live CPU and memory; start, stop, restart, \
                 pause, remove",
                "Listed in the dashboard with start, stop and remove",
            ),
            (
                "Logs and shell",
                "Live logs with search and timestamps; a shell in a FerrumTerm tab (or Terminal)",
                "Logs and an exec terminal in the dashboard",
            ),
            (
                "Images, volumes, networks",
                "Pull with progress, remove and prune, with the space you get back",
                "Managed in the dashboard",
            ),
            (
                "Building images",
                "Not built yet",
                "Yes, with the bundled Docker CLI",
            ),
            ("Kubernetes", "Not built yet", "Optional local cluster"),
            (
                "Private registries",
                "Not built yet (no registry sign-in)",
                "Yes, with docker login",
            ),
            (
                "App",
                "Native Rust app drawn with GPUI, installed by FerrumLauncher",
                "Desktop app with an Electron-based dashboard",
            ),
            (
                "Price",
                "Free download",
                "Free for personal use, education and small businesses; larger companies need a \
                 paid subscription (see Docker's terms)",
            ),
        ],
        other_fits: &[
            "You need a container engine on your Mac and want it set up for you.",
            "You build images, run a local Kubernetes cluster or pull from private registries.",
        ],
        ferrum_fits: &[
            "You already run an engine (Docker Desktop, OrbStack, Colima, Podman) and want a \
             fast window on it.",
            "You switch between engines, or want containers grouped by Compose project next to \
             your other Ferrum tools.",
        ],
    },
    Comparison {
        slug: "ferrumapi-vs-postman",
        product: products::FERRUMAPI,
        other: "Postman",
        title: "FerrumAPI vs Postman",
        description: "FerrumAPI is a local-first, native API client that imports Postman \
                      collections. How it compares on accounts, storage, scripts and teams.",
        summary: "FerrumAPI covers the everyday Postman basics, from REST to GraphQL and gRPC, \
                  on your Mac alone: no account, no cloud, data in a local file. Postman is a broader platform built around \
                  shared, synced workspaces.",
        rows: &[
            (
                "Account",
                "None",
                "A Postman account for workspaces and sync",
            ),
            (
                "Where your data lives",
                "A local SQLite file on your Mac",
                "Postman's cloud, synced across devices and teammates",
            ),
            (
                "Collaboration",
                "Not built in; save collections to .ferrum/api/ in your repo or share them as \
                 Postman v2.1 files",
                "Shared workspaces for teams",
            ),
            (
                "Import and export",
                "Postman Collection v2.1 (and v2.0 import), Postman environments, cURL",
                "Postman collections, OpenAPI, cURL and more",
            ),
            (
                "Protocols",
                "HTTP, GraphQL, SOAP, OData, tRPC, WebSocket and gRPC, mixed in one collection; \
                 no GraphQL subscriptions or gRPC client streaming yet",
                "HTTP, GraphQL, gRPC, WebSocket and more",
            ),
            (
                "Auth",
                "Basic, Bearer, API key, inherited from folders; OAuth, Digest and AWS not built \
                 yet",
                "Includes OAuth 1.0 and 2.0, Digest and AWS Signature",
            ),
            (
                "Scripts",
                "Not built yet (the runner passes on 2xx)",
                "Pre-request and test scripts in JavaScript",
            ),
            (
                "Local tools",
                "Mock server per collection, webhook inspector (save captures to a collection) \
                 and load test (p50/p90/p99), all on 127.0.0.1",
                "Mock servers, monitors and performance tests",
            ),
            (
                "App",
                "Native Rust app drawn with GPUI, no webview",
                "Electron desktop app and a web app",
            ),
            (
                "Price",
                "Free download",
                "Free plan with limits; paid plans for teams",
            ),
        ],
        other_fits: &[
            "Your team shares collections and needs them synced.",
            "You rely on scripts, OAuth, or GraphQL subscriptions and streaming gRPC.",
        ],
        ferrum_fits: &[
            "You want requests, history and secrets to stay on your Mac, with no account.",
            "You want a fast native client with a local mock server, webhook inspector and \
             load test built in.",
        ],
    },
];

pub fn find(slug: &str) -> Option<&'static Comparison> {
    COMPARISONS.iter().find(|c| c.slug == slug)
}

pub fn index(site_url: &str) -> Markup {
    let meta = PageMeta::new(
        Some("Compare Ferrum with other developer tools"),
        "Side-by-side notes on Ferrum apps and well-known tools: FerrumContainers and Docker Desktop, \
         FerrumAPI and Postman. What each does today, and when to pick which.",
        PATH,
        "page-compare",
    )
    .trail(vec![Crumb::new("Compare", PATH)]);
    page(
        site_url,
        &meta,
        html! {
            (PageHeader::new(&meta.trail, "Compare")
                .lead(html! {
                    p {
                        "How a Ferrum app sits next to a tool you may already use: what each does \
                         today, and when the other one is the better fit."
                    }
                })
                .render())
            section class="section" aria-labelledby="comparisons-title" {
                div class="container" {
                    h2 class="visually-hidden" id="comparisons-title" { "Comparisons" }
                    ul class="doc-index" {
                        @for c in &COMPARISONS {
                            li class="doc-index__item" style=(c.product.accent_style()) {
                                h3 class="doc-index__title" { a href=(c.path()) { (c.title) } }
                                p { (c.summary) }
                            }
                        }
                    }
                }
            }
        },
    )
}

pub fn render(site_url: &str, c: &Comparison) -> Markup {
    let path = c.path();
    let p = &c.product;
    let meta = PageMeta::new(Some(c.title), c.description, &path, "page-compare")
        .trail(vec![
            Crumb::new("Compare", PATH),
            Crumb::new(c.title, &path),
        ])
        .og_image(&p.og_image_asset(), &format!("{} logo and name", p.name));
    page(
        site_url,
        &meta,
        html! {
            (PageHeader::new(&meta.trail, c.title)
                .accent(p.accent_style())
                .lead(html! { p { (c.summary) } })
                .render())
            (Section::new("side-by-side", "Side by side").render(html! {
                div class="table-wrap" {
                    table class="data-table compare-table" style=(p.accent_style()) {
                        caption class="visually-hidden" { (p.name) " compared with " (c.other) }
                        thead {
                            tr {
                                th scope="col" { span class="visually-hidden" { "Topic" } }
                                th scope="col" { (p.name) }
                                th scope="col" { (c.other) }
                            }
                        }
                        tbody {
                            @for (topic, ours, theirs) in c.rows {
                                tr {
                                    th scope="row" { (topic) }
                                    td { (ours) }
                                    td { (theirs) }
                                }
                            }
                        }
                    }
                }
                (callout::render(Kind::Note, None, html! {
                    p {
                        "The " (p.name) " column follows its current alpha. The " (c.other)
                        " column describes the product in general terms, and its features, plans \
                         and prices can change; check its own documentation for details."
                    }
                }))
            }))
            (Section::new("which", "Which one fits").band().render(html! {
                div class="grid grid--2" {
                    div {
                        h3 { (p.name) }
                        ul class="prose-list" { @for f in c.ferrum_fits { li { (f) } } }
                    }
                    div {
                        h3 { (c.other) }
                        ul class="prose-list" { @for f in c.other_fits { li { (f) } } }
                    }
                }
                p class="compare-more" {
                    (button::link(&format!("More about {}", p.name), &p.path(), Variant::Secondary))
                }
            }))
            (cta::get_launcher())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_comparison_renders_a_fair_table() {
        for c in &COMPARISONS {
            let html = render("https://f.test", c).into_string();
            assert_eq!(html.matches("<h1").count(), 1);
            assert_eq!(html.matches("<th scope=\"row\">").count(), c.rows.len());
            assert!(html.contains("can change"));
            assert!(html.contains(&format!(r#"href="{}""#, c.product.path())));
        }
        assert!(find("ferrumapi-vs-postman").is_some());
        assert!(find("nope").is_none());
    }
}
