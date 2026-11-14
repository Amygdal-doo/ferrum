//! "Ferrum System, and FerrumAgentic is now FerrumVibe" (2026-11-15).
//! Claims follow `system/`, `vibe/apps/desktop`, `launcher/crates/core`
//! (`retire_agentic`, the shell commands), `term/crates/ui/src/git_pane.rs`
//! and `shared/ui/src/window_chrome.rs` (`settings_button`).

use maud::{Markup, html};

use super::product_row;
use crate::products::{
    FERRUM_EDITOR, FERRUM_LAUNCHER, FERRUM_SYSTEM, FERRUMAPI, FERRUMCONTAINERS, FERRUMDATA,
    FERRUMENV, FERRUMGIT, FERRUMKIT, FERRUMPORTS, FERRUMTERM, FERRUMVIBE, Product,
};
use crate::updates::Post;

pub const POST: Post = Post {
    slug: "ferrum-system-and-ferrumvibe",
    title: "Ferrum System, and FerrumAgentic is now FerrumVibe",
    date: "2026-11-15",
    byline: "Ferrum Team",
    summary: "A new app for designing a system's architecture next to its code, FerrumAgentic \
              renamed FerrumVibe, FerrumGIT inside FerrumTerm, and a Settings gear in every app.",
    body,
};

/// The releases with these changes: (product, version, one line).
const RELEASES: [(Product, &str, &str); 12] = [
    (
        FERRUM_SYSTEM,
        "0.1.0-alpha.1",
        "New: system architecture diagrams.",
    ),
    (
        FERRUMVIBE,
        "0.5.0-alpha.1",
        "FerrumAgentic's new name, FeV, and fev.",
    ),
    (
        FERRUMTERM,
        "0.10.0-alpha.1",
        "FerrumGIT and code changes beside the terminal.",
    ),
    (
        FERRUM_LAUNCHER,
        "0.13.0",
        "Ferrum System; moves FerrumAgentic to FerrumVibe.",
    ),
    (
        FERRUM_EDITOR,
        "0.8.2-alpha.1",
        "Settings gear; new agent logos.",
    ),
    (FERRUMGIT, "0.6.2-alpha.1", "Settings gear."),
    (FERRUMAPI, "0.5.3-alpha.1", "Settings gear."),
    (FERRUMDATA, "0.5.3-alpha.1", "Settings gear."),
    (FERRUMKIT, "0.2.3-alpha.1", "Settings gear."),
    (FERRUMCONTAINERS, "0.4.2-alpha.1", "Settings gear."),
    (FERRUMPORTS, "0.3.2-alpha.1", "Settings gear."),
    (
        FERRUMENV,
        "0.2.3-alpha.1",
        "Settings gear; its command is now fen.",
    ),
];

fn body() -> Markup {
    html! {
        p class="post__intro" {
            "A twelfth app, a new name, and FerrumGIT where you run your agents."
        }
        ul class="post-apps" {
            @for (p, version, text) in RELEASES {
                (product_row(&p, &format!("{version}. {text}")))
            }
        }

        h2 id="system" { "Ferrum System" }
        p {
            "Ferrum System designs a system's architecture and keeps it in the project, in an "
            code { ".architecture" } " folder. Components sit on layers, from clients down to \
            data and the platform, and connect as a request, internal RPC, an async message, \
            a socket or change data capture. One switch shows the service names for AWS, \
            Azure, GCP, Hetzner, DigitalOcean or open source."
        }
        p {
            "Double-click a database for its tables. Walk through a flow one hop at a time. \
             The panel on the right explains the design: overview and requirements, APIs, \
             sockets, data, flows, reliability, and where each component runs. The Runtime \
             tab asks kubectl and docker what is running and puts the pods next to their \
             components."
        }
        p {
            "Drag components between layers and layers into a new order, connect two \
             components by dragging from a side of the selected one, and right-click a \
             connection to change its sides, kind, colour or width. Every card has its own \
             colour, borders, radius, texts and items inside. Three demos show the idea: core \
             banking on microservices, a private RAG on Ollama with Claude Code, and live \
             delivery tracking. Open it with " code { "fes" } "."
        }

        h2 id="vibe" { "FerrumAgentic is now FerrumVibe" }
        p {
            "Same app, new name, FeV logo, and the " code { "ferrumvibe" } " command. Its \
             shell command is now " code { "fev" } ", so FerrumEnv's becomes "
            code { "fen" } ". Ferrum Launcher 0.13 removes the old FerrumAgentic install and \
             offers FerrumVibe to install; your threads, layouts and settings move over the \
             first time it starts."
        }

        h2 id="term" { "FerrumGIT in FerrumTerm" }
        p {
            "Two buttons in FerrumTerm's tab row open FerrumGIT beside the terminal: one on \
             the history, one on the uncommitted changes and their diffs. It follows the \
             focused tab's folder as you cd around, and opens files in Ferrum Editor."
        }

        h2 id="details" { "Smaller things" }
        ul {
            li { "Every app has a Settings gear next to the sidebar button." }
            li { "New agent logos: Claude, OpenAI, Gemini, OpenCode, Muse, DeepSeek and Kimi." }
            li { "Ferrum Launcher's menu no longer puts the cursor in its search field until you click it." }
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
    fn lists_the_releases_with_links() {
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
