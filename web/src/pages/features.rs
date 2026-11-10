//! Features page: what each app does in the current alpha, one section per
//! app with an animated mock window (see `feature_demos`). Claims follow the
//! release notes of the versions listed here.

use maud::{Markup, html};

use super::feature_demos;
use crate::components::{
    button::{self, Variant},
    card,
};
use crate::layout::{PageMeta, page};
use crate::products::{self, Product};
use crate::site;

pub const PATH: &str = "/features";

struct AppFeatures {
    product: Product,
    version: &'static str,
    summary: &'static str,
    /// (title, body) pairs.
    points: &'static [(&'static str, &'static str)],
    demo: fn() -> Markup,
    /// Text alternative for the animated demo.
    demo_label: &'static str,
}

const APPS: [AppFeatures; 6] = [
    AppFeatures {
        product: products::FERRUM_IDE,
        version: "0.3.0-alpha.1",
        summary: "The editor is now the front door to the rest of the suite: open the project \
                  you're in with the right Ferrum app in one keystroke.",
        points: &[
            (
                "Ferrum in the command palette",
                "Open FerrumGIT, FerrumTerm, FerrumData or FerrumAPI from the palette or \
                 File → Ferrum.",
            ),
            (
                "Status-bar button",
                "A Ferrum button in the status bar lists every app, ready to launch.",
            ),
            (
                "Two keystrokes",
                "⌃⌥⌘G opens the project in FerrumGIT, ⌃⌥⌘T opens a FerrumTerm tab in the \
                 project folder.",
            ),
        ],
        demo: feature_demos::ide,
        demo_label: "Ferrum IDE editor with the command palette open on Ferrum actions and a \
                     Ferrum button in the status bar",
    },
    AppFeatures {
        product: products::FERRUMAPI,
        version: "0.2.0-alpha.1",
        summary: "Read responses faster, keep sessions alive across requests and check a \
                  whole collection in one run.",
        points: &[
            (
                "Search and colour in responses",
                "⌘F finds text in the response body, and JSON is syntax-coloured.",
            ),
            (
                "Cookie jar",
                "Cookies from Set-Cookie are stored and sent to matching hosts. View and \
                 clear them any time.",
            ),
            (
                "Collection runner",
                "Run every request in a collection or folder and see pass or fail for each.",
            ),
        ],
        demo: feature_demos::api,
        demo_label: "FerrumAPI showing a coloured JSON response with search matches and a \
                     collection run passing request by request",
    },
    AppFeatures {
        product: products::FERRUMGIT,
        version: "0.2.0-alpha.1",
        summary: "Dig into history, compare changes side by side and take back the last \
                  thing you did.",
        points: &[
            (
                "File history and blame",
                "Every commit that touched a file, following renames, plus line-by-line blame.",
            ),
            (
                "Side-by-side diff",
                "Toggle between the unified view and old and new versions next to each other.",
            ),
            (
                "Commit search",
                "Search across commits and jump through the matches with next and previous.",
            ),
            (
                "Undo",
                "Take back the last commit, amend or checkout. FerrumGIT also opens a \
                 repository passed on the command line, which is how Ferrum IDE hands \
                 projects over.",
            ),
        ],
        demo: feature_demos::git,
        demo_label: "FerrumGIT with a commit graph, a highlighted search match, a side-by-side \
                     diff and an Undo notification",
    },
    AppFeatures {
        product: products::FERRUMDATA,
        version: "0.3.0-alpha.1",
        summary: "Find the query you ran yesterday, get results out in the format you need \
                  and work the grid like a spreadsheet.",
        points: &[
            (
                "Query history",
                "Every query per connection, searchable, ready to open or run again.",
            ),
            (
                "Export and copy",
                "Export results to CSV or JSON, or copy them as CSV, JSON or Markdown.",
            ),
            (
                "A better grid",
                "Sort by column, resize columns, select cells and copy them with ⌘C.",
            ),
            (
                "Column and index details",
                "Types, defaults, keys and indexes for PostgreSQL, MySQL and SQLite tables.",
            ),
        ],
        demo: feature_demos::data,
        demo_label: "FerrumData typing a SQL query, filling a sorted results grid, with query \
                     history and an export menu",
    },
    AppFeatures {
        product: products::FERRUMTERM,
        version: "0.2.0-alpha.1",
        summary: "A terminal that keeps an eye on your AI agents, with every session one \
                  click away.",
        points: &[
            (
                "Sidebar",
                "⌘B shows every session and tab, so switching is a click away.",
            ),
            (
                "Agents section",
                "Claude Code and Codex sessions show whether they're working, done or \
                 waiting, with a badge when one needs you.",
            ),
            (
                "New agent",
                "Start Claude Code, Codex or your own command in a new tab from one menu.",
            ),
            ("Select and copy", "Select with the mouse and copy with ⌘C."),
        ],
        demo: feature_demos::term,
        demo_label: "FerrumTerm with a sidebar of sessions and agents, an agent finishing its \
                     work and terminal output being selected and copied",
    },
    AppFeatures {
        product: products::FERRUM_TOOLBOX,
        version: "0.5.0",
        summary: "One window that installs and updates the whole suite, and now updates \
                  itself too.",
        points: &[
            (
                "Updates itself in place",
                "Update Toolbox replaces the app and restarts it. No DMG to download again.",
            ),
            (
                "Alpha channel",
                "Every app follows the Alpha channel by default. Switch any app back to Beta \
                 or Stable on its page.",
            ),
            (
                "Signed and notarized",
                "Every app is signed with Developer ID and notarized by Apple, so macOS opens \
                 it without warnings.",
            ),
            (
                "One-line install",
                "Install from Terminal with a single curl command (see Download).",
            ),
        ],
        demo: feature_demos::toolbox,
        demo_label: "Ferrum Toolbox updating itself, then updating each app on the Alpha channel",
    },
];

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(
        Some("Features"),
        "What every Ferrum app can do in the current alpha: Ferrum IDE, FerrumAPI, FerrumGIT, \
         FerrumData, FerrumTerm and Ferrum Toolbox.",
        PATH,
        "page-features",
    );
    page(
        site_url,
        &meta,
        html! {
            (hero())
            @for app in &APPS {
                (app_section(app))
            }
            (credit())
        },
    )
}

