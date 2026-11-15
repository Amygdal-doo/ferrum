//! Features page: what each app does in the current alpha, one section per
//! app with an animated mock window (see `feature_demos`). Claims follow the
//! release notes of the versions listed here.

use maud::{Markup, html};

use super::{feature_demos, project_demos, tool_demos};
use crate::components::{breadcrumb::Crumb, page_header::PageHeader};
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

const APPS: [AppFeatures; 11] = [
    AppFeatures {
        product: products::FERRUMIDE,
        version: "0.5.0-alpha.1",
        summary: "The suite's code editor: small, hackable and in the spirit of Atom, written \
                  from scratch in Rust. New: Git, Agent and Term open inside the window.",
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
                "Git, Agent and Term panes",
                "The buttons at the top right open FerrumGIT, FerrumChat and FerrumTerm as \
                 split panes, all at once if you like. Move them left, right or bottom from \
                 the ⋯ menu or by dragging; the layout is saved per project.",
            ),
            (
                "Builds in the Term pane",
                "⌘B builds with cargo, go or npm in a Term pane tab and lists the errors as \
                 problems.",
            ),
            (
                "Logs page",
                "Tail a project's log files, dev servers and Docker containers in one live, \
                 filtered view, with levels detected and JSON lines made readable.",
            ),
            (
                "CI page",
                "GitHub Actions for the open project: workflow runs, live job logs in colour, \
                 a form for workflow_dispatch and pull request checks.",
            ),
            (
                "Themes and file icons",
                "Pick a theme with live preview, write your own in TOML or import a VS Code \
                 theme, and load your own file icon sets.",
            ),
        ],
        demo: feature_demos::editor,
        demo_label: "FerrumIDE with a tree view and tabs: three cursors type at once, then \
                     the command palette filters commands on toggle",
    },
    AppFeatures {
        product: products::FERRUMENGINEER,
        version: "0.1.0-alpha.1",
        summary: "New: FerrumChat, FerrumGIT and FerrumTerm side by side on one project, in one \
                  window for working with coding agents.",
        points: &[
            (
                "Three columns",
                "Close any of them and bring it back with ⌥⌘1, ⌥⌘2 or ⌥⌘3, drag to resize or \
                 reorder, and get the same layout next time you open the project.",
            ),
            (
                "Any model or agent",
                "Anthropic, OpenAI or Ollama models, or Claude Code, Codex and Gemini CLI over \
                 ACP, in Ask, Agent or Plan mode, with MCP tools and your rules files.",
            ),
            (
                "Review and checkpoints",
                "Accept or reject agent edits per hunk or per file, and restore the files to \
                 how they were before any turn.",
            ),
            (
                "Chat, Git and Term together",
                "Run in terminal runs the chat's command in the terminal column, and agent \
                 edits show up in the git column right away.",
            ),
        ],
        demo: tool_demos::vibe,
        demo_label: "FerrumEngineer with three columns: the chat edits a file, the git column \
                     shows the change, and the terminal column runs the tests",
    },
    AppFeatures {
        product: products::FERRUMAPI,
        version: "0.4.0-alpha.1",
        summary: "Six request kinds join REST, mixed freely in one collection, and collections \
                  can now live in your repo.",
        points: &[
            (
                "GraphQL",
                "Query, variables and operation name over POST or GET, a schema panel from \
                 introspection and field suggestions as you type.",
            ),
            (
                "SOAP and OData",
                "Load a WSDL, pick an operation and get a sample envelope; XML responses are \
                 coloured. Build OData queries and pick entity sets from $metadata.",
            ),
            (
                "tRPC, WebSocket and gRPC",
                "tRPC queries, mutations and batches with a result.data view. WebSocket \
                 messages with templates and a live log. Unary and server-streaming gRPC from \
                 reflection or a .proto file.",
            ),
            (
                "Collections in the repo",
                "Collections belong to a project. Save to repo writes them to .ferrum/api/ and \
                 Load reads them back. Webhook captures can be saved to a collection.",
            ),
        ],
        demo: feature_demos::api,
        demo_label: "FerrumAPI showing a coloured JSON response with search matches and a \
                     collection run passing REST, GraphQL, SOAP and tRPC requests one by one",
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
        product: products::FERRUMCONTAINERS,
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
        demo_label: "FerrumContainers listing a Compose project's running containers with live CPU \
                     bars and their logs",
    },
    AppFeatures {
        product: products::FERRUM_SYSTEM,
        version: "0.1.0-alpha.1",
        summary: "New: design a system's architecture next to its code, on any cloud.",
        points: &[
            (
                "Layers and connections",
                "Components on layers, connected as requests, RPC, async messages, sockets or \
                 change data capture, dragged into place.",
            ),
            (
                "Every cloud's names",
                "Generic, AWS, Azure, GCP, Hetzner, DigitalOcean or open source, one switch.",
            ),
            (
                "Data and flows",
                "Double-click a database for its tables; walk a request through, hop by hop.",
            ),
            (
                "What runs it",
                "Pods, containers, VMs or managed services, with live status from kubectl and \
                 docker.",
            ),
        ],
        demo: project_demos::system,
        demo_label: "Ferrum System with the core banking demo on AWS: layers of components, \
                     the ledger database picked and its schema opening.",
    },
    AppFeatures {
        product: products::FERRUMPORTS,
        version: "0.1.0-alpha.1",
        summary: "New: which process is on port 3000, which project it belongs to, and a \
                  confirmed way to stop it.",
        points: &[
            (
                "Ports by project",
                "TCP listeners, and UDP on request, with process, command and user, grouped \
                 by the git project they run in.",
            ),
            (
                "Stop a process",
                "SIGTERM after a confirmation that shows the full command, then SIGKILL if it \
                 is still running 2 s later.",
            ),
            (
                "Is port N free?",
                "Check a port before you use it, and get the nearest free one if it's taken.",
            ),
            (
                "Search and hints",
                "Refreshes every 2 s, searches every column, and names about 45 well-known \
                 ports.",
            ),
        ],
        demo: project_demos::ports,
        demo_label: "FerrumPorts listing a project's listening ports, then stopping a dev \
                     server after a confirmation",
    },
    AppFeatures {
        product: products::FERRUMVARIABLES,
        version: "0.1.0-alpha.1",
        summary: "New: your projects' environment variables in an encrypted vault, locked by a \
                  master password.",
        points: &[
            (
                "Locked by default",
                "Nothing is decrypted before you type the master password, and the vault locks \
                 itself after 30 minutes without activity.",
            ),
            (
                "Environments per project",
                "Development, staging and production to start, with secret values masked and \
                 the clipboard cleared after a copy.",
            ),
            (
                "Encrypted sharing",
                "Send a teammate an encrypted bundle sealed with the project's share key, and \
                 preview an import before it changes anything.",
            ),
            (
                ".env in and out",
                "Import .env files from the project folder, write them back, and compare two \
                 environments.",
            ),
        ],
        demo: project_demos::env,
        demo_label: "FerrumVariables showing a project's development variables with secrets masked, \
                     one value revealed and copied",
    },
];

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(
        Some("Features of every Ferrum app"),
        "What every Ferrum app can do in the current alpha: FerrumIDE, FerrumEngineer, \
         FerrumAPI, FerrumGIT, FerrumData, FerrumTerm, FerrumKit, FerrumContainers, FerrumPorts \
         and FerrumVariables.",
        PATH,
        "page-features",
    )
    .trail(vec![Crumb::new("Features", PATH)]);
    page(
        site_url,
        &meta,
        html! {
            (hero(&meta.trail))
            @for app in &APPS {
                (app_section(app))
            }
        },
    )
}

fn hero(trail: &[Crumb]) -> Markup {
    PageHeader::new(trail, "What's new across Ferrum")
        .lead(html! {
            p {
                "Eleven native developer apps, written in Rust and rendered with GPUI. This \
                 is what each one can do in the current alpha. "
                a href=(site::DOWNLOAD_PATH) { "Ferrum Launcher" }
                " installs them and keeps them up to date."
            }
            p {
                "Most of them share a project strip at the top: the project's name, path and \
                 branch, and buttons to open it in FerrumIDE, FerrumGIT or FerrumTerm. See "
                a href=(super::docs::project_bar::PATH) { "the project bar" } "."
            }
        })
        .actions(html! {
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
        })
        .render()
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
                    ul class="feature-points" tabindex="0" aria-label=(format!("{} features", p.name)) {
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
