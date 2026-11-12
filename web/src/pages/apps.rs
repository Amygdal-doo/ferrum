//! `/apps`: every Ferrum app with its logo, one line and its key features.
//! The home page shows the four main apps from [`MAIN`] next to their demos.
//! Facts come from each app's README ("What works today") and catalog.json.

use maud::{Markup, html};
use serde_json::json;

use crate::components::{badge, breadcrumb::Crumb, card, cta, page_header::PageHeader};
use crate::layout::{PageMeta, page};
use crate::products::{self, Product};

pub const PATH: &str = "/apps";

pub struct AppEntry {
    pub product: Product,
    pub summary: &'static str,
    pub features: &'static [&'static str],
}

/// The four apps most of a working day happens in. Home shows these.
pub const MAIN: [AppEntry; 4] = [
    AppEntry {
        product: products::FERRUM_EDITOR,
        summary: "A small, hackable code editor written from scratch in Rust and configured in \
                  plain TOML.",
        features: &[
            "Tree view, tabs, split panes, a fuzzy file finder and a command palette",
            "Multi-cursor editing, regex find and replace, and find in files that respects \
             .gitignore",
            "Completions, problems and go to definition for Rust, Go, TypeScript and JavaScript",
            "A built-in terminal, ⌘B builds, and Git staging in the sidebar",
            "CI and Logs pages: GitHub Actions runs, and live logs of dev servers and containers",
        ],
    },
    AppEntry {
        product: products::FERRUMGIT,
        summary: "A native Git client with a commit graph, line staging and per-project SSH \
                  profiles.",
        features: &[
            "A commit graph with branches, tags and your uncommitted work on top",
            "Stage, unstage or discard one hunk, or only the lines you select",
            "Side-by-side diffs, file history that follows renames, and blame",
            "GitHub and GitLab pull requests: review, check out, create and merge",
            "Undo for commit, amend and checkout",
        ],
    },
    AppEntry {
        product: products::FERRUMTERM,
        summary: "A GPU terminal whose sessions live in a server, so closing the window stops \
                  nothing.",
        features: &[
            "Sessions, tabs and splits that keep running in a background server",
            "The fterm CLI starts, splits, types into and reads any pane, with --json output",
            "An agents sidebar shows when Claude Code or Codex is working, done or blocked",
            "SSH hosts from ~/.ssh/config in one click, and port forwards that keep running",
        ],
    },
    AppEntry {
        product: products::FERRUMDATA,
        summary: "A local-first database IDE for PostgreSQL, MySQL and MariaDB, SQLite, \
                  SurrealDB, MongoDB and Redis.",
        features: &[
            "Run the statement under the cursor and stream it into a typed results grid",
            "Sort the grid, keep a query history, and export to CSV or JSON",
            "Browse tables with their columns and indexes, and Redis keys with their TTL",
            "Read-only profiles, a confirmation before writes on production, and passwords in \
             the keychain",
        ],
    },
];

/// The tools around the main four.
pub const MORE: [AppEntry; 5] = [
    AppEntry {
        product: products::FERRUMAPI,
        summary: "A native, local-first API client with the Postman basics, minus the bloat.",
        features: &[
            "Every HTTP method; JSON, form, multipart and binary bodies; Basic, Bearer or API key \
             auth",
            "Collections, environments and {{variables}}, with history and tabs restored on launch",
            "Postman and cURL import, and code for cURL, Python and JavaScript",
            "A collection runner, a local mock server and a webhook inspector",
            "Load tests with requests per second and p50, p90 and p99 latency",
        ],
    },
    AppEntry {
        product: products::FERRUMKIT,
        summary: "The small conversions you would look up on random websites, in one native \
                  window that works offline.",
        features: &[
            "Format, validate and convert JSON, YAML and TOML, with the exact error position",
            "Decode JWTs, with the expiry shown as a date",
            "Base64, URL, timestamps, UUID and ULID, hashes, number bases and colours",
            "A regex tester with capture groups, and a line diff of two texts",
        ],
    },
    AppEntry {
        product: products::FERRUMCONTAINERS,
        summary: "A native window on your containers, for Docker Desktop, OrbStack, Colima and \
                  Podman.",
        features: &[
            "Containers grouped by Compose project, with ports and live CPU and memory",
            "Start, stop, restart or remove, and bring a Compose project up or down",
            "Live logs with search, and a shell in the container through FerrumTerm",
            "Images, volumes and networks, with a prune that shows the space you get back",
        ],
    },
    AppEntry {
        product: products::FERRUMPORTS,
        summary: "Every listening port with its process and project, and a confirmed way to \
                  stop it.",
        features: &[
            "TCP listeners, and UDP on request, grouped by the git project they run in",
            "Stop a process after a confirmation that shows its full command",
            "Check whether a port is free, and get the nearest free one if it isn't",
            "Names for about 45 well-known ports, and search across every column",
        ],
    },
    AppEntry {
        product: products::FERRUMENV,
        summary: "A password-locked vault for your projects' environment variables.",
        features: &[
            "Projects and environments behind a master password, locked after 30 idle minutes",
            "Secret values masked, and the clipboard cleared after a copy",
            "Send a teammate a project's variables, encrypted with its share key",
            "Import and write .env files, and compare two environments",
        ],
    },
];

