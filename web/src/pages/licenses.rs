//! `/licenses`: Ferrum's own licence (MIT OR Apache-2.0), then the
//! third-party works shipped inside the apps and their licence texts. Facts follow the repo's `THIRD_PARTY_NOTICES.md` and README
//! ("Licences"). The texts in `licenses/` are copies of
//! `shared/ui/assets/icons/LICENSE` (split into its ISC and MIT parts), the
//! fonts' `OFL.txt` (from the licence heading on) and the Apache-2.0 text
//! (the repository's `LICENSE-APACHE`; GPUI uses the same licence).

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

/// The website's own notable dependencies (web/Cargo.toml and the scripts in
/// web/assets/js), with the licence each declares.
const SITE_CRATES: [(&str, &str, &str); 7] = [
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
    ("htmx", "Moves between pages without a full reload", "0BSD"),
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
        Some("Licences: Ferrum is MIT OR Apache-2.0"),
        "Ferrum is open source under MIT OR Apache-2.0. The third-party works in the apps, \
         Lucide (ISC), IBM Plex Sans and Lilex (OFL 1.1) and GPUI (Apache-2.0), with full texts.",
        PATH,
        "page-licenses",
    )
    .trail(vec![Crumb::new("Licences", PATH)]);
    page(
        site_url,
        &meta,
        html! {
            (PageHeader::new(&meta.trail, "Licences")
                .lead(html! {
                    p {
                        "Ferrum is open source under the MIT licence or Apache-2.0, at your option. \
                         The apps also include the third-party works below, each under its own \
                         licence."
                    }
                })
                .render())

            (Section::new("ferrum", "Ferrum's licence").tight().render(html! {
                div class="prose" {
                    p {
                        "The source of every Ferrum app, FerrumLauncher, the shared "
                        code { "ferrum-ui" } " crate and this website is in "
                        a href=(site::SOURCE_REPO_URL) rel="noopener" { (site::SOURCE_REPO_SLUG) }
                        " on GitHub. You may use it under either licence:"
                    }
                    dl class="spec-sheet" {
                        div class="spec-sheet__row" {
                            dt { "SPDX" }
                            dd { code { (site::LICENSE_SPDX) } }
                        }
                        div class="spec-sheet__row" {
                            dt { "MIT" }
                            dd { a href=(site::LICENSE_MIT_URL) rel="noopener" { "LICENSE-MIT" } }
                        }
                        div class="spec-sheet__row" {
                            dt { "Apache-2.0" }
                            dd {
                                a href=(site::LICENSE_APACHE_URL) rel="noopener" { "LICENSE-APACHE" }
                                " (full text "
                                a href="#apache" { "below" } ")"
                            }
                        }
                    }
                    p {
                        "Unless you say otherwise, what you contribute is licensed the same way, \
                         with no contributor licence agreement or sign-off. See "
                        a href=(site::CONTRIBUTING_URL) rel="noopener" { "CONTRIBUTING.md" } "."
                    }
                }
            }))

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
                (licence_text("apache", "Apache License 2.0", "Ferrum (with MIT as the alternative) and GPUI", APACHE))
            }))

            (Section::new("website", "This website").tight().render(html! {
                div class="prose" {
                    p {
                        "This site is a Rust server built with axum, maud and htmx. Its notable \
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
        assert!(html.contains("MIT OR Apache-2.0"));
        assert!(html.contains(r#"id="ferrum""#));
        assert!(html.contains(site::SOURCE_REPO_URL));
        let lower = html.to_lowercase();
        for gone in ["proprietary", "all rights reserved", "aren't open source"] {
            assert!(!lower.contains(gone), "{gone}");
        }
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
