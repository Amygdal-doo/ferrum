//! `/docs` and its guides. Every guide uses [`article`]: page header, a
//! sticky index of all guides, the guide in a reading column, then the
//! launcher call to action. Copy follows the repo's READMEs and source.

pub mod install;
pub mod project_bar;
pub mod shell_commands;
pub mod shortcuts;

use maud::{Markup, html};

use crate::components::{breadcrumb::Crumb, cta, page_header::PageHeader};
use crate::layout::{PageMeta, page};

pub const PATH: &str = "/docs";

/// One guide: path, title, one-line summary.
pub struct Doc {
    pub path: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
}

/// Every guide, in reading order.
pub const DOCS: [Doc; 4] = [
    Doc {
        path: install::PATH,
        title: install::TITLE,
        summary: "Terminal one-liner or DMG, then installing, updating and rolling back apps.",
    },
    Doc {
        path: shell_commands::PATH,
        title: shell_commands::TITLE,
        summary: "fei, feg, fet and the rest: open any Ferrum app from a terminal.",
    },
    Doc {
        path: project_bar::PATH,
        title: project_bar::TITLE,
        summary: "The strip on top of the windows: switch projects with ⌥⌘O, open them elsewhere.",
    },
    Doc {
        path: shortcuts::PATH,
        title: shortcuts::TITLE,
        summary: "Keyboard shortcuts of every app, as they are bound in the code.",
    },
];

/// The trail for a guide: Docs, then the guide.
pub fn trail(title: &str, path: &str) -> Vec<Crumb> {
    vec![Crumb::new("Docs", PATH), Crumb::new(title, path)]
}

fn sidebar(current: &str) -> Markup {
    html! {
        nav class="docs-nav" aria-label="Documentation" {
            p class="docs-nav__title" { a href=(PATH) { "Docs" } }
            ul {
                @for d in &DOCS {
                    li {
                        a href=(d.path) aria-current=[(d.path == current).then_some("page")] {
                            (d.title)
                        }
                    }
                }
            }
        }
    }
}

/// A full guide page. `lead` sits under the `<h1>`; `body` is the article.
pub fn article(site_url: &str, meta: &PageMeta, title: &str, lead: Markup, body: Markup) -> Markup {
    page(
        site_url,
        meta,
        html! {
            (PageHeader::new(&meta.trail, title).lead(lead).render())
            div class="container docs-layout" {
                (sidebar(&meta.path))
                article class="docs-article prose" aria-labelledby="page-title" { (body) }
            }
            (cta::get_launcher())
        },
    )
}

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(
        Some("Documentation and guides"),
        "Guides for Ferrum: install the launcher, open apps from the shell with fei and feg, \
         switch projects with the project bar, and every keyboard shortcut.",
        PATH,
        "page-docs",
    )
    .trail(vec![Crumb::new("Docs", PATH)]);
    page(
        site_url,
        &meta,
        html! {
            (PageHeader::new(&meta.trail, "Documentation")
                .lead(html! {
                    p {
                        "How to install Ferrum and get around the apps. Each guide describes \
                         what the current alpha builds do."
                    }
                })
                .render())
            section class="section" aria-labelledby="guides-title" {
                div class="container" {
                    h2 class="visually-hidden" id="guides-title" { "Guides" }
                    ol class="doc-index" {
                        @for d in &DOCS {
                            li class="doc-index__item" {
                                h3 class="doc-index__title" { a href=(d.path) { (d.title) } }
                                p { (d.summary) }
                            }
                        }
                    }
                    p class="muted docs-more" {
                        "Looking for something else? Read the "
                        a href="/faq" { "FAQ" } ", how Ferrum handles "
                        a href="/security" { "security" } ", or what shipped in the "
                        a href="/changelog" { "changelog" } "."
                    }
                }
            }
            (cta::get_launcher())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hub_links_every_guide() {
        let html = render("https://f.test").into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        for d in &DOCS {
            assert!(
                html.contains(&format!(r#"href="{}""#, d.path)),
                "{}",
                d.path
            );
        }
    }
}
