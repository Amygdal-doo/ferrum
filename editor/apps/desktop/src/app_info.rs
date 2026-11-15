//! What FerrumIDE tells `ferrum-ui` for onboarding and Settings: the keys
//! worth knowing, the permissions it needs and where users migrate from.

use ferrum_ui::components::IconName;
use ferrum_ui::{AppInfo, Migration, PermissionKind};

pub fn app_info() -> AppInfo {
    AppInfo::new("FerrumIDE", "ferrumeditor")
        .tagline("A fast, hackable code editor with Git, an agent and terminals beside your code.")
        .icon(IconName::Code, 0xe8743b)
        .key("Command palette", "cmd-shift-p")
        .key("Go to file", "cmd-p")
        .key("Open folder", "cmd-shift-o")
        .key("Find in files", "cmd-shift-f")
        .key("Split right", "cmd-k right")
        .key("Settings", "cmd-,")
        .permission(
            PermissionKind::FullDiskAccess,
            "Lets the editor, its language servers and terminals open projects in Documents, Desktop and iCloud.",
        )
        .permission(
            PermissionKind::Notifications,
            "CI runs, long tasks and the agent tell you when they finish or need you.",
        )
        .migration(Migration {
            from: &["VS Code", "Cursor", "Zed"],
            summary: "Settings, key bindings and themes come over; extensions do not.",
            task: "FerrumIDE keeps its settings in ~/.ferrum/editor/config.toml (keys include font_family, font_size, line_height, theme, tab_size, soft_tabs, soft_wrap, show_line_numbers, format_on_save, trim_trailing_whitespace, ensure_final_newline, terminal_shell, and [languages.<name>] tables for per-language tab_size / soft_tabs / soft_wrap). Edit that file in place, keeping its comments; it lists every key with its default. Key bindings go in ~/.ferrum/editor/keymap.toml under [bindings] as \"keys\" = \"command:name\" (keys like cmd-shift-p or cmd-k right; command names are the editor's, e.g. command-palette:toggle, fuzzy-finder:toggle-file-finder, pane:split-right; run `ferrumeditor --help` and read the defaults in keymap.toml to find them). VS Code themes: copy a theme's .json into ~/.ferrum/editor/themes/ (VS Code JSON themes load as-is). Sources: ~/Library/Application Support/Code/User/settings.json and keybindings.json, the same under Cursor/User, and ~/.config/zed/settings.json and keymap.json. Extensions have no counterpart: list them under could not migrate.",
        })
        .event(ferrumagent_ui::NEEDS_YOU)
        .event(ferrumagent_ui::DONE)
        .event(ferrumagent_ui::FAILED)
        .page(ferrumterm_ui::TerminalPage::default())
}
