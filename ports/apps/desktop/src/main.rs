//! FerrumPorts desktop entry point. `ferrumports [folder]` opens with that
//! folder's project selected.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod app_info;
mod chrome;
mod commands;
mod detail;
mod forwards;
mod history;
mod remote;
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
        .min_size(900., 480.)
        .unified_titlebar()
        .app(app_info::app_info());
    ferrum_ui::run(config, move |window, cx| {
        ferrum_account_ui::init(ferrum_account_ui::AccountApp::Ports, cx);
        app::bind_keys(cx);
        let options = app::Options {
            project,
            autostart: true,
            home: ferrum_ui::project::home_dir(),
        };
        cx.new(|cx| app::FerrumPorts::new(options, window, cx))
    });
}
