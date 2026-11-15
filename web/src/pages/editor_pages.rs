//! Sections of the FerrumIDE page: the Git, Agent and Term panes (at
//! `#tools`), and the built-in pages that used to be separate apps: Logs
//! (formerly FerrumLogs, at `#logs`) and CI (formerly FerrumCI, at `#ci`).
//! Each has an animated demo from `tool_demos` or `project_demos` and uses
//! the `.feature-app` styles of the features page.
//!
//! Tool pane claims follow the editor README ("Tool panes"); Logs claims
//! follow the FerrumLogs README ("What works today").

use maud::{Markup, html};

use super::{project_demos, tool_demos};
use crate::products::FERRUMIDE;

/// One built-in page, shown as a section with copy on one side and its demo
/// on the other.
pub struct EditorPage {
    /// Section id, e.g. `logs` for `/ferrumide#logs`.
    pub anchor: &'static str,
    pub title: &'static str,
    pub meta: &'static str,
    pub lead: &'static str,
    /// (title, body) pairs.
    pub points: &'static [(&'static str, &'static str)],
    pub demo: fn() -> Markup,
    /// Text alternative for the animated demo.
    pub demo_label: &'static str,
}

pub const TOOLS: EditorPage = EditorPage {
    anchor: "tools",
    title: "Git, Agent and Term, inside the editor",
    meta: "FerrumGIT, FerrumChat and FerrumTerm · tool panes",
    lead: "The Git, Agent and Term buttons at the top right open each tool inside the window \
           as a split pane. Each is the tool's own view, from the same library the FerrumGIT \
           and FerrumTerm apps and FerrumEngineer are built on.",
    points: &[
        (
            "All of them at once",
            "Git and Agent open on the right and Term at the bottom, next to the CI, Logs and \
             Debug panes. Click a button again, or press its shortcut, to close the pane.",
        ),
        (
            "Put them where you want",
            "Move a pane to the left, right or bottom from its ⋯ menu or by dragging its \
             header, and drag the dividers to resize. The layout is saved per project.",
        ),
        (
            "Wired to the editor",
            "Open in a diff or a file link in the chat opens the file at that line. Run in \
             terminal runs the chat's command in a new Term tab, and open files reload when the \
             agent changes them.",
        ),
        (
            "Nothing stops when you close one",
            "A closed pane keeps its view: the terminal keeps running and the chat keeps its \
             thread. Open in FerrumGIT app and Open in FerrumTerm app are in the ⋯ menu.",
        ),
    ],
    demo: tool_demos::editor_panes,
    demo_label: "FerrumIDE with the Git and Agent panes docked on the right, the Terminal \
                 pane at the bottom, and a pane menu offering Move to Left and Move to Bottom",
};

pub const LOGS: EditorPage = EditorPage {
    anchor: "logs",
    title: "Logs, inside the editor",
    meta: "Project logs · Logs page",
    lead: "Everything a project prints while you develop, in one live, filtered view: log \
           files, dev servers and other commands, Docker containers and Compose services.",
    points: &[
        (
            "Sources per project",
            "Files and globs such as logs/*.log, followed through truncation and rotation; \
             commands such as pnpm dev, started, stopped and restarted from the page; \
             docker logs and docker compose logs. Sources are saved per project.",
        ),
        (
            "Suggestions to start from",
            "For a project without sources: package.json dev scripts, cargo run, manage.py \
             runserver, Compose services and *.log files in the usual folders. One click adds \
             and starts one.",
        ),
        (
            "Levels, colours and JSON",
            "Levels detected from common formats, ANSI colours rendered, stack traces kept \
             with their error, and JSON lines from pino, bunyan, tracing or zap shown as level \
             and message with every field in a details pane.",
        ),
        (
            "Filter, search and follow",
            "Level toggles, Errors only (⌘E), text or regex search with highlighted matches, \
             and a tail that pauses when you scroll up, with Jump to latest.",
        ),
    ],
    demo: project_demos::logs,
    demo_label: "FerrumIDE's Logs page merging a dev server, a database container and a \
                 log file, with an error line highlighted",
};

pub const CI: EditorPage = EditorPage {
    anchor: "ci",
    title: "CI, inside the editor",
    meta: "GitHub Actions · CI page",
    lead: "Check on CI without a browser tab. The CI page reads the GitHub repository of the \
           project you have open and shows its Actions runs next to your code.",
    points: &[
        (
            "Runs for the open project",
            "GitHub Actions runs for the repository you have open, with their status, branch \
             and duration, and the jobs and steps of each run.",
        ),
        (
            "Live logs, in colour",
            "Follow a job's log while it runs, with ANSI colours rendered instead of escape \
             codes.",
        ),
        (
            "Run a workflow",
            "Start a workflow_dispatch workflow from a form built from its inputs, without \
             leaving the editor.",
        ),
        (
            "Pull request checks",
            "Open pull requests with the status of their checks, next to the code they change.",
        ),
    ],
    demo: project_demos::ci,
    demo_label: "FerrumIDE's CI page listing workflow runs, with a failed job's steps and \
                 its first error in the log",
};

/// `/ferrumide#<anchor>`.
pub fn path(page: &EditorPage) -> String {
    format!("{}#{}", FERRUMIDE.path(), page.anchor)
}

/// One built-in page as a section of the editor's page.
pub fn section(page: &EditorPage) -> Markup {
    let heading = format!("{}-title", page.anchor);
    html! {
        section class="feature-app" id=(page.anchor) style=(FERRUMIDE.accent_style())
            aria-labelledby=(heading) {
            div class="container feature-app__inner" {
                div class="feature-app__copy" {
                    div class="feature-app__head" {
                        h2 id=(heading) { (page.title) }
                        p class="feature-app__meta" { (page.meta) }
                    }
                    p class="lead" { (page.lead) }
                    ul class="feature-points" {
                        @for (title, body) in page.points {
                            li {
                                strong { (title) }
                                span { (body) }
                            }
                        }
                    }
                }
                div class="feature-app__demo" role="img" aria-label=(page.demo_label) {
                    ((page.demo)())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_carry_their_anchor_and_demo() {
        let tools = section(&TOOLS).into_string();
        assert!(tools.contains(r#"id="tools""#));
        assert!(tools.contains("Move to Left"));
        assert_eq!(path(&TOOLS), "/ferrumide#tools");
        let logs = section(&LOGS).into_string();
        assert!(logs.contains(r#"id="logs""#));
        assert!(logs.contains("FerrumIDE · Logs"));
        assert!(!logs.contains("FerrumLogs"));
        let ci = section(&CI).into_string();
        assert!(ci.contains(r#"id="ci""#));
        assert!(ci.contains("FerrumIDE · CI"));
        assert!(!ci.contains("FerrumCI"));
        assert_eq!(path(&LOGS), "/ferrumide#logs");
        assert_eq!(path(&CI), "/ferrumide#ci");
    }
}
