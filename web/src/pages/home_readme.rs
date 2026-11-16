//! The home page's README: how Ferrum fits together, with a plain anchor
//! index (the topics aren't a sequence, so they aren't numbered). Claims
//! follow the READMEs: term/README.md (agent statuses and notifications),
//! shared/ui (the project bar), launcher/README.md and INSTALL.md.

use maud::{Markup, html};

use crate::components::code;
use crate::site;

/// Anchor and title of each topic, in page order.
pub const TOPICS: [(&str, &str); 7] = [
    ("native", "Native, not Electron"),
    ("launcher", "One launcher"),
    ("agents", "Agents in every terminal"),
    ("project-bar", "One project bar"),
    ("local", "Your work stays local"),
    ("open-source", "Open source"),
    ("faq", "Questions"),
];

/// Layers between a keystroke and the screen, top to bottom.
const FERRUM_STACK: [&str; 3] = ["The app, in Rust", "GPUI", "Metal, on the GPU"];
const BROWSER_STACK: [&str; 4] = [
    "The app, in JavaScript",
    "A UI framework",
    "Node.js and V8",
    "Chromium",
];

/// FerrumTerm's agent groups, as its sidebar names them.
const AGENT_STATES: [(&str, &str, &str); 4] = [
    (
        "needs",
        "Needs you",
        "A question or a permission prompt is waiting.",
    ),
    (
        "done",
        "Done",
        "It finished a turn you haven't looked at yet.",
    ),
    ("working", "Working", "It's busy."),
    ("idle", "Idle", "Nothing new."),
];

const QUESTIONS: [(&str, &str); 4] = [
    (
        "Is it free?",
        "Yes. Every build is a free download with no account, sign-in or licence key, and the \
         source is open.",
    ),
    (
        "Which systems?",
        "macOS on Apple Silicon and Intel, and Linux on x86_64 (.deb). Windows builds aren't \
         published yet.",
    ),
    (
        "Is it ready?",
        "Every app is in alpha: usable every day, released often, with rough edges. Each app's \
         page says what isn't built yet.",
    ),
    (
        "Does it phone home?",
        "Only to be counted. The apps have no telemetry and no Ferrum server holds your work; \
         the launcher sends one anonymous count a day, which you can turn off in Settings.",
    ),
];

fn stack_col(class: &str, name: &str, layers: &[&str]) -> Markup {
    html! {
        div class={ "stack__col " (class) } {
            p class="stack__name" { (name) }
            ol class="stack__layers" {
                @for l in layers { li { (l) } }
            }
        }
    }
}

fn topic(id: &str, body: Markup) -> Markup {
    let title = TOPICS
        .iter()
        .find(|(t, _)| *t == id)
        .map_or(id, |(_, title)| title);
    html! {
        section class="readme__topic" id=(id) aria-labelledby={ (id) "-title" } {
            h3 class="readme__h" id={ (id) "-title" } { (title) }
            (body)
        }
    }
}

pub fn render() -> Markup {
    html! {
        section class="section readme-section" id="readme" aria-labelledby="readme-title" {
            div class="container readme-layout" {
                nav class="readme-index" aria-labelledby="readme-index-title" {
                    h2 class="readme-index__title" id="readme-title" { "How Ferrum works" }
                    p class="visually-hidden" id="readme-index-title" { "On this page" }
                    ul {
                        @for (id, title) in TOPICS {
                            li { a href={ "#" (id) } { (title) } }
                        }
                    }
                }
                div class="readme" {
                    p class="readme__bar" aria-hidden="true" { "README.md" }
                    div class="readme__body" {
                        (native())
                        (launcher())
                        (agents())
                        (project_bar())
                        (local())
                        (open_source())
                        (questions())
                    }
                }
            }
        }
    }
}

fn native() -> Markup {
    topic(
        "native",
        html! {
            p {
                "No Electron, no webview, no JavaScript runtime. Each app is compiled Rust drawn \
                 by GPUI, the GPU framework behind Zed, so it opens fast and long lists, result \
                 grids and histories scroll without a stutter."
            }
            figure class="stack" {
                div class="stack__cols" {
                    (stack_col("stack__col--ferrum", "A Ferrum app", &FERRUM_STACK))
                    (stack_col("stack__col--web", "A browser in a box", &BROWSER_STACK))
                }
                figcaption {
                    "Fewer layers between your keystroke and the screen."
                }
            }
        },
    )
}

