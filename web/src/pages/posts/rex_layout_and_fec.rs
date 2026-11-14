//! "A calmer window in every app, agent logos and `fec`" (2026-11-14).
//! Claims follow the apps' code: `shared/ui/src/window_chrome.rs`,
//! `shared/ui/src/agent_logo.rs` and `dock/apps/cli`.

use maud::{Markup, html};

use super::product_row;
use crate::products::{
    FERRUM_EDITOR, FERRUM_LAUNCHER, FERRUMAPI, FERRUMCONTAINERS, FERRUMDATA, FERRUMENV, FERRUMGIT,
    FERRUMKIT, FERRUMPORTS, FERRUMTERM, FERRUMVIBE, Product,
};
use crate::updates::Post;

pub const POST: Post = Post {
    slug: "calmer-windows-agent-logos-and-fec",
    title: "Calmer windows, agent logos and the fec command",
    date: "2026-11-14",
    byline: "Ferrum Team",
    summary: "Every app gets a quiet sidebar, one slim row of actions and content in cards. \
              FerrumTerm shows agent logos; fec starts and stops the containers VM.",
    body,
};

/// Today's releases: (product, version, one line).
const RELEASES: [(Product, &str, &str); 11] = [
    (
        FERRUMTERM,
        "0.9.0-alpha.1",
        "Tabs in a calm sidebar, agent logos, more tab icons.",
    ),
    (
        FERRUMCONTAINERS,
        "0.3.0-alpha.1",
        "fec on the command line; the VM stops when idle.",
    ),
    (
        FERRUMGIT,
        "0.5.0-alpha.1",
        "One row of actions over list and detail cards.",
    ),
    (
        FERRUM_EDITOR,
        "0.7.0-alpha.1",
        "File tree on the backdrop, tabs in the editor card.",
    ),
    (
        FERRUMVIBE,
        "0.3.0-alpha.1",
        "One title row; the model's logo in the chat.",
    ),
    (
        FERRUMAPI,
        "0.5.0-alpha.1",
        "Request tabs and actions in the card header.",
    ),
    (
        FERRUMDATA,
        "0.5.0-alpha.1",
        "SQL tabs and Run in the editor card header.",
    ),
    (
        FERRUMPORTS,
        "0.2.0-alpha.1",
        "Search and filters in the title row.",
    ),
    (
        FERRUMENV,
        "0.2.0-alpha.1",
        "Environment tabs and actions in one header.",
    ),
    (
        FERRUMKIT,
        "0.2.0-alpha.1",
        "Tool list in the sidebar, the tool in a card.",
    ),
    (
        FERRUM_LAUNCHER,
        "0.11.0",
        "Simpler Settings; the fec command runs the new CLI.",
    ),
];

fn body() -> Markup {
    html! {
        p class="post__intro" {
            "The native title bar is gone from every Ferrum app. The traffic lights sit over \
             the window, and the space they leave holds the controls you reach for most."
        }
        ul class="post-apps" {
            @for (p, version, text) in RELEASES {
                (product_row(&p, &format!("{version}. {text}")))
            }
        }

        h2 id="layout" { "One layout everywhere" }
        ul class="post__list" {
            li {
                strong { "Sidebar on the backdrop" } ": no frame around it. Its top row has "
                strong { "◧" } " to hide it, " strong { "⌘" } " for the command palette or project \
                 switcher, and " strong { "+" } " for the app's main new thing."
            }
            li {
                strong { "One row of actions" } ": FerrumGIT's Fetch, Pull, Push and the rest, \
                 FerrumAPI's environment and import, FerrumData's Run, as small icons with \
                 tooltips in a single row."
            }
            li {
                strong { "Cards for content" } ": the card you used last is a little lighter, \
                 and so are the active sidebar row and tab."
            }
            li {
                strong { "No path strip" } ": the project name and branch stay, under the \
                 sidebar's top row."
            }
        }

        h2 id="term" { "FerrumTerm" }
        p {
            "With tabs in the sidebar, the top of the terminal shows only the current tab. \
             Agents and SSH hosts share the bottom 30% of the sidebar (drag the line to resize). \
             Tabs running Claude Code, Codex, Gemini or OpenCode show that agent's logo; other \
             tabs show a plain " code { ">_" } " tile. Pick from 20 icons and 9 colours for any \
             tab. FerrumVibe's chat shows the logo of the model you picked."
        }

        h2 id="fec" { "fec: FerrumContainers in the terminal" }
        p {
            "Turn on " code { "fec" } " in Ferrum Launcher and it works like Colima and Docker \
             in one:"
        }
        ul class="post__list" {
            li { code { "fec start" } " / " code { "fec stop" } ": start or stop the VM: Colima, OrbStack, Docker Desktop or Podman." }
            li { code { "fec status" } ": which VM, whether it runs, the running containers." }
            li { code { "fec up" } " / " code { "fec down" } ": " code { "docker compose up -d" } " (starting the VM first) and " code { "down" } "." }
            li { code { "fec ps" } ", " code { "fec logs -f web" } ", " code { "fec exec -it web sh" } ": anything else runs " code { "docker" } " against the VM." }
            li { code { "fec ." } ": open FerrumContainers on this folder." }
        }
        p {
            "FerrumContainers also stops the VM after 30 minutes with no running container, to \
             free its memory. Change or turn that off in Settings › Engine, or with "
            code { "fec autostop" } ". The same page starts and stops each VM."
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
        assert!(html.contains("fec start"));
    }
}
