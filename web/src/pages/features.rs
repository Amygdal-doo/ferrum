//! Features page: what each app does in the current alpha, one section per
//! app with an animated mock window (see `feature_demos`). Claims follow the
//! release notes of the versions listed here.

use maud::{Markup, html};

use super::feature_demos;
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

const APPS: [AppFeatures; 7] = [
    AppFeatures {
        product: products::FERRUM_EDITOR,
        version: "0.3.0-alpha.1",
        summary: "The suite's code editor: small, hackable and in the spirit of Atom, written \
                  from scratch in Rust. Light, fast to start, and yours to rewire.",
        points: &[
            (
                "Tree view, tabs and palette",
                "Split panes, a fuzzy file finder and a command palette that lists every \
                 command with its keys.",
            ),
            (
                "Multi-cursor",
                "Add carets with ⌘-click or ⌘D, split a selection into lines, and type, \
                 paste or undo on all of them at once.",
            ),
            (
                "Find everywhere",
                "Find and replace with regex in the file, and find in files that respects \
                 .gitignore.",
            ),
            (
                "Completions, problems and definitions",
                "Autocomplete, underlined errors and go to definition with F12 for Rust, Go, \
                 TypeScript and JavaScript. The language server installs itself the first \
                 time you open a file.",
            ),
            (
                "Build and Git built in",
                "⌘B builds with cargo, go or npm in the terminal and lists the errors. \
                 Stage, commit and push from the sidebar, and switch to a Git view for diffs.",
            ),
            (
                "Built-in terminal",
                "Open a terminal at the bottom from the status bar or with ⌃`, with tabs \
                 that keep running while the panel is hidden.",
            ),
            (
                "Themes and file icons",
                "Pick a theme with live preview, write your own in TOML or import a VS Code \
                 theme, and load your own file icon sets.",
            ),
        ],
        demo: feature_demos::editor,
        demo_label: "Ferrum Editor with a tree view and tabs: three cursors type at once, then \
                     the command palette filters commands on toggle",
    },
    AppFeatures {
        product: products::FERRUMAPI,
        version: "0.3.0-alpha.1",
        summary: "Three local tools for the parts of API work that happen before the backend \
                  is ready, or after it ships.",
        points: &[
            (
                "Mock server",
                "Serve a collection's mock responses on 127.0.0.1, with path parameters, \
                 query and header conditions, delays and a live log of what answered.",
            ),
            (
                "Webhook inspector",
                "A local listener that records every request it gets. Open any capture as a \
                 new request to replay it.",
            ),
            (
                "Load test",
                "Send a request N times with C at once and get requests per second, errors \
                 by status and p50, p90 and p99 latency.",
            ),
        ],
        demo: feature_demos::api,
        demo_label: "FerrumAPI showing a coloured JSON response with search matches and a \
                     collection run passing request by request",
    },
    AppFeatures {
        product: products::FERRUMGIT,
        version: "0.3.0-alpha.1",
        summary: "Review and ship pull requests without leaving the Git client.",
        points: &[
            (
                "GitHub and GitLab",
                "Pull and merge requests for github.com, gitlab.com and self-hosted GitLab or \
                 GitHub Enterprise, with a token in the keychain or your gh login.",
            ),
            (
                "Review in place",
                "Description, conversation, commits, checks, and the changed files in \
                 FerrumGIT's own diff view.",
            ),
            (
                "Act on it",
                "Check out a pull request, open one from your branch, comment, approve or \
                 request changes, and merge after a confirmation.",
            ),
        ],
        demo: feature_demos::git,
        demo_label: "FerrumGIT with a commit graph, a highlighted search match, a side-by-side \
                     diff and an Undo notification",
    },
    AppFeatures {
        product: products::FERRUMDATA,
        version: "0.4.0-alpha.1",
        summary: "Redis joins PostgreSQL, MySQL, SQLite, SurrealDB and MongoDB.",
        points: &[
            (
                "Redis key browser",
                "Keys found with SCAN and grouped into folders by `:`, each with its type \
                 and TTL.",
            ),
            (
                "Every value type",
                "Strings (with pretty JSON), hashes, lists, sets, sorted sets and streams, \
                 shown in the results grid with sort, copy and export.",
            ),
            (
                "Command console",
                "Run Redis commands with redis-cli quoting. Read-only profiles only allow \
                 read commands.",
            ),
            (
                "Quick edits",
                "Change a string, set or remove a TTL, or delete a key, with a confirmation.",
            ),
        ],
        demo: feature_demos::data,
        demo_label: "FerrumData typing a SQL query, filling a sorted results grid, with query \
                     history and an export menu",
    },
    AppFeatures {
        product: products::FERRUMTERM,
        version: "0.3.0-alpha.1",
        summary: "Remote machines and the ports on yours, from the sidebar.",
        points: &[
            (
                "SSH hosts",
                "Hosts from ~/.ssh/config and ones you save open in a new tab with one click, \
                 or with fterm ssh. Passwords and keys stay with ssh.",
            ),
            (
                "Port forwards",
                "Local, remote and SOCKS forwards per host, started and stopped from the \
                 sidebar. They keep running when you close the window.",
            ),
            (
                "What's using a port?",
                "See which process listens on a port, and stop it after a confirmation. Also \
                 fterm port 3000.",
            ),
        ],
        demo: feature_demos::term,
        demo_label: "FerrumTerm with a sidebar of sessions and agents, an agent finishing its \
                     work and terminal output being selected and copied",
    },
    AppFeatures {
        product: products::FERRUMKIT,
        version: "0.1.0-alpha.1",
        summary: "New: the small tools you otherwise look up on random websites, in one native \
                  window that works offline.",
        points: &[
            (
                "JSON, YAML and TOML",
                "Format, minify and validate with the exact error position, and convert \
                 between the three.",
            ),
            (
                "JWT decoder",
                "Header and payload pretty-printed, expiry shown as a date. Tokens never \
                 leave your Mac.",
            ),
            (
                "Everyday converters",
                "Base64, URL, timestamps, UUID and ULID, hashes, number bases, colours and \
                 text case.",
            ),
            (
                "Regex tester and diff",
                "Live match highlighting with capture groups, and a line diff of two texts.",
            ),
        ],
        demo: feature_demos::kit,
        demo_label: "FerrumKit decoding a JWT: the header and payload are shown and the token \
                     is marked expired",
    },
    AppFeatures {
        product: products::FERRUMDOCK,
        version: "0.1.0-alpha.1",
        summary: "New: a fast native window on Docker Desktop, OrbStack or Colima, and Podman \
                  through DOCKER_HOST.",
        points: &[
            (
                "Containers by Compose project",
                "Status, ports and live CPU and memory, with start, stop, restart and remove.",
            ),
            (
                "Logs and a shell",
                "Follow logs live with search, and open a shell in the container in \
                 FerrumTerm.",
            ),
            (
                "Images, volumes and networks",
                "Pull with progress, remove, and prune unused data with the space you get back.",
            ),
            (
                "Compose in one click",
                "Bring a whole project up, down or restart it.",
            ),
        ],
        demo: feature_demos::dock,
        demo_label: "FerrumDock listing a Compose project's running containers with live CPU \
                     bars and their logs",
    },
];

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(
        Some("Features"),
        "What every Ferrum app can do in the current alpha: Ferrum Editor, FerrumAPI, \
         FerrumGIT, FerrumData, FerrumTerm, FerrumKit and FerrumDock.",
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
                    "Seven native developer apps, written in Rust and rendered with GPUI. This \
                     is what each one can do in the current alpha. "
                    a href=(site::DOWNLOAD_PATH) { "Ferrum Launcher" }
                    " installs them and keeps them up to date."
                }
                nav aria-label="Apps on this page" {
                    ul class="feature-index" {
                        @for app in &APPS {
                            li {
                                a href={ "#" (app.product.slug) } style=(app.product.accent_style()) {
                                    (app.product.name)
                                }
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
                        h2 id=(heading) { (p.name) }
                        p class="feature-app__meta" { (p.kind) " · v" (app.version) }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered() -> String {
        render("https://example.test").into_string()
    }

    #[test]
    fn covers_every_app_but_the_launcher_without_logos() {
        let html = rendered();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(!html.contains(r#"id="ferrum-launcher""#));
        assert!(!html.contains("app-logo"));
        assert!(html.contains(r#"href="/download""#));
        for p in products::ALL
            .iter()
            .filter(|p| p.slug != products::FERRUM_LAUNCHER.slug)
        {
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
        assert!(html.contains(crate::site::AMYGDAL_URL));
        assert!(html.contains("for the Rust community"));
    }
}
