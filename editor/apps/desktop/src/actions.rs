//! One GPUI action, [`RunCommand`], carries a command name. Every key
//! binding and menu item dispatches it; views handle the names they know
//! and let the rest bubble up to the workspace's registry.

use ferrumeditor_core::commands::Scope;
use ferrumeditor_core::keymap::Keymap;
use gpui::{App, KeyBinding, Keystroke, Menu, MenuItem, NoAction, SharedString};

use crate::settings::MAC;

#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(namespace = ferrum_editor, no_json)]
pub struct RunCommand {
    pub name: SharedString,
}

pub fn run(name: &'static str) -> RunCommand {
    RunCommand { name: name.into() }
}

/// Bindings present before ours (ferrum-ui text inputs, menus), restored
/// when the keymap reloads. Quit is dropped: `application:quit` replaces it
/// so unsaved changes are checked first.
pub struct BaseBindings(pub Vec<KeyBinding>);

impl gpui::Global for BaseBindings {}

pub fn capture_base_bindings(cx: &mut App) {
    let bindings: Vec<KeyBinding> = cx
        .key_bindings()
        .borrow()
        .bindings()
        .filter(|b| b.action().name() != "ferrum::Quit")
        .cloned()
        .collect();
    cx.set_global(BaseBindings(bindings));
}

/// Replaces all key bindings with the base ones plus `keymap`. Returns
/// messages for bindings GPUI cannot parse.
pub fn apply_keymap(keymap: &Keymap, cx: &mut App) -> Vec<String> {
    let base = cx
        .try_global::<BaseBindings>()
        .map(|b| b.0.clone())
        .unwrap_or_default();
    let mut problems = Vec::new();
    let mut ours = Vec::new();
    for binding in &keymap.bindings {
        if let Err(error) = binding
            .keys
            .split_whitespace()
            .try_for_each(|k| Keystroke::parse(k).map(|_| ()))
        {
            problems.push(format!("keymap.toml: `{}`: {error}", binding.keys));
            continue;
        }
        ours.push(KeyBinding::new(
            &binding.keys,
            RunCommand {
                name: binding.command.clone().into(),
            },
            binding.scope.key_context(),
        ));
    }
    // Fixed bindings outside the user keymap: Shift+Enter in the find bar.
    ours.push(KeyBinding::new(
        "shift-enter",
        RunCommand {
            name: "find-and-replace:find-previous".into(),
        },
        Some("FerrumFindBar"),
    ));
    ours.extend(terminal_passthrough(keymap));
    cx.clear_key_bindings();
    cx.bind_keys(base);
    cx.bind_keys(ours);
    problems
}

/// Whether a focused terminal should get `keys` instead of a window-wide
/// binding: everything except Cmd shortcuts on macOS, and everything
/// except Ctrl+Shift shortcuts elsewhere.
pub fn terminal_owns(keys: &str) -> bool {
    let first = keys.split_whitespace().next().unwrap_or_default();
    let mods: Vec<&str> = first.split('-').collect();
    let has = |m: &str| mods[..mods.len().saturating_sub(1)].contains(&m);
    if MAC {
        !has("cmd")
    } else {
        !(has("ctrl") && has("shift"))
    }
}

/// `NoAction` bindings in the terminal's key context that hide
/// window-wide bindings the shell needs (Ctrl+G, Ctrl+Tab, Alt+Z, Escape…),
/// so those keys reach the terminal's key handler.
fn terminal_passthrough(keymap: &Keymap) -> Vec<KeyBinding> {
    let context = Scope::Terminal.key_context();
    let mut keys: Vec<&str> = keymap
        .bindings
        .iter()
        .filter(|b| b.scope == Scope::Workspace && !b.command.starts_with("terminal:"))
        .map(|b| b.keys.as_str())
        .filter(|keys| terminal_owns(keys))
        // Unparseable keys are reported by `apply_keymap`; never bind them.
        .filter(|keys| keys.split_whitespace().all(|k| Keystroke::parse(k).is_ok()))
        .collect();
    keys.push("escape");
    keys.sort_unstable();
    keys.dedup();
    keys.into_iter()
        .map(|keys| KeyBinding::new(keys, NoAction, context))
        .collect()
}

fn item(title: &'static str, command: &'static str) -> MenuItem {
    MenuItem::action(title, run(command))
}

