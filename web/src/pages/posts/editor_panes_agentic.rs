//! "Ferrum Editor 0.5 and FerrumVibe" (2026-11-13): the releases of that
//! day. Claims follow the editor README ("Tool panes"), the FerrumVibe and
//! FerrumChat READMEs, `catalog.json` and the launcher's `shell.rs`.

use maud::{Markup, html};

use super::product_row;
use crate::pages::tool_demos;
use crate::products::{FERRUM_LAUNCHER, FERRUMENGINEER, FERRUMGIT, FERRUMIDE, FERRUMTERM};
use crate::updates::Post;

pub const POST: Post = Post {
    slug: "ferrum-editor-0-5-and-ferrumagentic",
    title: "Ferrum Editor 0.5 and FerrumVibe",
    date: "2026-11-13",
    byline: "Ferrum Team",
    summary: "Git, Agent and Term now open inside Ferrum Editor as split panes. FerrumVibe, \
              a new app, puts FerrumChat, FerrumGIT and FerrumTerm side by side on one project.",
    body,
};

/// Today's releases: (product, version, one line).
const RELEASES: [(crate::products::Product, &str, &str); 5] = [
    (
        FERRUMIDE,
        "0.5.0-alpha.1",
        "Git, Agent and Term open inside the editor as split panes.",
    ),
    (
        FERRUMENGINEER,
        "0.1.0-alpha.1",
        "New: FerrumChat, FerrumGIT and FerrumTerm in three columns on one project.",
    ),
    (
        FERRUMGIT,
        "0.4.0-alpha.1",
        "Built on the shared FerrumGIT view that the editor and FerrumVibe embed.",
    ),
    (
        FERRUMTERM,
        "0.4.0-alpha.1",
        "Built on the shared FerrumTerm view that the editor and FerrumVibe embed.",
    ),
    (
        FERRUM_LAUNCHER,
        "0.9.0",
        "Installs FerrumVibe and offers fex as its shell command.",
    ),
];

fn body() -> Markup {
    html! {
        p class="post__intro" {
            "Two releases change how the apps fit together. Ferrum Editor opens Git, the agent \
             chat and the terminal inside its window. FerrumVibe is a new app that puts the \
             same three tools side by side, without the editor."
        }
        ul class="post-apps" {
            @for (p, version, text) in RELEASES {
                (product_row(&p, &format!("{version}. {text}")))
            }
        }

        h2 id="ferrum-editor" { "Ferrum Editor 0.5.0-alpha.1" }
        ul class="post__list" {
            li {
                "The " strong { "Git" } ", " strong { "Agent" } " and " strong { "Term" }
                " buttons at the top right open FerrumGIT, FerrumChat and FerrumTerm inside the \
                 window as split panes. They replace the old terminal panel and Git view."
            }
            li {
                "All of them can be open at once, with the CI, Logs and Debug panes. Git and \
                 Agent start on the right and Term at the bottom."
            }
            li {
                "Move a pane to the left, right or bottom from its ⋯ menu or by dragging its \
                 header. The layout is saved per project."
            }
            li {
                "Shortcuts: " kbd { "⌥⌘G" } " Git, " kbd { "⌥⌘A" } " Agent, " kbd { "⌃`" }
                " Term. A closed pane keeps its view, so terminals keep running and the chat \
                 keeps its thread."
            }
        }
        p { a href={ (FERRUMIDE.path()) "#tools" } { "The tool panes on the Ferrum Editor page" } }

        h2 id="ferrumvibe" { "FerrumVibe 0.1.0-alpha.1" }
        figure class="post__figure" style=(FERRUMENGINEER.accent_style()) {
            div class="post__demo" role="img"
                aria-label="FerrumVibe with three columns: the chat edits a file, the git column shows the change, and the terminal column runs the tests" {
                (tool_demos::vibe())
            }
            figcaption {
                "FerrumVibe. An animated mock-up of the app, drawn in HTML for this page."
            }
        }
        ul class="post__list" {
            li {
                "Three columns on one project: FerrumChat, FerrumGIT and FerrumTerm. Close any \
                 of them and bring it back with " kbd { "⌥⌘1" } ", " kbd { "⌥⌘2" } " or "
                kbd { "⌥⌘3" } "; resize and reorder them. The layout is saved per project."
            }
            li {
                "The chat works with Anthropic, OpenAI and Ollama models, or with Claude Code, \
                 Codex and Gemini CLI over ACP, in Ask, Agent or Plan mode, with MCP tools and \
                 your rules files. API keys stay in the system keychain."
            }
            li {
                "Agent edits are reviewed per hunk, and each turn is a checkpoint. Run in \
                 terminal runs a command in the terminal column, and edits show up in the git \
                 column right away."
            }
            li {
                "Open it from a shell with " code { "fex" } " once you turn the command on in \
                 Ferrum Launcher. Builds are for macOS and Linux; there is no Windows build."
            }
        }
        p { a href=(FERRUMENGINEER.path()) { "More about FerrumVibe" } }

        h2 id="git-and-term" { "FerrumGIT and FerrumTerm 0.4.0-alpha.1" }
        p {
            "Both apps are now built on the same embeddable views that Ferrum Editor and \
             FerrumVibe show, so the interface is the same in all three places."
        }

        h2 id="launcher" { "Ferrum Launcher 0.9.0" }
        p {
            "The launcher knows FerrumVibe: it installs and updates it like the other apps, \
             and suggests " code { "fex" } " as its shell command."
        }

        h2 id="not-built-yet" { "Not built yet" }
        ul class="post__list" {
            li { "FerrumVibe has no editor column: files open in a separate Ferrum Editor window." }
            li { "One chat per FerrumVibe window, and no Windows build." }
            li {
                "The chat's providers are tested against recorded streams; live calls to \
                 Anthropic, OpenAI, Ollama and real ACP agents weren't part of the tests."
            }
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
        assert!(html.contains("⌥⌘1"));
        assert!(html.contains("<code>fex</code>"));
        assert!(html.contains(">Not built yet</h2>"));
        assert!(html.contains(r#"href="/ferrumide#tools""#));
    }
}
