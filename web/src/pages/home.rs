//! Home page, top to bottom: a hero over a desktop with one app window and a
//! dock of every app (`home_desk`), an agent making a change in
//! FerrumEngineer, shell commands, a README with an anchor index
//! (`home_readme`), and a closing download.

use maud::{Markup, html};

use super::{home_desk, home_readme, tool_demos};
use crate::components::{
    button::{self, Variant},
    code, icons,
};
use crate::layout::{PageMeta, page};
use crate::products::{self, Product};
use crate::{seo, site};

const DESCRIPTION: &str = "Open-source native developer tools in Rust: a code editor, Git \
     client, terminal, API client, database IDE and more for macOS and Linux, installed by one \
     free launcher.";

const HEADLINE: [&str; 2] = ["Your daily tools,", "rebuilt in Rust."];

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(None, DESCRIPTION, "/", "page-home")
        .json_ld(seo::organization(site_url))
        .json_ld(seo::website(site_url));
    page(
        site_url,
        &meta,
        html! {
            (hero())
            (agent_showcase())
            (commands())
            (home_readme::render())
            (closing())
        },
    )
}

/// A secondary button to the source, with the GitHub mark.
fn source_button(label: &str) -> Markup {
    html! {
        a class="btn btn--secondary btn--icon" href=(site::SOURCE_REPO_URL) rel="noopener" {
            (icons::github()) (label)
        }
    }
}

fn hero() -> Markup {
    html! {
        section class="hero" aria-labelledby="hero-title" {
            div class="container hero__head" {
                h1 class="hero__title" id="hero-title" {
                    span class="hero__line" { (HEADLINE[0]) }
                    " "
                    span class="hero__line" { (HEADLINE[1]) }
                }
                div class="hero__copy" {
                    p class="hero__lead" {
                        "A code editor, Git client, terminal, API client, database IDE and six \
                         more tools, drawn on the GPU with no browser underneath. One launcher \
                         installs and updates them. All of it is open source."
                    }
                    div class="hero__ctas" {
                        (button::link("Download for macOS", site::MACOS_DOWNLOAD_PATH, Variant::Primary))
                        (source_button("View source"))
                    }
                }
            }
            div class="container" { (home_desk::render()) }
            div class="container hero__foot" {
                p class="hero__note" {
                    "The download button gets the Apple Silicon build. Intel Macs and Linux (.deb, \
                     x86_64) are on the "
                    a href=(site::DOWNLOAD_PATH) { "download page" } ". Signed and notarized by \
                     Apple, and free."
                }
                dl class="hero-stats" {
                    div class="hero-stat" {
                        dt { "apps, launcher included" }
                        dd { (products::ALL.len()) }
                    }
                    div class="hero-stat" {
                        dt { "licence, your choice" }
                        dd { "MIT / Apache-2.0" }
                    }
                    div class="hero-stat" {
                        dt { "Electron, webviews or JS runtimes" }
                        dd { "0" }
                    }
                }
            }
        }
    }
}

/// What the FerrumEngineer window on this section shows, step by step.
const AGENT_STEPS: [(&str, &str); 4] = [
    ("You ask", "\u{201c}Round cart totals to cents.\u{201d}"),
    (
        "The agent edits",
        "It changes src/cart.ts and says what it did.",
    ),
    (
        "Git shows the diff",
        "The change is in the Git column before you switch windows, ready to commit.",
    ),
    (
        "The terminal runs the tests",
        "Run in terminal sends pnpm test to the terminal column. 4 passed.",
    ),
];

fn agent_showcase() -> Markup {
    let p = products::FERRUMENGINEER;
    html! {
        section class="section agent-show" id="agents-at-work" aria-labelledby="agent-show-title"
            style=(p.accent_style()) {
            div class="container" {
                div class="section-header" {
                    h2 id="agent-show-title" { "Ask for a change, watch it land" }
                    p class="lead" {
                        "FerrumEngineer puts an agent chat, FerrumGIT and FerrumTerm side by side \
                         on one project. The chat uses your own API key, a local Ollama or an \
                         agent you already have, such as Claude Code."
                    }
                }
                div class="agent-show__body" {
                    ol class="agent-steps" {
                        @for (title, text) in AGENT_STEPS {
                            li { h3 { (title) } p { (text) } }
                        }
                    }
                    div class="agent-show__demo" role="img"
                        aria-label="FerrumEngineer: the agent edits cart.ts, Git shows the diff \
                                    and the terminal runs the tests" {
                        (tool_demos::vibe())
                    }
                }
                p class="agent-show__more" {
                    a href=(p.path()) { "More about FerrumEngineer" }
                }
            }
        }
    }
}

