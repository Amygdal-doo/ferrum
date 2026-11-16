//! What FerrumTerm tells `ferrum-ui` for onboarding, Settings and
//! notifications.

use ferrum_ui::components::IconName;
use ferrum_ui::notify::Channels;
use ferrum_ui::{AppInfo, Migration, NotificationEvent, PermissionKind};

use crate::{KeyboardPage, MachinesPage, TerminalPage};

/// FerrumTerm's [`AppInfo`]: keys, permissions, migration, notification
/// events and the Terminal / Keyboard / Machines settings pages.
pub fn app_info() -> AppInfo {
    AppInfo::new("FerrumTerm", "ferrumterm")
        .tagline("Terminals that keep running when the window closes, with agents in every pane.")
        .icon(IconName::Terminal, 0x3dd68c)
        .key("New tab", "cmd-t")
        .key("Split right", "cmd-d")
        .key("Split down", "cmd-shift-d")
        .key("Toggle sidebar", "cmd-b")
        .key("Next session", "cmd-shift-s")
        .key("Settings", "cmd-,")
        .permission(
            PermissionKind::FullDiskAccess,
            "Lets shells and tools you run read projects in Documents, Desktop and iCloud.",
        )
        .permission(
            PermissionKind::Notifications,
            "Tells you when an agent finishes, fails or needs you, even with the window closed.",
        )
        .permission(
            PermissionKind::LocalNetwork,
            "Needed for SSH and port forwards to machines on your network.",
        )
        .migration(Migration {
            from: &["iTerm2", "Ghostty", "Terminal.app", "Warp", "tmux"],
            summary: "Key bindings, font and cursor settings, and SSH hosts come over. Colour \
                      themes are chosen in Settings › Themes.",
            task: "FerrumTerm keeps its terminal settings in ~/.ferrum/ferrumterm/settings.json \
                   (fields: cursor_style block|beam|underline, force_cursor_style, cursor_blink \
                   off|slow|normal|fast, font_size in points, font_family, copy_on_select, \
                   option automatic|alt|characters|left_alt|right_alt, startup_command) and shortcut overrides in \
                   ~/.ferrum/ferrumterm/keymap.json as {\"bindings\": [{\"action\": \
                   \"ferrum_term::NewTab\", \"keys\": \"cmd-t\", \"context\": \"FerrumTerm\"}]}. \
                   Actions: ferrum_term::NewTab, NewSession, SplitRight, SplitDown, ClosePane, \
                   NextPane, PreviousPane, NextTab, PreviousTab, ActivateTab1…9, Copy, CopyScreen, \
                   Paste, IncreaseFontSize, DecreaseFontSize, ResetFontSize, NextSession, \
                   ToggleSidebar, ScrollPageUp, ScrollPageDown. Keys use GPUI syntax \
                   (cmd-shift-d, ctrl-a). Sources: ~/Library/Preferences/com.googlecode.iterm2.plist \
                   (read it with `plutil -convert json -o - …`), ~/.config/ghostty/config, \
                   ~/Library/Preferences/com.apple.Terminal.plist, ~/.warp, ~/.tmux.conf. \
                   tmux prefix bindings have no counterpart: list them under could not migrate \
                   and point to `fterm` sessions. SSH hosts stay in ~/.ssh/config and are added in \
                   Settings › Machines; do not copy them.",
        })
        .event(NotificationEvent {
            id: "working",
            title: "An agent started working",
            icon: IconName::LoadCircle,
            tint: 0x4c8bf5,
            defaults: Channels::default(),
        })
        .event(NotificationEvent {
            id: "blocked",
            title: "An agent needs you",
            icon: IconName::Warning,
            tint: 0xf5a524,
            defaults: Channels::ALL,
        })
        .event(NotificationEvent {
            id: "error",
            title: "An agent failed",
            icon: IconName::XCircle,
            tint: 0xf2555a,
            defaults: Channels::ALL,
        })
        .event(NotificationEvent {
            id: "done",
            title: "An agent finished",
            icon: IconName::CircleCheck,
            tint: 0x3dd68c,
            defaults: Channels::ALL,
        })
        // The Agent pane's chat.
        .event(ferrumagent_ui::NEEDS_YOU)
        .event(ferrumagent_ui::DONE)
        .event(ferrumagent_ui::FAILED)
        .page(TerminalPage::default())
        .page(KeyboardPage)
        .page(MachinesPage::default())
}
