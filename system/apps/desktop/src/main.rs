//! FerrumSystem (FeS) desktop entry point. `ferrumsystem [folder]` opens
//! that project (its `.architecture/` folder when it has one); without one
//! the project open last, the demo at the very first start, else nothing.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod agent;
mod agent_tools;
mod app;
mod app_info;
mod canvas;
mod component;
mod dialogs;
mod generate;
mod icons;
mod layout;
mod live;
mod menus;
mod panel;
mod prefs;
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
    let config = AppConfig::new("FerrumSystem")
        .size(1440., 880.)
        .min_size(1000., 600.)
        .unified_titlebar()
        .app(app_info::app_info());
    ferrum_ui::run(config, move |window, cx| {
        ferrum_account_ui::init(ferrum_account_ui::AccountApp::System, cx);
        ferrumagent_ui::init(cx);
        app::bind_keys(cx);
        cx.set_menus(menus::menus(&app_info::app_info()));
        let options = app::Options {
            project,
            ..app::Options::default()
        };
        cx.new(|cx| app::FerrumSystem::new(options, window, cx))
    });
}
