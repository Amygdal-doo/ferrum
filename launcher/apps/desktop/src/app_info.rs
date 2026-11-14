//! What Ferrum Launcher tells `ferrum-ui` for onboarding and Settings: the keys
//! worth knowing, the permissions it needs.

use ferrum_ui::AppInfo;
use ferrum_ui::components::IconName;

pub fn app_info() -> AppInfo {
    AppInfo::new("Ferrum Launcher", "ferrum-launcher")
        .tagline("Installs, updates and launches every Ferrum app.")
        .icon(IconName::Download, 0x8b8b90)
        .page(crate::settings_page::LauncherPage)
        .key("Settings", "cmd-,")
        .key("Quit", "cmd-q")
}
