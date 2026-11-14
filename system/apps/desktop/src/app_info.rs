//! What Ferrum System tells `ferrum-ui` for onboarding and Settings.

use ferrum_ui::AppInfo;
use ferrum_ui::components::IconName;

pub fn app_info() -> AppInfo {
    AppInfo::new("Ferrum System", "ferrumsystem")
        .tagline("Design a system: layers, services on any cloud, data, flows and live pods.")
        .icon(IconName::Blocks, 0x14b8a6)
        .key("Zoom in / out", "cmd-= / cmd--")
        .key("Fit diagram", "cmd-0")
        .key("Delete component", "backspace")
        .key("Settings", "cmd-,")
}
