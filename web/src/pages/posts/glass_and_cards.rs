//! "Glass, gradients and pane cards" (2026-11-14). Claims follow the
//! `ferrum-ui` section of the root README.

use maud::{Markup, html};

use super::product_row;
use crate::products::{
    FERRUM_EDITOR, FERRUM_LAUNCHER, FERRUMAGENTIC, FERRUMAPI, FERRUMCONTAINERS, FERRUMDATA,
    FERRUMENV, FERRUMGIT, FERRUMKIT, FERRUMPORTS, FERRUMTERM, Product,
};
use crate::updates::Post;

pub const POST: Post = Post {
    slug: "glass-gradients-and-pane-cards",
    title: "Glass, gradients and pane cards",
    date: "2026-11-14",
    byline: "Ferrum Team",
    summary: "A new Interface style setting (Solid, Gradient or Glass), pane cards in Ferrum \
              Editor, FerrumAgentic and FerrumTerm, a cleaner onboarding, and commit messages \
              that fold to one line in FerrumGIT.",
    body,
};

/// Today's releases: (product, version, one line).
const RELEASES: [(Product, &str, &str); 11] = [
    (
        FERRUM_EDITOR,
        "0.6.0-alpha.1",
        "Pane cards; follows the suite's built-in themes.",
    ),
    (
        FERRUMAGENTIC,
        "0.2.0-alpha.1",
        "Chat, Git and Term columns as pane cards.",
    ),
    (
        FERRUMTERM,
        "0.6.0-alpha.1",
        "Terminal and sidebar as cards, pill tabs.",
    ),
    (
        FERRUMGIT,
        "0.4.3-alpha.1",
        "Commit messages fold to their first line.",
    ),
    (
        FERRUMAPI,
        "0.4.4-alpha.1",
        "Interface styles and the new onboarding.",
    ),
    (
        FERRUMDATA,
        "0.4.6-alpha.1",
        "Interface styles and the new onboarding.",
    ),
    (
        FERRUMCONTAINERS,
        "0.2.5-alpha.1",
        "Interface styles and the new onboarding.",
    ),
    (
        FERRUMPORTS,
        "0.1.4-alpha.1",
        "Interface styles and the new onboarding.",
    ),
    (
        FERRUMENV,
        "0.1.5-alpha.1",
        "Interface styles and the new onboarding.",
    ),
    (
        FERRUMKIT,
        "0.1.6-alpha.1",
        "Interface styles and the new onboarding.",
    ),
    (
        FERRUM_LAUNCHER,
        "0.9.3",
        "Interface styles and the new onboarding.",
    ),
];

fn body() -> Markup {
    html! {
        p class="post__intro" {
            "Settings › Interface has a new " strong { "Interface style" } ": Solid, Gradient \
             or Glass. It applies to every Ferrum app at once, with any of the built-in themes."
        }
        ul class="post-apps" {
            @for (p, version, text) in RELEASES {
                (product_row(&p, &format!("{version}. {text}")))
            }
        }

        h2 id="styles" { "Solid, Gradient or Glass" }
        ul class="post__list" {
            li { strong { "Solid" } ": flat theme colours, as before. Still the default." }
            li { strong { "Gradient" } ": translucent panes on a colour wash taken from the theme's accent." }
            li { strong { "Glass" } ": translucent panes over the desktop, blurred by macOS." }
        }

        h2 id="cards" { "Pane cards" }
        p {
            "Ferrum Editor (the file tree, the editor and the Git, Agent, Term, CI and Logs \
             panes), FerrumAgentic's three columns and FerrumTerm's terminal and sidebar are \
             now rounded cards with gaps between them. Drag a gap to resize. FerrumTerm's tabs \
             are pills. Ferrum Editor now follows the built-in theme you pick for the suite, \
             unless you chose one of its own themes."
        }

        h2 id="onboarding" { "A cleaner onboarding" }
        ul class="post__list" {
            li { "Solid accent buttons, a step indicator at the bottom, and the interface style on the look step." }
            li {
                "The Notifications permission now asks macOS directly, so the app shows up under \
                 System Settings › Notifications with its own name."
            }
        }

        h2 id="ferrumgit" { "FerrumGIT" }
        p {
            "Commit details show only the first line of the message; Show more opens the rest, \
             and the choice stays while you move between commits."
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
