//! The macOS menu bar every Ferrum app shares: the app menu (About, Check
//! for Updates…, Settings…, Hide, Quit), File, Edit, View, Window and Help.
//! `ferrum_ui::run` installs [`standard`]; apps with menus of their own
//! (Ferrum Editor) splice in [`app_menu`], [`window_menu`] and [`help_menu`].

use gpui::{App, KeyBinding, Menu, MenuItem, OsAction};

use crate::app_info::{AppInfo, app_info};
use crate::settings::{OpenSettings, ShowOnboarding};
use crate::{Quit, ToggleProjectSwitcher};

gpui::actions!(
    ferrum,
    [
        /// Show the About page of Settings.
        About,
        /// Open Ferrum Launcher, which installs updates.
        CheckForUpdates,
        /// Hide this app.
        HideApp,
        /// Hide every other app.
        HideOthers,
        /// Show every hidden app.
        ShowAll,
        /// Minimize the active window.
        Minimize,
        /// Zoom (maximize) the active window.
        Zoom,
        /// Enter or leave full screen.
        ToggleFullScreen,
        /// Close the active window.
        CloseWindow,
        /// Larger interface text.
        LargerText,
        /// Smaller interface text.
        SmallerText,
        /// Open this app's page on the Ferrum website.
        OpenHelp,
        /// Open the Ferrum changelog.
        OpenReleaseNotes,
        /// Report a problem by email.
        ReportIssue,
        /// Edit › Cut in whatever has focus (text fields, terminals, …).
        EditCut,
        /// Edit › Copy in whatever has focus.
        EditCopy,
        /// Edit › Paste in whatever has focus.
        EditPaste,
        /// Edit › Select All in whatever has focus.
        EditSelectAll,
    ]
);

const WEBSITE: &str = "https://tryferrum.dev";

/// The website page of an app, by binary name.
pub fn help_url(binary: &str) -> String {
    let slug = match binary {
        "ferrumeditor" => "ferrum-editor",
        "ferrum-launcher" => "ferrum-launcher",
        "ferrumdock" => "ferrumcontainers",
        "ferrumterm" | "ferrumgit" | "ferrumapi" | "ferrumdata" | "ferrumkit" | "ferrumports"
        | "ferrumenv" | "ferrumagentic" => binary,
        _ => "apps",
    };
    format!("{WEBSITE}/{slug}")
}

/// The app menu: About, updates, Settings, onboarding, Hide, Quit.
pub fn app_menu(info: &AppInfo) -> Menu {
    Menu::new(info.name).items([
        MenuItem::action(format!("About {}", info.name), About),
        MenuItem::action("Check for Updates…", CheckForUpdates),
        MenuItem::separator(),
        MenuItem::action("Settings…", OpenSettings),
        MenuItem::action("Show Onboarding…", ShowOnboarding),
        MenuItem::separator(),
        MenuItem::action(format!("Hide {}", info.name), HideApp),
        MenuItem::action("Hide Others", HideOthers),
        MenuItem::action("Show All", ShowAll),
        MenuItem::separator(),
        MenuItem::action(format!("Quit {}", info.name), Quit),
    ])
}

pub fn file_menu() -> Menu {
    Menu::new("File").items([
        MenuItem::action("Switch Project…", ToggleProjectSwitcher),
        MenuItem::separator(),
        MenuItem::action("Close Window", CloseWindow),
    ])
}

/// Cut / Copy / Paste / Select All, routed to the focused text field (and
/// to the system for native text).
pub fn edit_menu() -> Menu {
    Menu::new("Edit").items([
        MenuItem::os_action("Cut", EditCut, OsAction::Cut),
        MenuItem::os_action("Copy", EditCopy, OsAction::Copy),
        MenuItem::os_action("Paste", EditPaste, OsAction::Paste),
        MenuItem::os_action("Select All", EditSelectAll, OsAction::SelectAll),
    ])
}

pub fn view_menu() -> Menu {
    Menu::new("View").items([
        MenuItem::action("Larger Text", LargerText),
        MenuItem::action("Smaller Text", SmallerText),
        MenuItem::separator(),
        MenuItem::action("Enter Full Screen", ToggleFullScreen),
    ])
}

pub fn window_menu() -> Menu {
    Menu::new("Window").items([
        MenuItem::action("Minimize", Minimize),
        MenuItem::action("Zoom", Zoom),
        MenuItem::separator(),
        MenuItem::action("Close Window", CloseWindow),
    ])
}

pub fn help_menu(info: &AppInfo) -> Menu {
    Menu::new("Help").items([
        MenuItem::action(format!("{} Help", info.name), OpenHelp),
        MenuItem::action("Release Notes", OpenReleaseNotes),
        MenuItem::separator(),
        MenuItem::action("Report an Issue…", ReportIssue),
    ])
}

/// The whole menu bar for an app without menus of its own.
pub fn standard(info: &AppInfo) -> Vec<Menu> {
    vec![
        app_menu(info),
        file_menu(),
        edit_menu(),
        view_menu(),
        window_menu(),
        help_menu(info),
    ]
}

