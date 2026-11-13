//! What FerrumKit tells `ferrum-ui` for onboarding and Settings: the keys
//! worth knowing, the permissions it needs.

use ferrum_ui::AppInfo;
use ferrum_ui::components::IconName;

pub fn app_info() -> AppInfo {
    AppInfo::new("FerrumKit", "ferrumkit")
        .tagline("Everyday developer utilities: JSON, JWT, Base64, hashes, regex, diffs and more.")
        .icon(IconName::ToolHammer, 0xf0b43c)
        .key("Search tools", "cmd-k")
        .key("Settings", "cmd-,")
}
