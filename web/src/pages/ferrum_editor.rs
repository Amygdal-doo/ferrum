//! Ferrum Editor product page. Claims follow the Ferrum Editor README
//! ("What works today" / "Not built yet") and nothing beyond it.

use maud::{Markup, html};

use super::product_sections::{self as sections, BulletGroup, ProductPage};
use crate::products::FERRUM_EDITOR;

const FEATURES: &[(&str, &str)] = &[
    (
        "The Atom layout",
        "Tree view, tabs with a dirty dot, split panes, a fuzzy file finder (⌘P), a command \
         palette (⇧⌘P) with every command and its keys, and go to line.",
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
    "Terminal panel from the status bar (⌃`), with tabs that keep running when hidden",
    "Theme picker with live preview; TOML themes and VS Code theme import",
    "File icons in the tree, tabs and finder; loadable icon themes",
];

const NOT_BUILT_YET: &[&str] = &[
    "Packages and plugins (the seam is there)",
    "Autocomplete, code folding and a minimap",
    "Multiple windows and project-wide replace",
    "Encodings other than UTF-8, and files over 64 MB",
];

const PAGE: ProductPage = ProductPage {
    product: FERRUM_EDITOR,
    meta_description: "Ferrum Editor is a small, hackable code editor in the spirit of Atom, \
                       written from scratch in Rust on GPUI and configured in plain TOML.",
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
        ("Network", "None"),
        (
            "Builds",
            "macOS (Apple Silicon and Intel), Linux x86_64 and Windows",
        ),
        ("Built with", "Rust, GPUI, ropey, tree-sitter"),
        ("Tested on", "macOS (automated tests)"),
    ],
    specs_note: "Key bindings are designed for macOS; on Linux and Windows ⌘ becomes Ctrl.",
};

pub fn render(site_url: &str) -> Markup {
    sections::render(site_url, &PAGE, html! {})
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
}