pub fn set_menus(cx: &mut App) {
    cx.set_menus([
        Menu::new("Ferrum Editor").items([
            item("About Ferrum Editor", "application:about"),
            MenuItem::separator(),
            item("Settings…", "application:open-config"),
            item("Keymap…", "application:open-keymap"),
            item("Snippets…", "application:open-snippets"),
            item("Theme Overrides…", "application:open-theme"),
            item("Select Theme…", "theme:select"),
            item("Select Icon Theme…", "icon-theme:select"),
            item("Install Theme from File…", "theme:install-from-file"),
            item("Open Themes Folder", "theme:open-themes-folder"),
            MenuItem::separator(),
            item("Quit Ferrum Editor", "application:quit"),
        ]),
        Menu::new("File").items([
            item("New File", "application:new-file"),
            item("Open File…", "application:open-file"),
            item("Open Folder…", "application:open-folder"),
            MenuItem::separator(),
            item("Save", "core:save"),
            item("Save As…", "core:save-as"),
            item("Save All", "core:save-all"),
            item("Reload File", "core:reload-file"),
            MenuItem::separator(),
            item("Reopen Closed Tab", "core:reopen-closed-tab"),
            item("Close Tab", "core:close-tab"),
            item("Close Pane", "pane:close"),
        ]),
        Menu::new("Edit").items([
            item("Undo", "editor:undo"),
            item("Redo", "editor:redo"),
            MenuItem::separator(),
            item("Cut", "editor:cut"),
            item("Copy", "editor:copy"),
            item("Paste", "editor:paste"),
            item("Select All", "editor:select-all"),
            MenuItem::separator(),
            item("Toggle Comment", "editor:toggle-comment"),
            item("Duplicate Lines", "editor:duplicate-lines"),
            item("Delete Line", "editor:delete-line"),
            item("Move Line Up", "editor:move-line-up"),
            item("Move Line Down", "editor:move-line-down"),
            item("Join Lines", "editor:join-lines"),
            item("Indent", "editor:indent"),
            item("Outdent", "editor:outdent"),
            item("Upper Case", "editor:upper-case"),
            item("Lower Case", "editor:lower-case"),
            MenuItem::separator(),
            item("Find…", "find-and-replace:show"),
            item("Replace…", "find-and-replace:show-replace"),
            item("Find Next", "find-and-replace:find-next"),
            item("Find Previous", "find-and-replace:find-previous"),
            item("Find in Files…", "find-in-files:show"),
        ]),
        Menu::new("Selection").items([
            item("Select Line", "editor:select-line"),
            item("Select Next Occurrence", "editor:select-next"),
            item("Select All Occurrences", "editor:select-all-occurrences"),
            item(
                "Split Selection into Lines",
                "editor:split-selection-into-lines",
            ),
            item("Add Cursor Above", "editor:add-cursor-above"),
            item("Add Cursor Below", "editor:add-cursor-below"),
            item("Go to Matching Bracket", "editor:go-to-matching-bracket"),
        ]),
        Menu::new("View").items([
            item("Command Palette…", "command-palette:toggle"),
            item("Go to File…", "fuzzy-finder:toggle-file-finder"),
            item("Go to Line…", "go-to-line:toggle"),
            MenuItem::separator(),
            item("Toggle Tree View", "tree-view:toggle"),
            item("Reveal Active File", "tree-view:reveal-active-file"),
            item("Markdown Preview", "markdown-preview:toggle"),
            item("Toggle Terminal", "terminal:toggle"),
            item("New Terminal", "terminal:new"),
            item("Open in FerrumTerm", "terminal:open-in-ferrumterm"),
            MenuItem::separator(),
            item("Split Right", "pane:split-right"),
            item("Split Down", "pane:split-down"),
            item("Next Tab", "pane:next-tab"),
            item("Previous Tab", "pane:previous-tab"),
            MenuItem::separator(),
            item("Toggle Soft Wrap", "editor:toggle-soft-wrap"),
            item("Toggle Line Numbers", "editor:toggle-line-numbers"),
            item("Toggle Indent Guides", "editor:toggle-indent-guides"),
            item("Increase Font Size", "window:increase-font-size"),
            item("Decrease Font Size", "window:decrease-font-size"),
            item("Reset Font Size", "window:reset-font-size"),
            MenuItem::separator(),
            item("Dark Theme", "theme:use-dark"),
            item("Light Theme", "theme:use-light"),
            item("Select Theme…", "theme:select"),
            item("Select Icon Theme…", "icon-theme:select"),
        ]),
    ]);
}
