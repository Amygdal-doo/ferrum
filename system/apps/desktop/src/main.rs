//! Ferrum System (FeS) desktop entry point. `ferrumsystem [folder]` opens
//! that project (its `.architecture/` folder when it has one).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod app_info;
mod canvas;
mod component;
mod dialogs;
mod icons;
mod layout;
mod live;
mod panel;
mod sidebar;
#[cfg(test)]
mod tests;
mod view;

use std::path::PathBuf;

use ferrum_ui::AppConfig;
use ferrum_ui::gpui::AppContext as _;

fn main() {
    let project = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .and_then(|p| std::fs::canonicalize(p).ok())
        .filter(|p| p.is_dir());
    let config = AppConfig::new("Ferrum System")
        .size(1440., 880.)
        .min_size(1000., 600.)
        .unified_titlebar()
        .app(app_info::app_info());
    ferrum_ui::run(config, move |window, cx| {
        app::bind_keys(cx);
        cx.new(|cx| app::FerrumSystem::new(project, window, cx))
    });
}
