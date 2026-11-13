//! Ferrum Editor product page. Claims follow the Ferrum Editor README
//! ("What works today" / "Not built yet") and nothing beyond it.
//!
//! The tool panes (Git, Agent and Term, `editor_pages::TOOLS`) and the Logs
//! and CI pages (formerly the FerrumLogs and FerrumCI apps) have their own
//! sections, defined in `editor_pages`; `/ferrumlogs` and `/ferrumci`
//! redirect to [`LOGS_PATH`] and [`CI_PATH`].

use maud::{Markup, html};

use super::editor_pages;
use super::product_sections::{self as sections, BulletGroup, ProductPage};
use crate::products::FERRUM_EDITOR;

/// Where `/ferrumlogs` points: this page's Logs section.
pub const LOGS_PATH: &str = "/ferrum-editor#logs";
/// Where `/ferrumci` points: this page's CI section.
pub const CI_PATH: &str = "/ferrum-editor#ci";

const FEATURES: &[(&str, &str)] = &[
    (
        "The Atom layout",
        "Tree view, tabs with a dirty dot, split panes, a fuzzy file finder (⌘P), a command \
         palette (⇧⌘P) with every command and its keys, and go to line.",
    ),
    (
        "Git, Agent and Term inside",
        "The Git, Agent and Term buttons open FerrumGIT, FerrumChat and FerrumTerm as split \
         panes in the window, all at once if you like. The layout is saved per project.",
    ),
    (
        "Multi-cursor editing",
        "⌘-click to add carets, ⌘D for the next occurrence, select all occurrences, split a \
         selection into lines, add cursors above and below, Alt-drag for columns.",
    ),
    (
        "Find, here and everywhere",
        "Find and replace in the file with case, whole word and regex with $1 groups, and \
         find in files that respects .gitignore.",
    ),
    (
        "Tree-sitter highlighting",
        "Rust, JavaScript, TypeScript, JSON, TOML, YAML, Markdown, Python, Go, HTML, CSS and \
         Bash, parsed in the background.",
    ),
    (
        "Hackable in plain text",
        "config.toml, keymap.toml, theme.toml and snippets.toml in ~/.ferrum/editor, opened \
         from the palette and applied the moment you save.",
    ),
    (
        "Language servers",
        "Completions as you type, problems underlined, go to definition, rename, find \
         references and code actions. rust-analyzer, gopls, TypeScript and the others \
         install themselves the first time you open a file.",
    ),
    (
        "Its own editor core",
        "Rope buffer, undo, multi-cursor, rendering and input are written from scratch on \
         GPUI, not forked from another editor.",
    ),
];

const WORKS_TODAY: &[&str] = &[
    "Rope buffer that keeps line endings and BOM, atomic saves, binary files refused",
    "Multi-cursor typing, paste and delete as single undo steps",
    "Auto-close brackets, auto-indent, toggle comment, move and duplicate lines",
    "Find and replace, find in files, fuzzy finder and command palette",
    "Git line marks and branch, Markdown preview, reload on change, session restore",
    "Snippets with tab stops; ferrumeditor path:line:col",
    "Git (⌥⌘G), Agent (⌥⌘A) and Term (⌃`) panes, docked left, right or bottom",
    "Pane layout saved per project; a closed pane keeps its terminal or chat",
    "Theme picker with live preview; TOML themes and VS Code theme import",
    "File icons in the tree, tabs and finder; loadable icon themes",
    "Autocomplete from language servers, or from words in the file without one",
    "Language servers install themselves (rust-analyzer, gopls, TypeScript and more)",
    "Build tasks with ⌘B in the Term pane, with their errors as problems",
    "Problems underlined, listed (⌃⇧M) and stepped through with F8",
    "Go to definition (F12 or ⌘-click), rename (F2), references and code actions",
    "Logs page: files, commands and containers of a project tailed in one filtered view",
    "CI page: GitHub Actions runs, live logs, workflow dispatch and pull request checks",
];

const NOT_BUILT_YET: &[&str] = &[
    "Packages and plugins (the seam is there)",
    "Inlay hints, semantic highlighting and workspace symbols",
    "Choosing where a dropped pane lands: it always goes after the pane you drop it on",
    "Code folding and a minimap",
    "Multiple windows and project-wide replace",
    "Encodings other than UTF-8, and files over 64 MB",
];

const PAGE: ProductPage = ProductPage {
    product: FERRUM_EDITOR,
    meta_description: "Ferrum Editor is a small, hackable code editor written in Rust and \
                       configured in plain TOML, with Git, an agent chat and a terminal as \
                       panes, plus LSP and a debugger.",
    intro: "Tree view, tabs, fuzzy finder and command palette, with config, keymap, theme and \
            snippets in plain files you edit in the editor itself. Lighter than an IDE, and \
            built from scratch rather than forked.",
    features_lead: "The buffer, cursors and commands are covered by unit tests, and the app by \
                    headless tests that type real keystrokes through the keymap.",
    features: FEATURES,
    screenshots: &[
        "three cursors typing at once",
        "the command palette filtering on toggle",
    ],
    status_summary: "Ferrum Editor is in alpha, a first vertical slice. It is tested \
                     automatically, but the window is new and its visual polish is unverified.",
    status_groups: &[
        BulletGroup {
            heading: "Works today",
            items: WORKS_TODAY,
        },
        BulletGroup {
            heading: "Not built yet",
            items: NOT_BUILT_YET,
        },
    ],
    specs: &[
        ("Install", "Ferrum Launcher (free, see Download)"),
        (
            "Network",
            "Only for what you use: language server downloads, the CI page (GitHub, with your \
             own token) and the Agent pane (the model or agent you pick)",
        ),
        (
            "Builds",
            "macOS (Apple Silicon and Intel), Linux x86_64 and Windows",
        ),
        (
            "Built with",
            "Rust, GPUI, ropey, tree-sitter, and the FerrumGIT, FerrumChat and FerrumTerm views",
        ),
        ("Tested on", "macOS (automated tests)"),
    ],
    specs_note: "Key bindings are designed for macOS; on Linux and Windows ⌘ becomes Ctrl.",
};

pub fn render(site_url: &str) -> Markup {
    let extra = html! {
        (editor_pages::section(&editor_pages::TOOLS))
        (editor_pages::section(&editor_pages::LOGS))
        (editor_pages::section(&editor_pages::CI))
    };
    sections::render(site_url, &PAGE, extra)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_title_status_and_demo() {
        let html = render("https://example.test").into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains(">Ferrum Editor</h1>"));
        assert!(html.contains("Animated preview of Ferrum Editor"));
        assert!(html.contains("/assets/img/brand/editor.svg"));
        assert!(html.contains("href=\"/download\""));
    }

    #[test]
    fn has_logs_and_ci_sections_with_their_demos() {
        let html = render("https://example.test").into_string();
        assert!(html.contains(r#"id="tools""#));
        assert!(html.contains(r#"id="logs""#));
        assert!(html.contains(r#"id="ci""#));
        assert!(html.contains("Ferrum Editor · Logs"));
        assert!(html.contains("Ferrum Editor · CI"));
        assert!(html.contains("workflow_dispatch"));
        assert!(!html.contains("FerrumCI"));
        assert!(!html.contains("FerrumLogs"));
        assert_eq!(LOGS_PATH, editor_pages::path(&editor_pages::LOGS));
        assert_eq!(CI_PATH, editor_pages::path(&editor_pages::CI));
    }
}
