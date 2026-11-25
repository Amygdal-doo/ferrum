//! What FerrumPorts tells `ferrum-ui` for onboarding and Settings: the keys
//! worth knowing, the permissions it needs.

use ferrum_ui::AppInfo;
use ferrum_ui::components::IconName;

pub fn app_info() -> AppInfo {
    AppInfo::new("FerrumPorts", "ferrumports")
        .tagline("Every listening port, the process behind it and the project it belongs to.")
        .icon(IconName::Server, 0x7cc85a)
        .key("Refresh", "cmd-r")
        .key("Search", "cmd-f")
        .key("Go to a port", "cmd-p")
        .key("Find in command lines", "cmd-shift-f")
        .key("Pause updates", "cmd-shift-p")
        .key("Settings", "cmd-,")
}
