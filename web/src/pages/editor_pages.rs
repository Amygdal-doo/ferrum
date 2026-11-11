//! Sections of the Ferrum Editor page for its built-in pages that used to be
//! separate apps: Logs (formerly FerrumLogs, at `#logs`) and CI (formerly
//! FerrumCI, at `#ci`). Each reuses its animated demo from `project_demos`
//! and the `.feature-app` styles of the features page.
//!
//! Logs claims follow the FerrumLogs README ("What works today").

use maud::{Markup, html};

use super::project_demos;
use crate::products::FERRUM_EDITOR;

/// One built-in page, shown as a section with copy on one side and its demo
/// on the other.
pub struct EditorPage {
    /// Section id, e.g. `logs` for `/ferrum-editor#logs`.
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
    demo_label: "Ferrum Editor's Logs page merging a dev server, a database container and a \
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
    demo_label: "Ferrum Editor's CI page listing workflow runs, with a failed job's steps and \
                 its first error in the log",
};

/// `/ferrum-editor#<anchor>`.
pub fn path(page: &EditorPage) -> String {
    format!("{}#{}", FERRUM_EDITOR.path(), page.anchor)
}

/// One built-in page as a section of the editor's page.
pub fn section(page: &EditorPage) -> Markup {
    let heading = format!("{}-title", page.anchor);
    html! {
        section class="feature-app" id=(page.anchor) style=(FERRUM_EDITOR.accent_style())
            aria-labelledby=(heading) {
            div class="container feature-app__inner" {
                div class="feature-app__copy" {
                    p class="eyebrow" { "Built in" }
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
        let logs = section(&LOGS).into_string();
        assert!(logs.contains(r#"id="logs""#));
        assert!(logs.contains("Ferrum Editor · Logs"));
        assert!(!logs.contains("FerrumLogs"));
        let ci = section(&CI).into_string();
        assert!(ci.contains(r#"id="ci""#));
        assert!(ci.contains("Ferrum Editor · CI"));
        assert!(!ci.contains("FerrumCI"));
        assert_eq!(path(&LOGS), "/ferrum-editor#logs");
        assert_eq!(path(&CI), "/ferrum-editor#ci");
    }
}
