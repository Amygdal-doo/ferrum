//! Home page, top to bottom: a short hero over every app's logo circling a
//! window of real screenshots, the agents Ferrum works with, screenshot
//! sections for FerrumEngineer, FerrumGIT and FerrumTerm (alternating
//! sides), the other apps and tools, shell commands, a README
//! with an anchor index (`home_readme`) and a closing download.
//!
//! The screenshots (`assets/img/shots/<shot>-{dark,light}.webp`) are taken
//! from the release builds with a demo project; the one for the page's
//! theme shows.

use maud::{Markup, html};

use super::home_readme;
use crate::components::{
    ai_logo,
    button::{self, Variant},
    card, code, icons,
};
use crate::layout::{PageMeta, page};
use crate::products::{self, Product};
use crate::routes::assets;
use crate::{seo, site};

const DESCRIPTION: &str = "Open-source native developer tools in Rust: a code editor, Git \
     client, terminal, API client, database IDE and more for macOS and Linux, installed by one \
     free launcher.";

const HEADLINE: &str = "Ferrum is a set of native developer tools, open source and built in Rust.";

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(None, DESCRIPTION, "/", "page-home")
        .json_ld(seo::organization(site_url))
        .json_ld(seo::website(site_url));
    page(
        site_url,
        &meta,
        html! {
            (hero())
            (works_with())
            @for (i, show) in SHOWCASE.iter().enumerate() {
                (showcase(show, i % 2 == 1))
            }
            (more_apps())
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

/// A screenshot in both themes, at its pixel size (2000 wide) for layout.
fn shot(name: &str, height: u32, alt: &str, eager: bool) -> Markup {
    let loading = if eager { "eager" } else { "lazy" };
    html! {
        @for theme in ["dark", "light"] {
            img class={ "shot shot--" (theme) } src=(assets::url(&format!("img/shots/{name}-{theme}.webp")))
                alt=(alt) width="2000" height=(height) loading=(loading) decoding="async";
        }
    }
}

fn hero() -> Markup {
    html! {
        section class="hero" aria-labelledby="hero-title" {
            div class="container" {
                h1 class="hero__title" id="hero-title" { (HEADLINE) }
                div class="hero__ctas" {
                    (button::link("Download for macOS", site::MACOS_DOWNLOAD_PATH, Variant::Primary))
                    (source_button("View source"))
                }
                (orbit())
                p class="hero__note" {
                    "Free for macOS (Apple Silicon and Intel) and Linux, signed and notarized. "
                    a href=(site::DOWNLOAD_PATH) { "All downloads" }
                }
            }
        }
    }
}

/// The hero's screenshots, shown in turn: (shot, height, app, alt).
const HERO_SHOTS: [(&str, u32, Product, &str); 4] = [
    (
        "ide",
        1281,
        products::FERRUMIDE,
        "FerrumIDE with a TypeScript file open",
    ),
    (
        "eng",
        1179,
        products::FERRUMENGINEER,
        "FerrumEngineer: tasks and project files beside an agent thread",
    ),
    (
        "git",
        1250,
        products::FERRUMGIT,
        "FerrumGIT: a repository's history with a commit and its diff",
    ),
    (
        "term",
        1272,
        products::FERRUMTERM,
        "FerrumTerm: sessions by project, git log and passing tests side by side",
    ),
];

/// Every app's logo circling an ellipse around a window that shows four of
/// them in turn. No app leads: each logo links to its page.
fn orbit() -> Markup {
    let apps: Vec<_> = products::apps().collect();
    let count = apps.len();
    html! {
        div class="stage stage--hero orbit" {
            div class="orbit__window" {
                @for (i, (name, height, app, alt)) in HERO_SHOTS.iter().enumerate() {
                    div class="orbit__shot" style=(format!("--n: {i}; {}", app.accent_style())) {
                        (shot(name, *height, alt, i == 0))
                    }
                }
            }
            ul class="orbit__ring" aria-label="Every Ferrum app" {
                @for (i, p) in apps.iter().enumerate() {
                    li class="orbit__item" style=(format!("--at: {}%", i * 100 / count)) {
                        a href=(p.path()) title=(p.name) {
                            img src=(assets::url(&p.logo_asset())) alt=(p.name) width="56" height="56";
                        }
                    }
                }
            }
        }
    }
}

/// Coding agents, then model providers, as their own logos.
const AGENTS: [&str; 7] = [
    "Claude Code",
    "Codex",
    "Gemini CLI",
    "OpenCode",
    "Qwen Code",
    "Cursor Agent",
    "Amp",
];
const MODELS: [&str; 7] = [
    "Anthropic",
    "OpenAI",
    "Gemini",
    "DeepSeek",
    "Kimi",
    "GLM",
    "Ollama",
];

fn works_with() -> Markup {
    html! {
        section class="section section--tight works-with" aria-labelledby="works-with-title" {
            div class="container" {
                h2 class="works-with__title" id="works-with-title" {
                    "Works with the agents and models you already use"
                }
                (ai_logo::strips(&[("Coding agents", &AGENTS), ("Models", &MODELS)]))
            }
        }
    }
}

/// One app shown at work: a claim, a muted explanation, then a screenshot.
struct Show {
    product: Product,
    title: &'static str,
    rest: &'static str,
    shot: &'static str,
    height: u32,
    alt: &'static str,
}

const SHOWCASE: [Show; 3] = [
    Show {
        product: products::FERRUMENGINEER,
        title: "Agents do the work, you review it.",
        rest: "Give a task to Claude Code, Codex, Gemini or a model on your own key. Each change \
               comes back as a diff, with Git and a terminal on the same project.",
        shot: "eng",
        height: 1179,
        alt: "FerrumEngineer: tasks and project files on the left, an agent thread on the right",
    },
    Show {
        product: products::FERRUMGIT,
        title: "History you can read.",
        rest: "A commit graph, the diff of every file, branches, stashes and pull requests. It \
               reads your repository directly, with no account.",
        shot: "git",
        height: 1250,
        alt: "FerrumGIT: the history of a repository with a commit and its diff",
    },
    Show {
        product: products::FERRUMTERM,
        title: "A terminal that keeps going.",
        rest: "Sessions run in a small server, so closing the window stops nothing. Group them by \
               project, split them, and get a notification when an agent finishes.",
        shot: "term",
        height: 1272,
        alt: "FerrumTerm: sessions grouped by project, git log and passing tests side by side",
    },
];

fn showcase(show: &Show, flip: bool) -> Markup {
    let p = show.product;
    let id = format!("show-{}", p.slug);
    html! {
        section class={ "section showcase" @if flip { " showcase--flip" } } id=(p.slug)
            aria-labelledby=(id) style=(p.accent_style()) {
            div class="container showcase__inner" {
                div class="showcase__copy" {
                    p class="showcase__app" {
                        img src=(assets::url(&p.logo_asset())) alt="" width="28" height="28";
                        (p.name)
                    }
                    h2 class="showcase__title" id=(id) { (show.title) }
                    p class="showcase__rest" { (show.rest) }
                    a class="showcase__more" href=(p.path()) { "More about " (p.name) }
                }
                div class={ "stage stage--" (show.shot) } { (shot(show.shot, show.height, show.alt, false)) }
            }
        }
    }
}

fn more_apps() -> Markup {
    html! {
        section class="section section--band more-apps" aria-labelledby="more-apps-title" {
            div class="container" {
                div class="section-header" {
                    h2 id="more-apps-title" { "Twelve apps, one launcher" }
                    p class="lead" {
                        "FerrumLauncher installs, updates and removes every app, and turns on \
                         their shell commands. Each app also works on its own."
                    }
                }
                (card::grouped_app_strips(true))
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
                        "Your daily tools, rebuilt in Rust."
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
    fn hero_has_one_headline_downloads_the_source_and_a_screenshot() {
        let html = rendered();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains(HEADLINE));
        assert!(html.contains(site::MACOS_DOWNLOAD_PATH));
        assert!(html.contains(&format!(r#"href="{}""#, site::SOURCE_REPO_URL)));
        assert!(html.contains("img/shots/ide-dark.webp"));
        assert!(html.contains("img/shots/ide-light.webp"));
        assert!(html.contains(r#""@type":"Organization""#));
        assert!(html.contains(r#""@type":"WebSite""#));
    }

    #[test]
    fn every_screenshot_is_an_asset_of_the_stated_size() {
        let mut shots: Vec<_> = HERO_SHOTS.iter().map(|(n, h, ..)| (*n, *h)).collect();
        shots.extend(SHOWCASE.iter().map(|s| (s.shot, s.height)));
        for (name, height) in shots {
            for theme in ["dark", "light"] {
                let path = format!(
                    "{}/assets/img/shots/{name}-{theme}.webp",
                    env!("CARGO_MANIFEST_DIR")
                );
                let bytes = std::fs::read(&path).expect(&path);
                assert!(
                    bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP",
                    "{path}"
                );
                // VP8X header: canvas height minus one, 24-bit little endian.
                let h = u32::from_le_bytes([bytes[27], bytes[28], bytes[29], 0]) + 1;
                assert_eq!(h, height, "{path}");
            }
        }
    }

    #[test]
    fn every_app_circles_the_hero() {
        let html = rendered();
        let ring = &html[html.find("orbit__ring").expect("ring")..];
        let ring = &ring[..ring.find("</ul>").expect("end")];
        for p in products::apps() {
            assert!(
                ring.contains(&format!(r#"href="{}""#, p.path())),
                "{}",
                p.name
            );
        }
        assert_eq!(ring.matches("<li").count(), products::apps().count());
    }

    #[test]
    fn shows_the_main_apps_the_agents_and_the_rest() {
        let html = rendered();
        for show in &SHOWCASE {
            assert!(html.contains(show.title), "{}", show.title);
            assert!(html.contains(&format!(r#"href="{}""#, show.product.path())));
        }
        assert!(html.contains("claude.png"));
        assert!(html.contains("trademarks of their owners"));
        for p in products::apps() {
            assert!(
                html.contains(&format!(r#"href="{}""#, p.path())),
                "{}",
                p.name
            );
        }
    }

    #[test]
    fn has_shell_commands() {
        let html = rendered();
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
