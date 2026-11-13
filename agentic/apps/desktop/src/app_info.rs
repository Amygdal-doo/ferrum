//! What FerrumAgentic tells `ferrum-ui` for onboarding and Settings: the keys
//! worth knowing, the permissions it needs.

use ferrum_ui::components::IconName;
use ferrum_ui::{AppInfo, PermissionKind};

pub fn app_info() -> AppInfo {
    AppInfo::new("FerrumAgentic", "ferrumagentic")
        .tagline("Chat, Git and a terminal side by side for agentic development.")
        .icon(IconName::Sparkle, 0xa970ff)
        .key("Show or hide chat", "cmd-alt-1")
        .key("Show or hide Git", "cmd-alt-2")
        .key("Show or hide terminal", "cmd-alt-3")
        .key("Switch project", "cmd-alt-o")
        .key("Settings", "cmd-,")
        .permission(
            PermissionKind::FullDiskAccess,
            "Lets agents, Git and the terminal work in projects in Documents, Desktop and iCloud.",
        )
        .permission(
            PermissionKind::Notifications,
            "Tells you when an agent finishes or needs you.",
        )
        .page(ferrumterm_ui::TerminalPage::default())
}