fn hero() -> Markup {
    html! {
        section class="features-hero" aria-labelledby="features-title" {
            div class="container" {
                p class="eyebrow" { "Every app is in alpha" }
                h1 id="features-title" { "What's new across Ferrum" }
                p class="lead" {
                    "Six native apps, written in Rust and rendered with GPUI. This is what \
                     each one can do in the current alpha, and Ferrum Toolbox keeps them all \
                     up to date."
                }
                ul class="app-strip" {
                    @for app in &APPS {
                        li {
                            a class="app-strip__link" href={ "#" (app.product.slug) }
                                style=(app.product.accent_style()) {
                                (card::app_logo(&app.product, "app-logo--xs", true))
                                (app.product.name)
                            }
                        }
                    }
                }
            }
        }
    }
}

fn app_section(app: &AppFeatures) -> Markup {
    let p = &app.product;
    let heading = format!("{}-title", p.slug);
    html! {
        section class="feature-app" id=(p.slug) style=(p.accent_style())
            aria-labelledby=(heading) {
            div class="container feature-app__inner" {
                div class="feature-app__copy" {
                    div class="feature-app__head" {
                        (card::app_logo(p, "app-logo--sm", true))
                        div {
                            h2 id=(heading) { (p.name) }
                            p class="feature-app__meta" { (p.kind) " · v" (app.version) }
                        }
                    }
                    p class="lead" { (app.summary) }
                    ul class="feature-points" {
                        @for (title, body) in app.points {
                            li {
                                strong { (title) }
                                span { (body) }
                            }
                        }
                    }
                    a class="feature-app__more" href=(p.path()) {
                        "More about " (p.name)
                    }
                }
                div class="feature-app__demo" role="img" aria-label=(app.demo_label) {
                    ((app.demo)())
                }
            }
        }
    }
}

fn credit() -> Markup {
    html! {
        section class="features-credit" aria-labelledby="credit-title" {
            div class="container features-credit__inner" {
                p class="eyebrow" { "Made in Rust, for Rust" }
                h2 id="credit-title" {
                    "Built by "
                    a href=(site::AMYGDAL_URL) rel="noopener" { "amygdal.com" }
                    " for the Rust community"
                }
                p class="lead" {
                    "Ferrum is designed and built by Amygdal and shared with the Rust \
                     community: native tools, written in Rust, for the people who write it. \
                     Every app is free to download, and every release is a thank-you to the \
                     crates and the people that made it possible."
                }
                div class="features-credit__ctas" {
                    (button::link(site::DOWNLOAD_LABEL, site::DOWNLOAD_PATH, Variant::Primary))
                    (button::link("Visit amygdal.com", site::AMYGDAL_URL, Variant::Secondary))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered() -> String {
        render("https://example.test").into_string()
    }

    #[test]
    fn covers_every_app_with_a_demo() {
        let html = rendered();
        assert_eq!(html.matches("<h1").count(), 1);
        for p in products::ALL {
            assert!(html.contains(&format!(r#"id="{}""#, p.slug)), "{}", p.slug);
            assert!(
                html.contains(&format!(r##"href="#{}""##, p.slug)),
                "{}",
                p.slug
            );
        }
        assert_eq!(html.matches(r#"class="demo""#).count(), APPS.len());
        assert_eq!(html.matches(r#"role="img""#).count(), APPS.len());
    }

    #[test]
    fn credits_amygdal_to_the_rust_community() {
        let html = rendered();
        assert!(html.contains(site::AMYGDAL_URL));
        assert!(html.contains("for the Rust community"));
    }
}
