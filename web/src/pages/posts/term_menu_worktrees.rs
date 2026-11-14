//! "FerrumTerm 0.7: terminals, cloud agents and worktrees from one menu"
//! (2026-11-14). Claims follow the FerrumTerm README and the root README.

use maud::{Markup, html};

use super::product_row;
use crate::products::{
    FERRUM_EDITOR, FERRUM_LAUNCHER, FERRUMAPI, FERRUMCONTAINERS, FERRUMDATA, FERRUMENV, FERRUMGIT,
    FERRUMKIT, FERRUMPORTS, FERRUMTERM, FERRUMVIBE, Product,
};
use crate::updates::Post;

pub const POST: Post = Post {
    slug: "ferrumterm-0-7-menu-and-worktrees",
    title: "FerrumTerm 0.7: agents, worktrees and a new + menu",
    date: "2026-11-14",
    byline: "Ferrum Team",
    summary: "FerrumTerm's + menu starts terminals, agents, agents on SSH hosts, git worktrees \
              and saved tab configs. Every app gets a full menu bar, and Ferrum Launcher can \
              update all apps at once.",
    body,
};

/// Today's releases: (product, version, one line).
const RELEASES: [(Product, &str, &str); 11] = [
    (
        FERRUMTERM,
        "0.7.0-alpha.1",
        "New + menu, agents section, Restart server for old servers.",
    ),
    (
        FERRUM_LAUNCHER,
        "0.10.0",
        "Update All, and the suite's look one click away.",
    ),
    (
        FERRUM_EDITOR,
        "0.6.1-alpha.1",
        "Shared App, Window and Help menus; terminal fixes.",
    ),
    (
        FERRUMVIBE,
        "0.2.1-alpha.1",
        "Full menu bar; terminal fixes.",
    ),
    (
        FERRUMGIT,
        "0.4.4-alpha.1",
        "Full menu bar and the new onboarding.",
    ),
    (
        FERRUMAPI,
        "0.4.5-alpha.1",
        "Full menu bar and the new onboarding.",
    ),
    (
        FERRUMDATA,
        "0.4.7-alpha.1",
        "Full menu bar and the new onboarding.",
    ),
    (
        FERRUMCONTAINERS,
        "0.2.6-alpha.1",
        "Full menu bar and the new onboarding.",
    ),
    (
        FERRUMPORTS,
        "0.1.5-alpha.1",
        "Full menu bar and the new onboarding.",
    ),
    (
        FERRUMENV,
        "0.1.6-alpha.1",
        "Full menu bar and the new onboarding.",
    ),
    (
        FERRUMKIT,
        "0.1.7-alpha.1",
        "Full menu bar and the new onboarding.",
    ),
];

fn body() -> Markup {
    html! {
        p class="post__intro" {
            "FerrumTerm's " strong { "+" } " menu now starts everything a session needs, and the \
             sidebar keeps terminals and agents apart."
        }
        ul class="post-apps" {
            @for (p, version, text) in RELEASES {
                (product_row(&p, &format!("{version}. {text}")))
            }
        }

        h2 id="menu" { "One + menu" }
        ul class="post__list" {
            li { strong { "Terminal" } ": a new tab with your shell." }
            li { strong { "Agent" } ": Claude Code, Codex, a recent command or any custom command." }
            li {
                strong { "Cloud agent" } ": Claude Code or Codex on one of your SSH hosts, \
                 through the host's login shell."
            }
            li {
                strong { "New worktree…" } ": type a branch; FerrumTerm creates "
                code { "<repo>-<branch>" } " next to the repository and opens a terminal in it."
            }
            li {
                strong { "New tab config…" } ": save " code { "name = command" } " once and \
                 start it from the menu as \"New tab: name\"."
            }
        }

        h2 id="sidebar" { "Terminals and agents" }
        p {
            "Sessions, tabs and SSH hosts are at the top of the sidebar. Agents have their own \
             scrolling section at the bottom (30% by default); drag the line above it to resize."
        }

        h2 id="server" { "Old servers" }
        p {
            "If a FerrumTerm server from an older build is still running, the window now says so \
             and offers Restart server instead of retrying forever. "
            code { "fterm server stop" } " stops such a server too."
        }

        h2 id="menus" { "A full menu bar in every app" }
        p {
            "About, Check for Updates…, Settings…, Hide and Quit, plus File, Edit, View, Window \
             and Help menus with the usual shortcuts. Ferrum Launcher gets " strong { "Update All" }
            " and a button for the suite's colour scheme, themes and interface style. Onboarding \
             makes the window larger while it runs and keeps Continue at the bottom."
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