fn with_window(cx: &mut App, f: impl FnOnce(&mut gpui::Window)) {
    if let Some(handle) = cx.active_window() {
        let _ = handle.update(cx, |_, window, _| f(window));
    }
}

/// Opens Ferrum Launcher (which shows and installs updates).
pub fn open_launcher() {
    if cfg!(target_os = "macos") {
        let launched = std::process::Command::new("/usr/bin/open")
            .args(["-a", "Ferrum Launcher"])
            .status()
            .is_ok_and(|s| s.success());
        if launched {
            return;
        }
    }
    if let Some(bin) = crate::integrations::cli_path("ferrum-launcher") {
        let _ = std::process::Command::new(bin).spawn();
    }
}

/// Key bindings and handlers for the shared menu actions.
pub(crate) fn register(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-h", HideApp, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
        KeyBinding::new("cmd-m", Minimize, None),
        KeyBinding::new("ctrl-cmd-f", ToggleFullScreen, None),
        KeyBinding::new("shift-cmd-w", CloseWindow, None),
        // Fallbacks so the Edit menu shows its shortcuts; text fields and
        // terminals bind these keys in their own contexts first.
        KeyBinding::new("cmd-x", EditCut, None),
        KeyBinding::new("cmd-c", EditCopy, None),
        KeyBinding::new("cmd-v", EditPaste, None),
        KeyBinding::new("cmd-a", EditSelectAll, None),
    ]);
    cx.on_action(|_: &About, cx| crate::settings::open_settings(Some("about"), cx));
    cx.on_action(|_: &CheckForUpdates, _| open_launcher());
    cx.on_action(|_: &HideApp, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &Minimize, cx| with_window(cx, |w| w.minimize_window()));
    cx.on_action(|_: &Zoom, cx| with_window(cx, |w| w.zoom_window()));
    cx.on_action(|_: &ToggleFullScreen, cx| with_window(cx, |w| w.toggle_fullscreen()));
    cx.on_action(|_: &CloseWindow, cx| with_window(cx, |w| w.remove_window()));
    cx.on_action(|_: &LargerText, cx| step_text(cx, 1));
    cx.on_action(|_: &SmallerText, cx| step_text(cx, -1));
    cx.on_action(|_: &OpenHelp, cx| {
        let url = help_url(app_info(cx).binary);
        cx.open_url(&url);
    });
    cx.on_action(|_: &OpenReleaseNotes, cx| cx.open_url(&format!("{WEBSITE}/changelog")));
    cx.on_action(|_: &ReportIssue, cx| {
        let info = app_info(cx);
        cx.open_url(&format!(
            "mailto:hello@tryferrum.dev?subject={}%20{}%20issue",
            info.name.replace(' ', "%20"),
            crate::settings::about_page::version()
        ));
    });
}

fn step_text(cx: &mut App, delta: i32) {
    use crate::appearance::TextSize;
    let sizes = [TextSize::Small, TextSize::Default, TextSize::Large];
    crate::appearance::update(cx, |p| {
        let i = sizes.iter().position(|s| *s == p.text_size).unwrap_or(1) as i32;
        p.text_size = sizes[(i + delta).clamp(0, 2) as usize];
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// macOS greys out (and never runs) a menu item whose action nothing
    /// handles; every shared item must have a handler.
    #[gpui::test]
    fn every_menu_action_is_enabled_and_handled(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            crate::init_for_tests(cx);
            crate::appearance::init_at(None, None, cx);
            register(cx);
            cx.on_action(|_: &OpenSettings, cx| crate::settings::open_settings(None, cx));
            let actions: Vec<Box<dyn gpui::Action>> = vec![
                Box::new(About),
                Box::new(CheckForUpdates),
                Box::new(HideApp),
                Box::new(HideOthers),
                Box::new(ShowAll),
                Box::new(Minimize),
                Box::new(Zoom),
                Box::new(ToggleFullScreen),
                Box::new(CloseWindow),
                Box::new(LargerText),
                Box::new(SmallerText),
                Box::new(OpenHelp),
                Box::new(OpenReleaseNotes),
                Box::new(ReportIssue),
                Box::new(OpenSettings),
            ];
            for action in &actions {
                assert!(cx.is_action_available(action.as_ref()), "{}", action.name());
            }
            assert!(cx.windows().is_empty());
            cx.dispatch_action(&About);
        });
        cx.run_until_parked();
        cx.update(|cx| assert_eq!(cx.windows().len(), 1, "About opens Settings"));
    }

    #[test]
    fn help_pages_match_the_website() {
        assert_eq!(
            help_url("ferrumdock"),
            "https://tryferrum.dev/ferrumcontainers"
        );
        assert_eq!(
            help_url("ferrumeditor"),
            "https://tryferrum.dev/ferrum-editor"
        );
        assert_eq!(help_url("ferrumterm"), "https://tryferrum.dev/ferrumterm");
        assert_eq!(help_url("x"), "https://tryferrum.dev/apps");
    }
}
