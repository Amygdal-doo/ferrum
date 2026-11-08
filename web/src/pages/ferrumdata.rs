//! FerrumData product page. Claims follow the FerrumData README
//! ("What works today" / "Not built yet") and nothing beyond it.

use maud::{Markup, html};

use super::product_sections::{self as sections, BulletGroup, ProductPage};
use crate::products::FERRUMDATA;

const FEATURES: &[(&str, &str)] = &[
    (
        "Desktop shell",
        "Connections sidebar, tabbed SQL editor, results panel and status bar, with resizable \
         panels that remember their size. Light, dark or system theme.",
    ),
    (
        "PostgreSQL profiles",
        "Create, edit, test and delete connection profiles with TLS mode, an environment tag \
         and an optional read-only session.",
    ),
    (
        "Passwords in your keychain",
        "Saved passwords live in the OS keychain (or only for the session) and are never \
         written to the app database.",
    ),
    (
        "Run what you mean",
        "Run the selection or the statement under the caret with Cmd/Ctrl+Enter. Results \
         stream in batches with a row limit, a timeout and server-side cancel.",
    ),
    (
        "Virtualized results grid",
        "Typed rendering for NULL, numbers, booleans, binary, JSON and timestamps, drawn only \
         for the rows on screen.",
    ),
    (
        "Production guard",
        "Mutating statements against a connection tagged as production ask for confirmation \
         first. It is a UX guard, not a security boundary.",
    ),
    (
        "Table explorer",
        "Expand a connection to see tables and views grouped by schema, loaded lazily and \
         filterable.",
    ),
    (
        "Table browser",
        "Open a table in its own tab with a breadcrumb, refresh and an editable SQL bar, \
         ordered by primary key (with a warning when there is none).",
    ),
    (
        "Picks up where you left off",
        "Editor tabs, their SQL text and table tabs are restored after a restart.",
    ),
];

const WORKS_TODAY: &[&str] = &[
    "Desktop shell with tabbed editor and persisted, resizable panels",
    "PostgreSQL connection profiles (TLS, environment tag, read-only)",
    "OS keychain credentials",
    "Streaming query execution with row limit, timeout and cancel",
    "Virtualized, typed results grid (shows up to 10,000 rows by default)",
    "Table explorer and table browser tabs",
];

const NOT_BUILT_YET: &[&str] = &[
    "Row editing, adding rows and columns",
    "Syntax highlighting and autocomplete",
    "Query history",
    "CSV / JSON export",
    "Transactions UI",
    "MySQL and SQLite support",
    "Column resize, sort and copy in the grid",
    "SSH tunnels",
    "Packaging, signing and installers",
];

const PAGE: ProductPage = ProductPage {
    product: FERRUMDATA,
    meta_description: "FerrumData is a local-first database IDE written in Rust with Dioxus and \
                       SQLx. PostgreSQL today: streaming queries, typed grid, keychain secrets.",
    intro: "A native desktop database IDE built with Dioxus 0.7 and SQLx. No backend server, \
            no Electron: your connections, tabs and settings stay on your machine.",
    features_lead: "The first PostgreSQL slice is working. This is what the alpha does today, \
                    and nothing more.",
    features: FEATURES,
    screenshots: &[
        "SQL editor with streamed results in the virtualized grid",
        "Table explorer and table browser tab",
    ],
    status_summary: "FerrumData is in alpha: the foundation and a first PostgreSQL slice work and \
                     are covered by tests in CI on macOS, Linux and Windows. Expect rough edges \
                     and missing features.",
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
        ("Platforms", "macOS, Linux and Windows targets"),
        ("Install", "Build from source today (no installers yet)"),
        (
            "Toolchain",
            "Rust 1.99.1 via rustup (pinned by the repository)",
        ),
        ("Linux", "webkit2gtk development libraries"),
        ("Windows", "WebView2 (preinstalled on Windows 10/11)"),
        ("Databases", "PostgreSQL"),
        ("Built with", "Rust, Dioxus 0.7, SQLx"),
    ],
    specs_note: "Packaging and signing have not been tested yet, so there are no installers. \
                 Downloads will appear here once they exist.",
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
        assert!(html.contains(">FerrumData</h1>"));
        assert!(html.contains("Alpha"));
    }

    #[test]
    fn is_honest_about_missing_features_and_downloads() {
        let html = rendered();
        assert!(html.contains("Not built yet"));
        assert!(html.contains("Syntax highlighting and autocomplete"));
        assert!(html.contains("Coming soon"));
        assert!(html.contains("Screenshot placeholder"));
    }

    #[test]
    fn links_to_the_other_products() {
        let html = rendered();
        assert!(html.contains("href=\"/ferrumapi\""));
        assert!(html.contains("href=\"/ferrum-ide\""));
    }
}
