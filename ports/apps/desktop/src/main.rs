//! FerrumPorts desktop entry point. `ferrumports [folder]` opens with that
//! folder's project selected.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod chrome;
mod commands;
mod detail;
mod table;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use ferrum_ui::AppConfig;
use ferrum_ui::gpui::AppContext as _;

fn main() {
    let project = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .and_then(|p| std::fs::canonicalize(p).ok())
        .filter(|p| p.is_dir());
    let config = AppConfig::new("FerrumPorts")
        .size(1240., 760.)
        .min_size(900., 480.);
    ferrum_ui::run(config, move |window, cx| {
        app::bind_keys(cx);
        let options = app::Options {
            project,
            autostart: true,
        };
        cx.new(|cx| app::FerrumPorts::new(options, window, cx))
    });
}
