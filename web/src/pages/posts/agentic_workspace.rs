//! "FerrumVibe becomes an agent workspace" (2026-11-14). Claims follow
//! `vibe/apps/desktop`, `agent/crates/ui` and `launcher/apps/desktop/src/panel`.

use maud::{Markup, html};

use super::product_row;
use crate::products::{
    FERRUM_LAUNCHER, FERRUMAPI, FERRUMCONTAINERS, FERRUMDATA, FERRUMENGINEER, FERRUMGIT, FERRUMIDE,
    FERRUMKIT, FERRUMPORTS, FERRUMTERM, FERRUMVARIABLES, Product,
};
use crate::updates::Post;

pub const POST: Post = Post {
    slug: "ferrumagentic-agent-workspace",
    title: "FerrumVibe becomes an agent workspace",
    date: "2026-11-14",
    byline: "Ferrum Team",
    summary: "Tasks, projects, discussions and automations in one sidebar, a new composer, \
              a Toolbox-style launcher panel and the same card layout in every app.",
    body,
};

/// Today's releases: (product, version, one line).
const RELEASES: [(Product, &str, &str); 11] = [
    (
        FERRUMENGINEER,
        "0.4.0-alpha.1",
        "Tasks, folders, knowledge, sites, automations and extensions.",
    ),
    (
        FERRUM_LAUNCHER,
        "0.12.0",
        "A panel under the menu bar icon; hide from the Dock.",
    ),
    (
        FERRUMIDE,
        "0.8.0-alpha.1",
        "The new chat composer; the bottom terminal fills its panel.",
    ),
    (
        FERRUMGIT,
        "0.6.0-alpha.1",
        "List and detail cards with their own headers.",
    ),
    (
        FERRUMCONTAINERS,
        "0.4.0-alpha.1",
        "Engine, filter and actions in the card header.",
    ),
    (
        FERRUMPORTS,
        "0.3.0-alpha.1",
        "Search and filters in the table's header.",
    ),
    (
        FERRUMTERM,
        "0.9.2-alpha.1",
        "Title bar setting; works inside the new FerrumVibe.",
    ),
    (FERRUMAPI, "0.5.1-alpha.1", "Title bar setting."),
    (FERRUMDATA, "0.5.1-alpha.1", "Title bar setting."),
    (FERRUMVARIABLES, "0.2.1-alpha.1", "Title bar setting."),
    (FERRUMKIT, "0.2.1-alpha.1", "Title bar setting."),
];

fn body() -> Markup {
    html! {
        p class="post__intro" {
            "FerrumVibe now works like a desktop agent app: pick a folder, start a task, \
             and the agent edits that code while you watch the conversation."
        }
        ul class="post-apps" {
            @for (p, version, text) in RELEASES {
                (product_row(&p, &format!("{version}. {text}")))
            }
        }

        h2 id="sidebar" { "The sidebar" }
        ul class="post__list" {
            li { strong { "New Task" } ", " strong { "Projects" } ", " strong { "Discussion" } " (conversations that don't edit code) and " strong { "Search" } " across every project's threads." }
            li { strong { "Folders" } ": your projects, each with its tasks; " strong { "Recent conversations" } " below them." }
            li { strong { "Knowledge Center" } ": the rule files the agent reads (AGENTS.md, CLAUDE.md, .ferrum/rules), with a button for new rules." }
            li { strong { "Sites" } ": saved web pages to add to a chat as context." }
            li { strong { "Automations" } ": saved prompts that run on demand, every few hours or daily, while the app is open." }
            li { strong { "Extensions" } ": your MCP servers and their status. Usage and settings sit at the bottom." }
        }

        h2 id="composer" { "The composer" }
        p {
            code { "+" } " adds files, folders, the git diff, terminal output or a web page; the \
             paperclip attaches files. Choose the mode (Agent, Ask, Plan) and how much the agent \
             may do on its own: Ask for approval, Auto-accept edits or Full access. Under it: \
             the folder, the branch with Push, Pull and Commit, and how full the context is. \
             Git and Terminal open beside the chat from the top right."
        }

        h2 id="launcher" { "Ferrum Launcher in the menu bar" }
        p {
            "Click the menu bar icon for a panel with every app, its version and an Update or \
             Install button, download progress with Cancel, your recent projects and search. \
             Settings can now hide the launcher from the Dock."
        }

        h2 id="layout" { "One layout everywhere" }
        p {
            "FerrumGIT, FerrumContainers and FerrumPorts lose their separate toolbar rows: \
             content starts at the top in cards, and each card's header holds its title and \
             actions, like FerrumTerm. Every app's Settings › Interface has a Title bar choice: \
             transparent or tinted."
        }
        p {
            "Every release is also on the " a href=(crate::pages::changelog::PATH) { "changelog" } "."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_todays_releases_with_links() {
        let html = body().into_string();
        for (p, version, _) in RELEASES {
            assert!(
                html.contains(&format!(r#"href="{}""#, p.path())),
                "{}",
                p.name
            );
            assert!(html.contains(version), "{version}");
        }
    }
}
