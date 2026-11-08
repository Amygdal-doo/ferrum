pub mod detail;
pub mod settings;
pub mod sidebar;

use ferrum_ui::prelude::IconName;
use ferrumtoolbox_core::AppId;

pub fn icon_for(app: AppId) -> IconName {
    match app {
        AppId::Ide => IconName::Code,
        AppId::Api => IconName::Public,
        AppId::Git => IconName::GitBranch,
        AppId::Data => IconName::DatabaseZap,
        AppId::Term => IconName::Terminal,
        AppId::Toolbox => IconName::Box,
    }
}

pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::format_size;

    #[test]
    fn formats_sizes() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(25 * 1024 * 1024), "25.0 MB");
    }
}
