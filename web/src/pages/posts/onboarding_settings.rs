//! "Onboarding and Settings in every Ferrum app" (2026-11-13). Claims follow
//! the `ferrum-ui` section of the root README and the FerrumTerm README.

use maud::{Markup, html};

use super::product_row;
use crate::products::{
    FERRUM_LAUNCHER, FERRUMAPI, FERRUMCONTAINERS, FERRUMDATA, FERRUMENGINEER, FERRUMGIT, FERRUMIDE,
    FERRUMKIT, FERRUMPORTS, FERRUMTERM, FERRUMVARIABLES, Product,
};
use crate::updates::Post;

pub const POST: Post = Post {
    slug: "onboarding-and-settings",
    title: "Onboarding and Settings in every Ferrum app",
    date: "2026-11-13",
    byline: "Ferrum Team",
    summary: "Every app now opens with a short walkthrough (look, permissions, agent skill, \
              config migration, keys to know) and shares one Settings window. FerrumTerm gets \
              Terminal and Remote Hosts settings.",
    body,
};

/// Today's releases: (product, version, one line).
const RELEASES: [(Product, &str, &str); 11] = [
    (
        FERRUMTERM,
        "0.5.0-alpha.1",
        "Settings for font, cursor, keyboard, selection, shell and SSH hosts.",
    ),
    (
        FERRUMIDE,
        "0.5.2-alpha.1",
        "Onboarding and the shared Settings window.",
    ),
    (
        FERRUMENGINEER,
        "0.1.2-alpha.1",
        "Onboarding and the shared Settings window.",
    ),
    (
        FERRUMGIT,
        "0.4.2-alpha.1",
        "Onboarding and the shared Settings window.",
    ),
    (
        FERRUMAPI,
        "0.4.3-alpha.1",
        "Onboarding and the shared Settings window.",
    ),
    (
        FERRUMDATA,
        "0.4.5-alpha.1",
        "Onboarding and the shared Settings window.",
    ),
    (
        FERRUMCONTAINERS,
        "0.2.4-alpha.1",
        "Onboarding and the shared Settings window.",
    ),
    (
        FERRUMPORTS,
        "0.1.3-alpha.1",
        "Onboarding and the shared Settings window.",
    ),
    (
        FERRUMVARIABLES,
        "0.1.4-alpha.1",
        "Onboarding and the shared Settings window.",
    ),
    (
        FERRUMKIT,
        "0.1.5-alpha.1",
        "Onboarding and the shared Settings window.",
    ),
    (
        FERRUM_LAUNCHER,
        "0.9.2",
        "Onboarding and the shared Settings window.",
    ),
];

fn body() -> Markup {
    html! {
        p class="post__intro" {
            "The first time you open a Ferrum app it now walks you through setting it up, and "
            kbd { "⌘," } " opens one Settings window that looks the same in every app."
        }
        ul class="post-apps" {
            @for (p, version, text) in RELEASES {
                (product_row(&p, &format!("{version}. {text}")))
            }
        }

        h2 id="onboarding" { "A walkthrough on first launch" }
        ul class="post__list" {
            li {
                strong { "Look and feel. " }
                "Pick Auto, Light or Dark, a comfortable or compact toolbar and one of 14 built-in \
                 themes. The choice is shared: every open Ferrum app changes with it."
            }
            li {
                strong { "Integrations and permissions. " }
                "Install the Ferrum skill so Claude Code, Codex and OpenCode know how to open and \
                 drive the apps, and open the right System Settings pane for each permission the \
                 app needs."
            }
            li {
                strong { "Migrate your config. " }
                "Choose a coding agent installed on your Mac and it opens in a terminal with a \
                 task written for that app: where iTerm2, Ghostty, VS Code, Postman, TablePlus \
                 and others keep their settings, and where Ferrum keeps its own. It asks before \
                 it changes anything. Or copy the prompt and use it yourself."
            }
            li {
                strong { "Keys worth knowing. " }
                "The last step lists the app's most useful shortcuts."
            }
        }
        p {
            "Skip it any time; it comes back from the app menu (Show Onboarding…) or "
            "Settings › Interface."
        }

        h2 id="settings" { "One Settings window" }
        ul class="post__list" {
            li { strong { "Interface" } ": colour scheme, toolbar density, text size and the Dock icon." }
            li { strong { "Themes" } ": the built-in themes for light and dark mode." }
            li {
                strong { "Shortcuts" } ": every key binding, filterable by name or key. Record \
                 a new one; it wins over the built-in key."
            }
            li { strong { "Integrations" } ": the agent skill, the command-line tool, permissions and migration." }
            li {
                strong { "Notifications" } ": where toasts appear, system notifications, Dock \
                 badge and sound, switchable per event."
            }
        }

        h2 id="ferrumterm" { "FerrumTerm 0.5.0-alpha.1" }
        ul class="post__list" {
            li {
                strong { "Terminal" } ": font and size, cursor style and blink, Option as Meta, \
                 copy on selection, activity in tabs and a startup command. They apply to the \
                 terminal panes in Ferrum Editor and FerrumVibe too."
            }
            li {
                strong { "Remote Hosts" } ": add and remove saved SSH hosts, see the hosts from "
                code { "~/.ssh/config" } " and how many keys your SSH agent holds."
            }
            li {
                strong { "Notifications" } ": choose how Working, Blocked, Error and Done reach \
                 you. The server's system notifications follow the same switches."
            }
        }
        p { a href=(FERRUMTERM.path()) { "More about FerrumTerm" } }

        h2 id="not-built-yet" { "Not built yet" }
        ul class="post__list" {
            li {
                "Ferrum Editor keeps its own themes: the colour scheme switches between its \
                 light and dark built-ins, and a theme picked in the editor stays."
            }
            li { "Resetting a recorded shortcut takes effect after a restart." }
            li { "The toolbar can't move to the side of the window yet." }
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
        assert!(html.contains(">Not built yet</h2>"));
    }
}