/// Shell commands and what they open, from `docs::shell_commands` and
/// term/README.md (fterm).
const COMMANDS: [(&str, Product, &str); 6] = [
    (
        "fei .",
        products::FERRUMIDE,
        "Open this folder in FerrumIDE.",
    ),
    (
        "feg .",
        products::FERRUMGIT,
        "Open the repository in this folder in FerrumGIT.",
    ),
    (
        "fet",
        products::FERRUMTERM,
        "Start a FerrumTerm session in this folder.",
    ),
    (
        "fen .",
        products::FERRUMENGINEER,
        "Open FerrumEngineer on this folder: chat, Git and terminal.",
    ),
    (
        "fterm run --window -l agent -- claude",
        products::FERRUMTERM,
        "Run Claude Code in its own tab. Close the window and it keeps going.",
    ),
    (
        "fterm wait -t agent --status done,blocked",
        products::FERRUMTERM,
        "Wait until that agent is done or needs you. Handy in scripts.",
    ),
];

fn commands() -> Markup {
    html! {
        section class="section section--band commands" id="commands" aria-labelledby="commands-title" {
            div class="container" {
                div class="section-header" {
                    h2 id="commands-title" { "Open anything from your shell" }
                    p class="lead" {
                        "Turn on an app's shell command in FerrumLauncher, then use it from any \
                         project folder. FerrumTerm's fterm CLI drives sessions and agents."
                    }
                }
                ul class="cmd-list" {
                    @for (cmd, p, text) in COMMANDS {
                        li class="cmd" style=(p.accent_style()) {
                            a class="cmd__app app-dot" href=(p.path()) { (p.name) }
                            code class="cmd__line" { (cmd) }
                            p class="cmd__text" { (text) }
                        }
                    }
                }
                p class="commands__more" {
                    "Every app has one. See "
                    a href=(super::docs::shell_commands::PATH) { "shell commands" }
                    " for the full list and how to rename them."
                }
            }
        }
    }
}

fn closing() -> Markup {
    html! {
        section class="cta closing" aria-labelledby="closing-title" {
            div class="container cta__inner closing__inner" {
                div class="cta__copy" {
                    h2 class="cta__title closing__title" id="closing-title" {
                        (HEADLINE[0]) " " (HEADLINE[1])
                    }
                    p {
                        "Free, open source, and one download away. Or paste this in Terminal on a \
                         Mac: it fetches the newest launcher, checks its checksum and opens it."
                    }
                    (code::command(site::MACOS_INSTALL_COMMAND))
                }
                div class="cta__actions" {
                    (button::link("Download for macOS", site::MACOS_DOWNLOAD_PATH, Variant::Primary))
                    (button::link("All downloads", site::DOWNLOAD_PATH, Variant::Secondary))
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
    fn hero_has_one_headline_downloads_and_the_source() {
        let html = rendered();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains(site::MACOS_DOWNLOAD_PATH));
        assert!(html.contains(&format!(r#"href="{}""#, site::SOURCE_REPO_URL)));
        assert!(html.contains("open source"));
        assert!(html.contains("MIT / Apache-2.0"));
        assert!(html.contains(r#"<dd>12</dd>"#));
        assert!(html.contains(r#""@type":"Organization""#));
        assert!(html.contains(r#""@type":"WebSite""#));
    }

    #[test]
    fn shows_an_agent_at_work_and_shell_commands() {
        let html = rendered();
        assert!(html.contains(r#"id="agents-at-work""#));
        assert!(html.contains("Round cart totals to cents"));
        for (cmd, p, _) in &COMMANDS {
            assert!(html.contains(cmd), "{cmd}");
            assert!(html.contains(&format!(r#"href="{}""#, p.path())));
        }
        assert!(html.contains(r#"href="/docs/shell-commands""#));
    }

    #[test]
    fn closes_with_download_and_the_install_command() {
        let html = rendered();
        assert!(html.contains(r#"id="closing-title""#));
        assert!(html.contains(r#"data-copy="curl -fsSL"#));
        assert!(html.contains(r#"href="/download""#));
        assert!(!html.contains("<form"));
    }
}
