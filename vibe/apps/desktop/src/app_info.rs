//! What FerrumEngineer tells `ferrum-ui` for onboarding and Settings: the keys
//! worth knowing, the permissions it needs.

use ferrum_ui::components::IconName;
use ferrum_ui::{AppInfo, PermissionKind};

pub fn app_info() -> AppInfo {
    AppInfo::new("FerrumEngineer", "ferrumvibe")
        .tagline("An agent always live in your project, with Git and a terminal beside it.")
        .icon(IconName::Sparkle, 0xa970ff)
        .key("New task", "cmd-n")
        .key("New discussion", "cmd-shift-n")
        .key("Go to a file or thread", "cmd-p")
        .key("Find in files and threads", "cmd-shift-f")
        .key("Show or hide the sidebar", "cmd-\\")
        .key("Back / forward", "cmd-[")
        .key("Open a folder", "cmd-o")
        .key("Show or hide Files", "cmd-alt-1")
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
        .event(ferrumagent_ui::NEEDS_YOU)
        .event(ferrumagent_ui::DONE)
        .event(ferrumagent_ui::FAILED)
        .page(ferrumterm_ui::TerminalPage::default())
}
