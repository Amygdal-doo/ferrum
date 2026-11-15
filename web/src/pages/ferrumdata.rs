//! FerrumData product page. Claims follow the FerrumData README
//! ("What works today" / "Not built yet") and nothing beyond it.

use maud::{Markup, html};

use super::product_sections::{self as sections, BulletGroup, ProductPage};
use crate::products::FERRUMDATA;

const FEATURES: &[(&str, &str)] = &[
    (
        "Six engines",
        "PostgreSQL, MySQL 8 and MariaDB, SQLite, SurrealDB 3, MongoDB 8 and Redis 7 (alpha). \
         Every engine runs with a row limit and a timeout, and can be cancelled.",
    ),
    (
        "Connection profiles",
        "Pick the engine and fill in its fields: host and port, a SQLite file, a MongoDB \
         connection string or a SurrealDB namespace. Each profile has a TLS mode, an \
         environment tag and an optional read-only mode.",
    ),
    (
        "Passwords in your keychain",
        "Saved passwords live in the OS keychain (or only for the session) and are never \
         written to the app database.",
    ),
    (
        "Run what you mean",
        "Run the selection or the statement under the caret with Cmd/Ctrl+Enter. Results \
         stream in batches. PostgreSQL and MySQL cancel on the server.",
    ),
    (
        "MongoDB like the shell",
        "Write find, aggregate, countDocuments, distinct, insert, update and delete calls the \
         way you would in the mongo shell, or send any database command as JSON.",
    ),
    (
        "Redis keys and console",
        "Browse keys with SCAN, grouped into folders by `:` with their type and TTL. View every \
         value type in the grid, run commands in a console, and change a string, a TTL or \
         delete a key after a confirmation.",
    ),
    (
        "A results grid that keeps up",
        "Typed values (NULL, numbers, booleans, binary, JSON, timestamps) drawn only for the \
         rows on screen. Sort by any column, resize columns and copy cells or ranges.",
    ),
    (
        "History and export",
        "Every run is kept per connection in a searchable history. Export the loaded rows to \
         CSV or JSON, or copy them as CSV, JSON or Markdown.",
    ),
    (
        "Table explorer and browser",
        "Tables and views by schema, with columns and indexes for PostgreSQL, MySQL and \
         SQLite. Open a table in its own tab, ordered by primary key.",
    ),
    (
        "Production guard",
        "Mutating statements against a connection tagged as production ask for confirmation \
         first. It is a UX guard, not a security boundary.",
    ),
];

const WORKS_TODAY: &[&str] = &[
    "PostgreSQL, MySQL 8 / MariaDB, SQLite, SurrealDB 3, MongoDB 8 and Redis 7 (alpha)",
    "Connection profiles per engine (TLS mode, environment tag, read-only)",
    "Read-only enforced by the database for PostgreSQL, MySQL and SQLite; FerrumData refuses \
     writes for SurrealDB, MongoDB and Redis",
    "OS keychain credentials",
    "Streaming query execution with row limit, timeout and cancel",
    "Virtualized, typed results grid (shows up to 10,000 rows by default)",
    "Table explorer and table browser tabs",
    "Column and index details for PostgreSQL, MySQL and SQLite tables",
    "Query history, CSV / JSON export, and grid sort, resize and copy",
    "Redis key browser, value viewer, command console and quick edits",
    "Editor and table tabs restored after a restart",
];

const NOT_BUILT_YET: &[&str] = &[
    "Row editing, adding rows and columns",
    "Syntax highlighting and autocomplete",
    "Transactions UI",
    "SSH tunnels",
    "Column and index details for SurrealDB and MongoDB",
    "Editing Redis hashes, lists, sets, sorted sets and streams; pub/sub; Cluster and Sentinel",
    "Light theme (only the built-in One Dark theme renders today)",
];

const PAGE: ProductPage = ProductPage {
    product: FERRUMDATA,
    meta_description: "FerrumData is a local-first database IDE written in Rust with GPUI: \
                       PostgreSQL, MySQL, SQLite, SurrealDB, MongoDB and Redis, a typed grid \
                       and keychain secrets.",
    intro: "A native desktop database IDE built with GPUI, the UI framework behind Zed. No \
            backend server, no webview, no Electron: your connections, tabs and settings stay \
            on your machine.",
    features_lead: "Six engines in one window. This is what the alpha does today, and nothing \
                    more.",
    features: FEATURES,
    screenshots: &[
        "SQL editor with streamed results in the virtualized grid",
        "Table explorer and table browser tab",
    ],
    status_summary: "FerrumData is in alpha. Each engine is tested against a live server (Redis \
                     TLS only in unit tests), and much of the newer interface is not yet tested \
                     by hand, so expect rough edges and missing features.",
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
        ("Builds", "macOS (Apple Silicon and Intel) and Linux x86_64"),
        (
            "Databases",
            "PostgreSQL, MySQL 8 / MariaDB, SQLite, SurrealDB 3.x, MongoDB 8, Redis 7 (alpha)",
        ),
        ("Built with", "Rust, GPUI, SQLx"),
        ("Data", "Stored locally; passwords in the OS keychain"),
    ],
    specs_note: "Builds are free public GitHub Releases, installed through Ferrum Launcher.",
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
    fn is_honest_about_missing_features_and_links_download() {
        let html = rendered();
        assert!(html.contains("Not built yet"));
        assert!(html.contains("Syntax highlighting and autocomplete"));
        assert!(html.contains("href=\"/download\""));
        assert!(html.contains("Animated preview of FerrumData"));
    }

    #[test]
    fn lists_all_six_engines() {
        let html = rendered();
        for engine in [
            "PostgreSQL",
            "MySQL",
            "MariaDB",
            "SQLite",
            "SurrealDB",
            "MongoDB",
            "Redis",
        ] {
            assert!(html.contains(engine), "{engine}");
        }
        assert!(!html.contains("PostgreSQL today"));
    }

    #[test]
    fn links_to_the_other_products() {
        let html = rendered();
        assert!(html.contains("href=\"/ferrumapi\""));
        assert!(html.contains("href=\"/ferrumide\""));
    }
}
