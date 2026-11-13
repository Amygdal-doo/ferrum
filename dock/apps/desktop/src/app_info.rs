//! What FerrumContainers tells `ferrum-ui` for onboarding and Settings: the keys
//! worth knowing, the permissions it needs.

use ferrum_ui::components::IconName;
use ferrum_ui::{AppInfo, PermissionKind};

pub fn app_info() -> AppInfo {
    AppInfo::new("FerrumContainers", "ferrumdock")
        .tagline("Containers, images, volumes and logs for Docker, OrbStack, Colima and Podman.")
        .icon(IconName::Box, 0x2496ed)
        .key("Refresh", "cmd-r")
        .key("Filter", "cmd-f")
        .key("Switch project", "cmd-alt-o")
        .key("Settings", "cmd-,")
        .permission(
            PermissionKind::Automation,
            "Opens a shell inside a container in Terminal when FerrumTerm isn't installed.",
        )
}
