//! `/docs/project-bar`: the project strip shared by the apps. Follows
//! `shared/ui/src/project_bar.rs` and `shared/ui/src/project.rs`.

use maud::{Markup, html};

use super::{article, trail};
use crate::components::{
    callout::{self, Kind},
    code,
};
use crate::layout::PageMeta;
use crate::products::{self, Product};

pub const PATH: &str = "/docs/project-bar";
pub const TITLE: &str = "The project bar";

/// Apps whose project bar switches projects with ⌥⌘O.
const SWITCHING: [Product; 6] = [
    products::FERRUM_EDITOR,
    products::FERRUMGIT,
    products::FERRUMTERM,
    products::FERRUMCONTAINERS,
    products::FERRUMPORTS,
    products::FERRUMENV,
];

/// Apps that show the bar without the switcher.
const SHOW_ONLY: [Product; 2] = [products::FERRUMAPI, products::FERRUMDATA];

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(
        Some("The project bar and ⌥⌘O project switcher"),
        "The strip on top of Ferrum windows: project and Git branch, ⌥⌘O to switch between \
         recent projects shared by every app, and buttons to open the project elsewhere.",
        PATH,
        "page-docs",
    )
    .trail(trail("Project bar", PATH));
    article(
        site_url,
        &meta,
        TITLE,
        html! {
            p {
                "The strip along the top of most Ferrum windows shows the project you're working \
                 on, and moves it between apps. A folder you open in Ferrum Editor shows up in \
                 FerrumGIT's and FerrumTerm's switchers too."
            }
        },
        html! {
            figure class="bar-diagram" {
                div class="bar-diagram__strip" aria-hidden="true" {
                    span class="bar-diagram__name" { "ferrum ▾" }
                    span class="bar-diagram__path" { "~ / Projects / ferrum" }
                    span class="bar-diagram__branch" { "⎇ main" }
                    span class="bar-diagram__buttons" { span { "Editor" } span { "Git" } span { "Term" } }
                }
                figcaption {
                    "The project bar: project name, path, Git branch, and buttons that open the \
                     project in another app."
                }
            }

            h2 id="parts" { "What's on it" }
            dl class="spec-sheet" {
                div class="spec-sheet__row" {
                    dt { "Project name" }
                    dd { "Opens the switcher. Same as " (code::keys("⌥⌘O")) "." }
                }
                div class="spec-sheet__row" {
                    dt { "Path" }
                    dd { "Click a segment to reveal that folder in Finder. Right-click copies the path." }
                }
                div class="spec-sheet__row" {
                    dt { "Branch" }
                    dd { "The Git branch of the project, when it's in a repository." }
                }
                div class="spec-sheet__row" {
                    dt { "Editor, Git, Term" }
                    dd { "Open the same folder in Ferrum Editor, FerrumGIT or FerrumTerm." }
                }
            }

            h2 id="switcher" { "Switch projects with " (code::keys("⌥⌘O")) }
            ol class="prose-steps" {
                li { "Press " (code::keys("⌥⌘O")) " or click the project name." }
                li {
                    "Type to filter the recent projects by path. Use " (code::keys("↑")) " and "
                    (code::keys("↓")) " to move, " (code::keys("↩")) " to open."
                }
                li {
                    "No match? Type a folder path (" code { "~/" } " works) and press "
                    (code::keys("↩")) " to open it, or choose " strong { "Open Folder…" } "."
                }
            }
            p {
                "Each row has a button to remove it from the recent projects. Opening a folder \
                 that no longer exists shows a message instead."
            }

            h2 id="where" { "Which apps have it" }
            ul class="prose-list" {
                li {
                    strong { "Switches projects: " }
                    @for (i, p) in SWITCHING.iter().enumerate() {
                        @if i > 0 { ", " }
                        a href=(p.path()) { (p.name) }
                    }
                    "."
                }
                li {
                    strong { "Shows the project, no switcher: " }
                    @for (i, p) in SHOW_ONLY.iter().enumerate() {
                        @if i > 0 { " and " }
                        a href=(p.path()) { (p.name) }
                    }
                    ". FerrumData shows the database connection in place of the path."
                }
                li { "FerrumKit and Ferrum Launcher have no project bar." }
            }

            h2 id="file" { "The recent projects file" }
            p {
                "Every app reads and writes the same list, " code { "~/.ferrum/projects.json" }
                ", newest first, up to 50 projects. Each entry has the folder's path, when it was \
                 last opened and which apps opened it:"
            }
            (code::block("{\n  \"projects\": [\n    {\n      \"path\": \"/Users/you/Projects/ferrum\",\n      \"opened_at\": 1793145600,\n      \"apps\": [\"ferrumeditor\", \"ferrumgit\"]\n    }\n  ]\n}"))
            (callout::render(Kind::Note, None, html! {
                p {
                    "The file only lists folders. Nothing in it leaves your Mac; deleting it just \
                     empties the recent list."
                }
            }))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documents_the_switcher_and_the_file() {
        let html = render("https://f.test").into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains("~/.ferrum/projects.json"));
        assert!(html.contains("Option Command O"));
        for p in SWITCHING.iter().chain(&SHOW_ONLY) {
            assert!(html.contains(&format!(r#"href="{}""#, p.path())));
        }
    }
}
