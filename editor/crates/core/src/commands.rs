//! The command catalogue: every action has a stable `namespace:command`
//! name used by the palette, the keymap and the menus. Default key
//! bindings are written for macOS (`cmd`); other platforms get `cmd`
//! translated to `ctrl` (see [`crate::keymap`]).
//!
//! Extensions (a future package system) add [`CommandInfo`]s through a
//! [`CommandSource`]; the desktop app pairs every name with a handler.

/// Where a command's key bindings are active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Scope {
    /// Anywhere in the window.
    Workspace,
    /// Only while a text editor has focus.
    Editor,
    /// Only while the tree view has focus.
    TreeView,
    /// Only while the Term pane (FerrumTerm's view) has focus.
    Terminal,
}

impl Scope {
    /// GPUI key context for bindings in this scope.
    pub fn key_context(self) -> Option<&'static str> {
        match self {
            Scope::Workspace => None,
            Scope::Editor => Some("FerrumEditor"),
            Scope::TreeView => Some("FerrumTreeView"),
            Scope::Terminal => Some("FerrumTerm"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandInfo {
    pub name: String,
    pub scope: Scope,
    pub default_keys: Vec<String>,
}

impl CommandInfo {
    /// "editor:duplicate-lines" → "Editor: Duplicate Lines".
    pub fn title(&self) -> String {
        title_for(&self.name)
    }
}

/// Supplies commands to the registry. Built-ins implement it today; a
/// package system would add more sources.
pub trait CommandSource {
    fn commands(&self) -> Vec<CommandInfo>;
}

pub fn title_for(name: &str) -> String {
    let words = |s: &str| {
        s.split(['-', '_'])
            .filter(|w| !w.is_empty())
            .map(|w| {
                // Names whose spelling isn't just a capital first letter.
                if w == "vscode" {
                    return "VS Code".to_string();
                }
                let mut chars = w.chars();
                chars
                    .next()
                    .map(|c| c.to_uppercase().chain(chars).collect::<String>())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    match name.split_once(':') {
        Some((ns, cmd)) => format!("{}: {}", words(ns), words(cmd)),
        None => words(name),
    }
}

use Scope::{Editor as E, TreeView as T, Workspace as W};

/// (name, scope, default keys).
const BUILTIN: &[(&str, Scope, &[&str])] = &[
    // Application / files
    ("application:new-file", W, &["cmd-n"]),
    ("application:open-file", W, &["cmd-o"]),
    ("application:open-folder", W, &["cmd-shift-o"]),
    ("application:new-window", W, &["cmd-shift-n"]),
    // ⌘, opens the Settings window, as in every Ferrum app; the config
    // file is ⌥⌘,.
    ("application:open-settings", W, &["cmd-,"]),
    ("application:open-config", W, &["cmd-alt-,"]),
    ("application:open-keymap", W, &[]),
    ("application:open-snippets", W, &[]),
    ("application:open-theme", W, &[]),
    ("application:about", W, &[]),
    ("application:quit", W, &["cmd-q"]),
    ("core:save", W, &["cmd-s"]),
    ("core:save-as", W, &["cmd-shift-s"]),
    ("core:save-all", W, &["cmd-alt-s"]),
    ("core:close-tab", W, &["cmd-w"]),
    ("core:reopen-closed-tab", W, &["cmd-shift-t"]),
    ("core:reload-file", W, &[]),
    // Panes and tabs
    ("pane:next-tab", W, &["ctrl-tab", "cmd-shift-]"]),
    ("pane:previous-tab", W, &["ctrl-shift-tab", "cmd-shift-["]),
    ("pane:split-right", W, &["cmd-k right"]),
    ("pane:split-down", W, &["cmd-k down"]),
    ("pane:close", W, &["cmd-k cmd-w"]),
    ("pane:focus-next", W, &["cmd-k cmd-right"]),
    ("pane:focus-previous", W, &["cmd-k cmd-left"]),
    // Panels and pickers
    ("tree-view:toggle", W, &["cmd-\\"]),
    ("tree-view:reveal-active-file", W, &["cmd-shift-\\"]),
    ("tree-view:focus", W, &["ctrl-0"]),
    ("tree-view:new-file", T, &["a"]),
    ("tree-view:new-folder", T, &["shift-a"]),
    ("tree-view:rename", T, &["f2", "m"]),
    ("tree-view:delete", T, &["delete", "backspace"]),
    ("tree-view:collapse-all", T, &[]),
    ("tree-view:select-next", T, &["down"]),
    ("tree-view:select-previous", T, &["up"]),
    ("tree-view:expand", T, &["right"]),
    ("tree-view:collapse", T, &["left"]),
    ("tree-view:open-selected", T, &["enter", "space"]),
    ("tree-view:reveal-in-finder", T, &[]),
    ("fuzzy-finder:toggle-file-finder", W, &["cmd-p", "cmd-t"]),
    ("command-palette:toggle", W, &["cmd-shift-p"]),
    ("go-to-line:toggle", W, &["ctrl-g"]),
    ("find-and-replace:show", W, &["cmd-f"]),
    ("find-and-replace:show-replace", W, &["cmd-alt-f"]),
    ("find-and-replace:find-next", W, &["cmd-g"]),
    ("find-and-replace:find-previous", W, &["cmd-shift-g"]),
    ("find-and-replace:use-selection-as-find", E, &["cmd-e"]),
    ("find-in-files:show", W, &["cmd-shift-f"]),
    ("markdown-preview:toggle", W, &["cmd-shift-m"]),
    // Tool panes inside the window (Git, Agent, Term, CI, Logs, Debug)
    ("git:toggle-view", W, &["cmd-alt-g"]),
    ("git:show-panel", W, &["ctrl-shift-g"]),
    ("git:open-in-ferrumgit", W, &[]),
    ("tools:close-all", W, &[]),
    ("tools:reset-layout", W, &[]),
    // CI page: GitHub Actions runs, logs and PR checks (ferrumci-core)
    ("ci:toggle-view", W, &["cmd-shift-i"]),
    ("ci:refresh", W, &[]),
    ("ci:search-log", W, &[]),
    // Logs page: live logs per project (ferrumlogs-core)
    ("logs:toggle-view", W, &["cmd-alt-l"]),
    ("logs:add-source", W, &[]),
    ("logs:stop-all", W, &[]),
    // Tasks (build, test, run) in the terminal
    ("tasks:build", W, &["cmd-b"]),
    ("tasks:run", W, &["cmd-shift-b"]),
    ("tasks:test", W, &[]),
    ("tasks:rerun-last", W, &[]),
    // The Agent pane (FerrumChat) and CLI agents in the terminal
    ("agents:toggle-panel", W, &["cmd-alt-a"]),
    ("agents:new-session", W, &[]),
    // Debugger (DAP)
    ("debug:start", W, &[]),
    ("debug:continue", W, &["f5"]),
    ("debug:stop", W, &["shift-f5"]),
    ("debug:restart", W, &["cmd-shift-f5"]),
    ("debug:pause", W, &["f6"]),
    ("debug:step-over", W, &["f10"]),
    ("debug:step-into", W, &["f11"]),
    ("debug:step-out", W, &["shift-f11"]),
    ("debug:toggle-breakpoint", W, &["f9"]),
    ("debug:remove-all-breakpoints", W, &[]),
    ("debug:toggle-panel", W, &["cmd-shift-y"]),
    ("debug:add-watch", W, &[]),
    ("debug:install-adapter", W, &[]),
    // Language servers
    ("problems:show", W, &["ctrl-shift-m"]),
    ("language-server:restart", W, &[]),
    ("language-server:install", W, &[]),
    ("language-server:show-install-log", W, &[]),
    ("navigation:back", W, &["ctrl--"]),
    ("navigation:forward", W, &["ctrl-shift--"]),
    // The Term pane (FerrumTerm); its own keys apply while it has focus
    ("terminal:toggle", W, &["ctrl-`"]),
    ("terminal:new", W, &["ctrl-shift-`"]),
    ("terminal:open-in-ferrumterm", W, &[]),
    // View
    ("window:increase-font-size", W, &["cmd-=", "cmd-+"]),
    ("window:decrease-font-size", W, &["cmd--"]),
    ("window:reset-font-size", W, &["cmd-0"]),
    ("theme:use-dark", W, &[]),
    ("theme:use-light", W, &[]),
    ("theme:select", W, &["cmd-k cmd-t"]),
    ("theme:install-from-file", W, &[]),
    ("theme:import-vscode-theme", W, &[]),
    ("theme:open-themes-folder", W, &[]),
    ("theme:reload", W, &[]),
    ("icon-theme:select", W, &[]),
    ("icon-theme:open-folder", W, &[]),
    ("icon-theme:reload", W, &[]),
    ("editor:toggle-soft-wrap", W, &["alt-z"]),
    ("editor:toggle-line-numbers", W, &[]),
    ("editor:toggle-indent-guides", W, &[]),
    ("editor:toggle-minimap", W, &[]),
    // Editing
    ("editor:undo", E, &["cmd-z"]),
    ("editor:redo", E, &["cmd-shift-z", "cmd-y"]),
    ("editor:copy", E, &["cmd-c"]),
    ("editor:cut", E, &["cmd-x"]),
    ("editor:paste", E, &["cmd-v"]),
    ("editor:select-all", E, &["cmd-a"]),
    ("editor:select-line", E, &["cmd-l"]),
    ("editor:newline", E, &["enter", "shift-enter"]),
    ("editor:newline-below", E, &["cmd-enter"]),
    ("editor:newline-above", E, &["cmd-shift-enter"]),
    ("editor:backspace", E, &["backspace", "shift-backspace"]),
    ("editor:delete", E, &["delete"]),
    ("editor:delete-word-left", E, &["alt-backspace"]),
    ("editor:delete-word-right", E, &["alt-delete"]),
    ("editor:delete-to-line-start", E, &["cmd-backspace"]),
    ("editor:delete-to-line-end", E, &["cmd-delete"]),
    ("editor:delete-line", E, &["ctrl-shift-k"]),
    ("editor:tab", E, &["tab"]),
    ("editor:shift-tab", E, &["shift-tab"]),
    ("editor:indent", E, &["cmd-]"]),
    ("editor:outdent", E, &["cmd-["]),
    ("editor:toggle-comment", E, &["cmd-/"]),
    ("editor:duplicate-lines", E, &["cmd-shift-d"]),
    ("editor:move-line-up", E, &["ctrl-cmd-up"]),
    ("editor:move-line-down", E, &["ctrl-cmd-down"]),
    ("editor:join-lines", E, &["cmd-j"]),
    ("editor:upper-case", E, &["cmd-k cmd-u"]),
    ("editor:lower-case", E, &["cmd-k cmd-l"]),
    ("editor:select-next", E, &["cmd-d"]),
    ("editor:select-all-occurrences", E, &["cmd-ctrl-g"]),
    ("editor:split-selection-into-lines", E, &["cmd-shift-l"]),
    ("editor:add-cursor-above", E, &["ctrl-shift-up"]),
    ("editor:add-cursor-below", E, &["ctrl-shift-down"]),
    ("editor:go-to-matching-bracket", E, &["ctrl-m"]),
    // Code folding (regions from tree-sitter)
    ("editor:fold", E, &["cmd-alt-["]),
    ("editor:unfold", E, &["cmd-alt-]"]),
    ("editor:toggle-fold", E, &[]),
    ("editor:fold-all", E, &["cmd-k cmd-0"]),
    ("editor:unfold-all", E, &["cmd-k cmd-j"]),
    ("editor:cancel", E, &["escape"]),
    ("editor:show-character-palette", E, &["ctrl-cmd-space"]),
    ("editor:show-completions", E, &["ctrl-space", "cmd-i"]),
    ("editor:go-to-definition", E, &["f12"]),
    ("editor:rename-symbol", E, &["f2"]),
    ("editor:find-references", E, &["shift-f12"]),
    ("editor:show-code-actions", E, &["cmd-."]),
    ("editor:format-document", E, &["shift-alt-f"]),
    ("editor:show-signature-help", E, &["cmd-shift-space"]),
    ("editor:show-hover", E, &["cmd-k cmd-i"]),
    // Agent edit: prompt, then the answer to accept (⌘Enter) or reject
    // (Escape). ⌘K alone also opens it (see the editor's key handler: a
    // binding for ⌘K would hide the ⌘K chords).
    ("editor:inline-edit", E, &[]),
    ("editor:accept-inline-edit", E, &[]),
    ("editor:reject-inline-edit", E, &[]),
    ("editor:go-to-next-problem", E, &["f8"]),
    ("editor:go-to-previous-problem", E, &["shift-f8"]),
    // Motions
    ("editor:move-left", E, &["left", "ctrl-b"]),
    ("editor:move-right", E, &["right", "ctrl-f"]),
    ("editor:move-up", E, &["up", "ctrl-p"]),
    ("editor:move-down", E, &["down", "ctrl-n"]),
    ("editor:select-left", E, &["shift-left"]),
    ("editor:select-right", E, &["shift-right"]),
    ("editor:select-up", E, &["shift-up"]),
    ("editor:select-down", E, &["shift-down"]),
    ("editor:move-to-previous-word", E, &["alt-left"]),
    ("editor:move-to-next-word", E, &["alt-right"]),
    ("editor:select-to-previous-word", E, &["alt-shift-left"]),
    ("editor:select-to-next-word", E, &["alt-shift-right"]),
    (
        "editor:move-to-line-start",
        E,
        &["cmd-left", "home", "ctrl-a"],
    ),
    (
        "editor:move-to-line-end",
        E,
        &["cmd-right", "end", "ctrl-e"],
    ),
    (
        "editor:select-to-line-start",
        E,
        &["cmd-shift-left", "shift-home"],
    ),
    (
        "editor:select-to-line-end",
        E,
        &["cmd-shift-right", "shift-end"],
    ),
    ("editor:move-to-top", E, &["cmd-up", "cmd-home"]),
    ("editor:move-to-bottom", E, &["cmd-down", "cmd-end"]),
    (
        "editor:select-to-top",
        E,
        &["cmd-shift-up", "cmd-shift-home"],
    ),
    (
        "editor:select-to-bottom",
        E,
        &["cmd-shift-down", "cmd-shift-end"],
    ),
    ("editor:page-up", E, &["pageup"]),
    ("editor:page-down", E, &["pagedown"]),
    ("editor:select-page-up", E, &["shift-pageup"]),
    ("editor:select-page-down", E, &["shift-pagedown"]),
];

pub struct Builtins;

impl CommandSource for Builtins {
    fn commands(&self) -> Vec<CommandInfo> {
        BUILTIN
            .iter()
            .map(|(name, scope, keys)| CommandInfo {
                name: (*name).to_string(),
                scope: *scope,
                default_keys: keys.iter().map(|k| (*k).to_string()).collect(),
            })
            .collect()
    }
}

/// All built-in commands.
pub fn builtin_commands() -> Vec<CommandInfo> {
    Builtins.commands()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn names_are_unique_and_well_formed() {
        let commands = builtin_commands();
        let mut seen = HashSet::new();
        for command in &commands {
            assert!(
                seen.insert(command.name.clone()),
                "duplicate {}",
                command.name
            );
            let (ns, cmd) = command.name.split_once(':').expect("namespace:command");
            assert!(!ns.is_empty() && !cmd.is_empty());
            assert!(
                command
                    .name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-' || c == ':')
            );
        }
        assert!(commands.len() > 90);
    }

    #[test]
    fn titles_are_readable() {
        assert_eq!(
            title_for("editor:duplicate-lines"),
            "Editor: Duplicate Lines"
        );
        assert_eq!(title_for("tree-view:toggle"), "Tree View: Toggle");
        assert_eq!(title_for("plain"), "Plain");
        assert_eq!(
            title_for("theme:import-vscode-theme"),
            "Theme: Import VS Code Theme"
        );
    }

    #[test]
    fn scopes_map_to_key_contexts() {
        assert_eq!(Scope::Workspace.key_context(), None);
        assert_eq!(Scope::Editor.key_context(), Some("FerrumEditor"));
        assert_eq!(Scope::Terminal.key_context(), Some("FerrumTerm"));
    }
}