pub const LAUNCHER: AppEntry = AppEntry {
    product: products::FERRUM_LAUNCHER,
    summary: "Installs, updates, rolls back and launches every Ferrum app from one window.",
    features: &[
        "Every version comes from GitHub Releases, with its checksum verified",
        "Stable and Beta update channels for each app",
        "Keeps earlier versions, so you can switch back in one click",
        "No account and no sign-in, and it updates itself in place",
    ],
};

const DESCRIPTION: &str = "Every Ferrum app in one list: code editor, Git client, terminal, \
     database IDE, API client, utilities, containers, ports, env vault and the launcher.";

pub fn render(site_url: &str) -> Markup {
    let trail = vec![Crumb::new("Apps", PATH)];
    let meta = PageMeta::new(Some("All Ferrum apps"), DESCRIPTION, PATH, "page-apps")
        .trail(trail.clone())
        .json_ld(item_list(site_url));
    page(
        site_url,
        &meta,
        html! {
            (PageHeader::new(&trail, "Every Ferrum app")
                .lead(html! {
                    p {
                        "Ten native macOS apps, written in Rust and drawn on the GPU. All of \
                         them are free and in alpha, and Ferrum Launcher installs any of them \
                         and keeps it up to date."
                    }
                })
                .render())
            (group("main", "Write it, commit it, run it, query it",
                "The four apps most of a working day happens in.", &MAIN))
            (group("more", "Tools for the rest of the job",
                "APIs, containers, ports, secrets, and the small conversions in between.", &MORE))
            (group("launcher", "The launcher",
                "One free download that installs every other app.", std::slice::from_ref(&LAUNCHER)))
            (cta::get_launcher())
        },
    )
}

fn group(id: &str, title: &str, lead: &str, entries: &[AppEntry]) -> Markup {
    let title_id = format!("{id}-title");
    html! {
        section class="app-group" aria-labelledby=(title_id) {
            div class="container" {
                div class="app-group__head" {
                    h2 id=(title_id) { (title) }
                    p { (lead) }
                }
                @for e in entries { (row(e)) }
            }
        }
    }
}

fn row(e: &AppEntry) -> Markup {
    let p = &e.product;
    let title_id = format!("app-{}", p.slug);
    html! {
        article class="app-row" style=(p.accent_style()) aria-labelledby=(title_id) {
            div class="app-row__id" {
                (card::app_logo(p, "app-row__logo", true))
                div {
                    h3 class="app-row__name" id=(title_id) {
                        a href=(p.path()) { (p.name) }
                    }
                    p class="app-row__kind" { (p.kind) " " (badge::status(p.status)) }
                }
            }
            div class="app-row__copy" {
                p class="app-row__summary" { (e.summary) }
                a class="app-row__more" href=(p.path()) {
                    "More about " (p.name)
                }
            }
            ul class="app-row__features" {
                @for f in e.features { li { (f) } }
            }
        }
    }
}

/// Every entry, in page order.
pub fn all() -> impl Iterator<Item = &'static AppEntry> {
    MAIN.iter()
        .chain(MORE.iter())
        .chain(std::iter::once(&LAUNCHER))
}

fn item_list(site_url: &str) -> serde_json::Value {
    let items: Vec<_> = all()
        .enumerate()
        .map(|(i, e)| {
            json!({
                "@type": "ListItem",
                "position": i + 1,
                "name": e.product.name,
                "url": format!("{site_url}{}", e.product.path()),
            })
        })
        .collect();
    json!({
        "@context": "https://schema.org",
        "@type": "ItemList",
        "name": "Ferrum apps",
        "itemListElement": items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_every_product_once_with_logo_link_and_features() {
        assert_eq!(all().count(), products::ALL.len());
        let html = render("https://example.test").into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        for p in products::ALL {
            assert_eq!(html.matches(&format!(r#"id="app-{}""#, p.slug)).count(), 1);
            assert!(
                html.contains(&format!(r#"href="{}""#, p.path())),
                "{}",
                p.slug
            );
            assert!(html.contains(&format!("/assets/{}?v=", p.logo_asset())));
        }
        for e in all() {
            assert!(e.features.len() >= 4, "{}", e.product.slug);
        }
        assert!(html.contains(r#""@type":"ItemList""#));
        assert!(html.contains(r#"href="/download""#));
    }

    #[test]
    fn api_claims_only_what_ships() {
        let api = MORE.iter().find(|e| e.product.slug == "ferrumapi");
        let text = api.map(|e| e.features.join(" ")).unwrap_or_default();
        for soon in ["GraphQL", "SOAP", "OData", "tRPC", "WebSocket", "gRPC"] {
            assert!(!text.contains(soon), "{soon}");
        }
    }
}
