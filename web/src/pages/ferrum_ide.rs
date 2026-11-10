//! Ferrum IDE product page: an independent fork of the open-source Zed editor.
//! Text-only credit to Zed; no Zed logos or trademarks are used as branding.

use maud::{Markup, html};

use super::product_sections::{self as sections, BulletGroup, ProductPage};
use crate::products::FERRUM_IDE;
use crate::site;

const FEATURES: &[(&str, &str)] = &[
    (
        "GPU-accelerated UI",
        "Inherits from upstream Zed: the interface is rendered on the GPU with GPUI, Zed's \
         Rust UI framework.",
    ),
    (
        "Written in Rust",
        "Inherits from upstream Zed: a native editor written in Rust, without Electron.",
    ),
    (
        "Multi-buffer editing",
        "Inherits from upstream Zed: edit excerpts from many files in a single buffer.",
    ),
    (
        "Tree-sitter syntax",
        "Inherits from upstream Zed: fast, incremental, structure-aware syntax highlighting.",
    ),
    (
        "Language servers",
        "Inherits from upstream Zed: language support through the Language Server Protocol.",
    ),
    (
        "Built-in terminal",
        "Inherits from upstream Zed: a terminal inside the editor.",
    ),
];

const INHERITED: &[&str] = &[
    "Fast, GPU-rendered editing",
    "Multi-buffer editing",
    "Tree-sitter syntax highlighting",
    "LSP language support",
    "Built-in terminal",
    "Collaboration features (as in upstream Zed)",
];

const FERRUM: &[&str] = &[
    "Rebranded as Ferrum IDE",
    "Its GPUI, UI and theme crates power every other Ferrum app",
    "Installed and updated through Ferrum Launcher",
    "Ferrum-specific integrations with the other apps are being explored",
];

const PAGE: ProductPage = ProductPage {
    product: FERRUM_IDE,
    meta_description: "Ferrum IDE is a GPU-accelerated code editor written in Rust: an \
                       independent fork of the open-source Zed editor. Alpha.",
    intro: "A high-performance code editor written in Rust with a GPU-accelerated UI. Ferrum \
            IDE is an independent fork of the open-source Zed editor.",
    features_lead: "Today, Ferrum IDE's capabilities come from upstream Zed. Ferrum-specific \
                    work is early and is listed under Status.",
    features: FEATURES,
    screenshots: &["Editor with a project open and the built-in terminal"],
    status_summary: "Ferrum IDE is in alpha. The fork tracks upstream Zed, and its UI \
                     framework is shared with the rest of the Ferrum family.",
    status_groups: &[
        BulletGroup {
            heading: "Inherited from upstream Zed",
            items: INHERITED,
        },
        BulletGroup {
            heading: "Ferrum-specific",
            items: FERRUM,
        },
    ],
    specs: &[
        ("Upstream", "Zed, by Zed Industries"),
        ("Built with", "Rust, GPUI (via Zed)"),
        ("Licensing", "Follows Zed's open-source licenses"),
        ("Install", "Ferrum Launcher (free, see Download)"),
    ],
    specs_note: "Builds are free public GitHub Releases, installed through Ferrum \
                 Launcher.",
};

/// Upstream credit and independence disclaimer, shown right after the hero.
fn credit() -> Markup {
    html! {
        section class="section section--tight" aria-labelledby="credit-title" {
            div class="container" {
                div class="callout product-credit" {
                    h2 id="credit-title" { "Built on Zed" }
                    p {
                        "Ferrum IDE is based on the open-source "
                        a href=(site::ZED_UPSTREAM_URL) rel="noopener" { "Zed editor" }
                        ". Zed is developed by Zed Industries, and the editing experience \
                         you get here is their work."
                    }
                    p {
                        "Ferrum IDE is an independent fork. It is not affiliated with or \
                         endorsed by Zed Industries. The fork respects upstream licensing: \
                         Zed's open-source licenses continue to apply to the code it inherits."
                    }
                }
            }
        }
    }
}

pub fn render(site_url: &str) -> Markup {
    sections::render(site_url, &PAGE, credit())
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
        assert!(html.contains(">Ferrum IDE</h1>"));
        assert!(html.contains("Alpha"));
    }

    #[test]
    fn credits_zed_and_disclaims_affiliation() {
        let html = rendered();
        assert!(html.contains("Ferrum IDE is based on the open-source"));
        assert!(html.contains(site::ZED_UPSTREAM_URL));
        assert!(html.contains("Zed Industries"));
        assert!(html.contains("not affiliated with or endorsed by Zed Industries"));
    }

    #[test]
    fn links_to_the_other_products() {
        let html = rendered();
        assert!(html.contains("href=\"/ferrumdata\""));
        assert!(html.contains("href=\"/ferrum-launcher\""));
    }
}
