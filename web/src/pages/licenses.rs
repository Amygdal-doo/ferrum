//! `/licenses`: the third-party works shipped inside the Ferrum apps and their
//! licence texts. Facts follow the repo's `THIRD_PARTY_NOTICES.md` and README
//! ("Licences"). The texts in `licenses/` are copies of
//! `shared/ui/assets/icons/LICENSE` (split into its ISC and MIT parts), the
//! fonts' `OFL.txt` (from the licence heading on) and `ide/LICENSE-APACHE`.

use maud::{Markup, html};

use crate::components::{breadcrumb::Crumb, page_header::PageHeader, section::Section};
use crate::layout::{PageMeta, page};
use crate::site;

pub const PATH: &str = "/licenses";

const ISC_LUCIDE: &str = include_str!("licenses/isc-lucide.txt");
const MIT_FEATHER: &str = include_str!("licenses/mit-feather.txt");
const OFL: &str = include_str!("licenses/ofl-1.1.txt");
const APACHE: &str = include_str!("licenses/apache-2.0.txt");

/// A third-party work bundled in the desktop apps.
struct Component {
    name: &'static str,
    used_for: &'static str,
    /// Version or release, and the copyright notice where the licence has one.
    detail: &'static str,
    licence: &'static str,
    /// `id` of the licence text below.
    licence_id: &'static str,
    upstream: &'static str,
}

const COMPONENTS: [Component; 4] = [
    Component {
        name: "Lucide icons",
        used_for: "The icons in every desktop app.",
        detail: "lucide-static 1.72.0. Copyright (c) 2026 Lucide Icons and Contributors. \
                 play-filled and square-filled are play and square with a solid fill.",
        licence: "ISC; icons derived from Feather: MIT",
        licence_id: "isc",
        upstream: "https://github.com/lucide-icons/lucide",
    },
    Component {
        name: "IBM Plex Sans",
        used_for: "The interface font.",
        detail: "Release @ibm/plex-sans@1.1.0, unmodified. Copyright © 2017 IBM Corp. with \
                 Reserved Font Name \"Plex\".",
        licence: "SIL Open Font License 1.1",
        licence_id: "ofl",
        upstream: "https://github.com/IBM/plex",
    },
    Component {
        name: "Lilex",
        used_for: "The code font.",
        detail: "Release 2.630, unmodified. Copyright 2019 The Lilex Project Authors.",
        licence: "SIL Open Font License 1.1",
        licence_id: "ofl",
        upstream: "https://github.com/mishamyrt/Lilex",
    },
    Component {
        name: "GPUI",
        used_for: "The GPU-accelerated UI framework every desktop app renders with.",
        detail: "The gpui and gpui_platform crates and their internal crates, from the Zed \
                 codebase, used as a library. Copyright 2022 - 2025 Zed Industries, Inc.",
        licence: "Apache-2.0",
        licence_id: "apache",
        upstream: site::ZED_UPSTREAM_URL,
    },
];

/// The website's own notable dependencies (web/Cargo.toml), with the licence
/// each crate declares.
const SITE_CRATES: [(&str, &str, &str); 6] = [
    ("axum", "Web server and routing", "MIT"),
    ("maud", "HTML templates", "MIT or Apache-2.0"),
    ("tokio", "Async runtime", "MIT"),
    ("tower-http", "Compression and headers", "MIT"),
    (
        "rust-embed",
        "CSS, scripts and images built into the binary",
        "MIT",
    ),
    (
        "reqwest",
        "Reads the release list from GitHub",
        "MIT or Apache-2.0",
    ),
];

fn licence_text(id: &str, title: &str, applies_to: &str, text: &str) -> Markup {
    html! {
        details class="licence-text" id=(id) {
            summary { (title) span class="licence-text__for" { " · " (applies_to) } }
            pre { (text.trim_end()) }
        }
    }
}

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(
        Some("Open source licences in the Ferrum apps"),
        "The third-party icons, fonts and libraries in the Ferrum apps: Lucide (ISC), IBM Plex \
         Sans and Lilex (OFL 1.1) and GPUI (Apache-2.0), with full licence texts.",
        PATH,
        "page-licenses",
    )
    .trail(vec![Crumb::new("Open source licences", PATH)]);
    page(
        site_url,
        &meta,
        html! {
            (PageHeader::new(&meta.trail, "Open source licences")
                .lead(html! {
                    p {
                        "The Ferrum apps are proprietary: © " (site::ORGANIZATION_LEGAL_NAME)
                        ", all rights reserved. They aren't open source. They include the \
                         third-party works below, each under its own licence."
                    }
                })
                .render())

            (Section::new("components", "In the apps")
                .lead(html! {
                    p {
                        "Every app links only permissively licensed code. A licence check in CI \
                         fails if a GPL-family crate gets into any app."
                    }
                })
                .render(html! {
                    div class="table-wrap" {
                        table class="data-table licence-table" {
                            caption class="visually-hidden" {
                                "Third-party works in the Ferrum apps and their licences"
                            }
                            thead {
                                tr {
                                    th scope="col" { "Component" }
                                    th scope="col" { "Used for" }
                                    th scope="col" { "Licence" }
                                    th scope="col" { "Upstream" }
                                }
                            }
                            tbody {
                                @for c in &COMPONENTS {
                                    tr {
                                        th scope="row" { (c.name) }
                                        td { (c.used_for) " " span class="licence-table__detail" { (c.detail) } }
                                        td { a href={ "#" (c.licence_id) } { (c.licence) } }
                                        td { a href=(c.upstream) rel="noopener" { (c.upstream.trim_start_matches("https://")) } }
                                    }
                                }
                            }
                        }
                    }
                }))

            (Section::new("texts", "Licence texts").band().render(html! {
                (licence_text("isc", "ISC License", "Lucide icons", ISC_LUCIDE))
                (licence_text("mit", "MIT License", "Lucide icons derived from Feather", MIT_FEATHER))
                (licence_text("ofl", "SIL Open Font License 1.1", "IBM Plex Sans and Lilex", OFL))
                (licence_text("apache", "Apache License 2.0", "GPUI", APACHE))
            }))

            (Section::new("website", "This website").tight().render(html! {
                div class="prose" {
                    p {
                        "This site is a Rust server built with axum and maud. Its notable \
                         dependencies and the licences they declare:"
                    }
                    dl class="spec-sheet" {
                        @for (name, what, licence) in SITE_CRATES {
                            div class="spec-sheet__row" { dt { (name) } dd { (what) ". " (licence) "." } }
                        }
                    }
                    p {
                        "Its fonts are loaded from Google Fonts; see "
                        a href="/privacy" { "Privacy" } "."
                    }
                }
            }))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_every_component_with_its_full_licence() {
        let html = render("https://f.test").into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains("all rights reserved"));
        for id in ["isc", "mit", "ofl", "apache"] {
            assert!(html.contains(&format!(r#"id="{id}""#)), "{id}");
        }
        assert!(html.contains("Permission to use, copy, modify, and/or distribute"));
        assert!(html.contains("Cole Bemis"));
        assert!(html.contains("SIL OPEN FONT LICENSE Version 1.1"));
        assert!(html.contains("END OF TERMS AND CONDITIONS"));
        assert!(!html.contains("GNU General Public License"));
    }
}