fn launcher() -> Markup {
    topic(
        "launcher",
        html! {
            p {
                "You download FerrumLauncher once. It installs every other app, checks each \
                 download against its SHA-256 checksum, looks for new releases on startup and \
                 every 6 hours, and updates itself in place. It keeps the last three versions of \
                 each app, so you can switch back."
            }
            p {
                a href="/docs/install" { "Install guide" } " · "
                a href=(crate::products::FERRUM_LAUNCHER.path()) { "FerrumLauncher" }
            }
        },
    )
}

fn agents() -> Markup {
    topic(
        "agents",
        html! {
            p {
                "FerrumTerm's sidebar lists every coding agent running in any of its terminals, \
                 such as Claude Code, Codex, Gemini or aider, even one you typed into a plain \
                 shell. They're grouped by state:"
            }
            dl class="agent-states" {
                @for (class, name, what) in AGENT_STATES {
                    div class={ "agent-state agent-state--" (class) } {
                        dt { (name) }
                        dd { (what) }
                    }
                }
            }
            p {
                "When an agent needs you or finishes while its terminal is off screen, you get a \
                 toast, a system notification, a Dock badge and a sound, each one switchable in \
                 Settings. Sessions live in a background server, so closing the window stops \
                 nothing. FerrumIDE and FerrumEngineer use the same terminal."
            }
        },
    )
}

fn project_bar() -> Markup {
    topic(
        "project-bar",
        html! {
            p {
                "A strip at the top of the window shows the current project, its path and Git \
                 branch. " (code::keys("⌥⌘O")) " switches between recent projects, a list every \
                 app shares, and one click opens the project in FerrumIDE, FerrumGIT or \
                 FerrumTerm."
            }
            p { a href="/docs/project-bar" { "About the project bar" } }
        },
    )
}

fn local() -> Markup {
    topic(
        "local",
        html! {
            p {
                "Requests, queries, repositories, vaults and terminal sessions stay on your \
                 machine. Nothing needs an account, and the apps contain no analytics or \
                 telemetry. The launcher sends one anonymous daily count, "
                a href="/privacy#usage-counts" { "and you can turn it off" } "."
            }
            p { a href="/security" { "How Ferrum handles security" } }
        },
    )
}

fn open_source() -> Markup {
    topic(
        "open-source",
        html! {
            p {
                "The source of every app, the launcher, the shared UI crate and this website is \
                 public on GitHub, under the MIT licence or Apache-2.0, at your choice. Third-party \
                 parts keep their own licences, listed on "
                a href=(crate::pages::licenses::PATH) { "Licences" } "."
            }
            p { "To build FerrumIDE from source:" }
            pre class="code-block" {
                code { @for line in site::BUILD_FROM_SOURCE { (line) "\n" } }
            }
            p {
                "Each app's README names its package. Bugs and ideas go to "
                a href=(site::ISSUES_URL) rel="noopener" { "issues" } "; before a pull request, \
                 read "
                a href=(site::CONTRIBUTING_URL) rel="noopener" { "CONTRIBUTING" } "."
            }
            p { a href=(site::SOURCE_REPO_URL) rel="noopener" { (site::SOURCE_REPO_SLUG) " on GitHub" } }
        },
    )
}

fn questions() -> Markup {
    topic(
        "faq",
        html! {
            dl class="readme-faq" {
                @for (q, a) in QUESTIONS {
                    div { dt { (q) } dd { (a) } }
                }
            }
            p { a href=(crate::pages::faq::PATH) { "All questions" } }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_topic_has_its_anchor_and_index_link() {
        let html = render().into_string();
        for (id, title) in TOPICS {
            assert!(html.contains(&format!(r##"href="#{id}""##)), "{id}");
            assert!(html.contains(&format!(r#"id="{id}""#)), "{id}");
            assert!(html.contains(title), "{title}");
        }
        assert!(!html.contains("<ol class=\"readme-index"), "not a sequence");
    }

    #[test]
    fn names_agent_states_and_how_to_build() {
        let html = render().into_string();
        for (_, name, _) in AGENT_STATES {
            assert!(html.contains(name), "{name}");
        }
        for line in site::BUILD_FROM_SOURCE {
            assert!(html.contains(line), "{line}");
        }
        assert!(html.contains(site::CONTRIBUTING_URL));
        assert!(html.contains("MIT licence or Apache-2.0"));
    }
}
