pub mod detail;
pub mod logo;
pub mod onboarding;
pub mod overview;

use ferrum_ui::gpui::Hsla;
use ferrum_ui::prelude::*;
use ferrumlauncher_core::{AppStatus, Channel};

/// A `ferrum_ui::components::IconName` from its variant name (`ToolHammer`) or its snake_case
/// form (`tool_hammer`); unknown names get a generic icon.
pub fn icon_for(name: &str) -> IconName {
    let mut snake = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                snake.push('_');
            }
            snake.push(c.to_ascii_lowercase());
        } else {
            snake.push(c);
        }
    }
    snake.parse().unwrap_or(IconName::Box)
}

pub fn status_color(status: AppStatus, cx: &App) -> Hsla {
    let status_colors = cx.theme().status();
    match status {
        AppStatus::Planned | AppStatus::Deprecated => cx.theme().colors().text_muted,
        AppStatus::InDevelopment => status_colors.info,
        AppStatus::Alpha => status_colors.warning,
        AppStatus::Beta => status_colors.modified,
        AppStatus::Stable => status_colors.success,
    }
}

pub fn status_badge(status: AppStatus, cx: &App) -> impl IntoElement {
    badge(status.label(), status_color(status, cx), cx)
}

/// Shown instead of the status for an installed app the suite dropped.
pub fn discontinued_badge(cx: &App) -> impl IntoElement {
    badge("No longer offered", cx.theme().colors().text_muted, cx)
}

pub fn channel_badge(channel: Channel, cx: &App) -> impl IntoElement {
    let status_colors = cx.theme().status();
    let color = match channel {
        Channel::Stable => status_colors.success,
        Channel::Beta => status_colors.modified,
        Channel::Alpha => status_colors.warning,
    };
    badge(channel.label(), color, cx)
}

pub fn format_date(time: Option<chrono::DateTime<chrono::Utc>>) -> String {
    time.map(|t| {
        t.with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string()
    })
    .unwrap_or_else(|| "—".into())
}

pub fn format_count(count: u64) -> String {
    match count {
        0..=999 => count.to_string(),
        1_000..=999_999 => format!("{:.1}k", count as f64 / 1_000.0),
        _ => format!("{:.1}M", count as f64 / 1_000_000.0),
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
    use super::{IconName, format_count, format_size, icon_for};

    #[test]
    fn icons_by_name() {
        assert_eq!(icon_for("ToolHammer"), IconName::ToolHammer);
        assert_eq!(icon_for("tool_hammer"), IconName::ToolHammer);
        assert_eq!(icon_for("DatabaseZap"), IconName::DatabaseZap);
        assert_eq!(icon_for("NoSuchIcon"), IconName::Box);
        assert_eq!(icon_for(""), IconName::Box);
    }

    #[test]
    fn formats_counts() {
        assert_eq!(format_count(7), "7");
        assert_eq!(format_count(1_500), "1.5k");
        assert_eq!(format_count(2_000_000), "2.0M");
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(25 * 1024 * 1024), "25.0 MB");
    }
}
