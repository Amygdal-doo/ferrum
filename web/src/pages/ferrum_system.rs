//! Ferrum System product page. Claims follow `system/` (the core crate's
//! model, catalogue and runtime readers, and the desktop app) and nothing
//! beyond it.

use maud::{Markup, html};

use super::product_sections::{self as sections, BulletGroup, ProductPage};
use crate::products::FERRUM_SYSTEM;

const FEATURES: &[(&str, &str)] = &[
    (
        "Kept with the code",
        "Pick a project and Ferrum System creates an .architecture folder in it: system.json \
         for the design and icons/ for your own icons. It is plain JSON, so it is reviewed \
         and versioned like the rest of the repo.",
    ),
    (
        "Layers, components and connections",
        "Components sit on layers (clients, edge, services, async, data, platform, or your \
         own). Connections are a request, internal RPC, an async message, a socket or change \
         data capture, each drawn its own way, from the side you choose, in your colour and \
         width.",
    ),
    (
        "Every cloud's names",
        "One switch shows the diagram as Generic, AWS, Azure, GCP, Hetzner, DigitalOcean or \
         open source: a load balancer reads Elastic Load Balancing, Application Gateway, \
         Cloud Load Balancing or HAProxy.",
    ),
    (
        "Technologies with icons",
        "iOS, Android, React Native and Flutter; React, Angular, Vue and more; Node.js, .NET, \
         Java, PHP, Rust, Go and others; PostgreSQL, Redis, Elasticsearch, Kafka; Ollama, \
         Claude Code, TensorFlow, Qdrant. Anything missing gets your own icon.",
    ),
    (
        "Data and flows",
        "Double-click a database for its tables, keys and references. A flow walks a request \
         through the system step by step, lighting up each hop on the diagram.",
    ),
    (
        "What runs it",
        "Each component says whether it runs as Kubernetes pods, a Docker container, a VM, \
         serverless or a managed service. The Runtime tab reads kubectl and docker and puts \
         the live pods and containers next to the components they belong to.",
    ),
];

const WORKS_TODAY: &[&str] = &[
    "Create, open and save a project's .architecture folder",
    "Add components from a palette, drag them between layers, add and reorder layers",
    "Connect components by dragging from a side handle; restyle a connection with a right-click",
    "Per-card colour, fill, borders per side, radius, texts and inner items",
    "Overview, APIs, sockets, data, flows, reliability, deploy and runtime tabs",
    "Three demos: core banking on microservices, a private RAG on Ollama, live delivery tracking",
];

const NOT_BUILT_YET: &[&str] = &[
    "Editing APIs, sockets, flows and tables inside the app (edit system.json for now)",
    "Export to PNG, SVG or Mermaid",
    "Official cloud service icons (components show a technology badge instead)",
    "Windows and Linux builds",
];

const PAGE: ProductPage = ProductPage {
    product: FERRUM_SYSTEM,
    meta_description: "Ferrum System is a native Rust app for designing a system's \
                       architecture: layers, services on any cloud, databases, flows and the \
                       pods that run them, kept in the project's .architecture folder.",
    intro: "Design the system next to its code. Ferrum System draws layers, components and the \
            connections between them, explains the design (APIs, data, flows, reliability) \
            and shows what is running right now.",
    features_lead: "The model, the catalogue, the demos and the kubectl and docker readers are \
                    unit-tested; the diagram has been clicked through by hand.",
    features: FEATURES,
    screenshots: &[
        "the core banking demo on AWS",
        "a database schema opened from the diagram",
    ],
    status_summary: "Ferrum System is new in this alpha.",
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
        ("Install", "Ferrum Launcher (free, see Download)"),
        ("Shell command", "fes, turned on in Ferrum Launcher"),
        ("Platforms", "macOS"),
        ("Built with", "Rust, GPUI, kubectl and docker"),
        ("Stored in", ".architecture/system.json in your project"),
    ],
    specs_note: "Live status needs kubectl (pods are matched by their app label) and docker \
                 (containers by name) on this Mac; without them the Runtime tab says so.",
};

pub fn render(site_url: &str) -> Markup {
    sections::render(site_url, &PAGE, html! {})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_title_and_logo() {
        let html = render("https://example.test").into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains(">Ferrum System</h1>"));
        assert!(html.contains("/assets/img/brand/system.svg"));
        assert!(html.contains("href=\"/download\""));
    }
}
