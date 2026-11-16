//! Running without a Dock icon. The choice is recorded per app in
//! `~/.ferrum/preferences.json` (`hide_from_dock`: binary names) by
//! Settings › System or FerrumLauncher; every app reads it when it starts
//! and switches at once when it is changed in its own Settings.
//!
//! macOS shows the menu bar only for apps with a Dock icon (activation
//! policy `Regular`); an `Accessory` app keeps the previous app's menu bar
//! even while its own window is key. So a hidden app carries its menus in a
//! menu bar extra instead (see `status_item`), unless it has one of its own
//! (FerrumLauncher). An app in the Dock shows the extra too when it is
//! listed in `show_in_menu_bar` (Settings › System › Show in menu bar).

use std::path::PathBuf;
use std::sync::Mutex;

#[cfg(target_os = "macos")]
mod status_item;

/// The app name for the menu bar extra; `None` while the app opts out.
static STATUS_ITEM_NAME: Mutex<Option<String>> = Mutex::new(None);

/// Gives the app a menu bar extra with its menus while it runs without a
/// Dock icon. `ferrum_ui::run` turns it on unless the app has its own.
pub(crate) fn use_status_item(app_name: Option<&str>) {
    *STATUS_ITEM_NAME.lock().unwrap_or_else(|e| e.into_inner()) = app_name.map(str::to_owned);
}

/// The titles of the menu bar extra's submenus, from the main menu's: the
/// app menu (first, possibly untitled) is named after the app.
pub fn status_menu_titles(app_name: &str, main_menu_titles: &[String]) -> Vec<String> {
    main_menu_titles
        .iter()
        .enumerate()
        .map(|(i, title)| {
            if i == 0 && title.trim().is_empty() {
                app_name.to_owned()
            } else {
                title.clone()
            }
        })
        .collect()
}

fn preferences_file() -> Option<PathBuf> {
    crate::prefs::path("preferences.json")
}

/// Whether `binary` is in the `hide_from_dock` list of the preferences
/// JSON `text`.
pub fn listed(text: &str, binary: &str) -> bool {
    listed_under(text, "hide_from_dock", binary)
}

/// Whether `binary` is in the list `key` of the preferences JSON `text`.
pub fn listed_under(text: &str, key: &str, binary: &str) -> bool {
    // A tiny reader so the shared UI crate needs no JSON dependency: the
    // file is `{"hide_from_dock": ["a", "b"]}`, written by the launcher.
    let Some((_, rest)) = text.split_once(&format!("\"{key}\"")) else {
        return false;
    };
    let Some(list) = rest
        .split_once('[')
        .and_then(|(_, r)| r.split_once(']'))
        .map(|(l, _)| l)
    else {
        return false;
    };
    list.split(',')
        .map(|item| item.trim().trim_matches('"'))
        .any(|item| item == binary)
}

/// Whether this app should start without a Dock icon.
pub fn hidden_by_preference() -> bool {
    preference_lists("hide_from_dock")
}

/// Whether this app keeps its menu bar icon while it is in the Dock.
pub fn in_menu_bar_by_preference() -> bool {
    preference_lists("show_in_menu_bar")
}

/// Whether the preferences list `key` names this app's binary.
fn preference_lists(key: &str) -> bool {
    let (Some(file), Some(binary)) = (
        preferences_file(),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string())),
    ) else {
        return false;
    };
    std::fs::read_to_string(file).is_ok_and(|text| listed_under(&text, key, &binary))
}

/// Whether the app can show a menu bar extra at all (FerrumLauncher has
/// its own).
pub fn has_status_item() -> bool {
    STATUS_ITEM_NAME
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_some()
}

/// Adds or removes `binary` from the `show_in_menu_bar` list (Settings ›
/// System › Show in menu bar), keeping every other field of the file.
pub fn set_in_menu_bar(binary: &str, shown: bool) {
    let Some(file) = preferences_file() else {
        return;
    };
    if let Err(error) = ferrum_shell::ops::set_in_menu_bar(&file, binary, shown) {
        eprintln!("ferrum: could not save {}: {error}", file.display());
    }
}

/// Shows or hides the menu bar extra of an app that has a Dock icon (one
/// without always shows it).
#[cfg(target_os = "macos")]
pub fn set_menu_bar_icon_visible(visible: bool) {
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return;
    };
    if visible {
        show_status_item(mtm);
    } else if !hidden_by_preference() {
        status_item::hide();
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_menu_bar_icon_visible(_visible: bool) {}

#[cfg(target_os = "macos")]
fn show_status_item(mtm: objc2::MainThreadMarker) {
    let name = STATUS_ITEM_NAME
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    if let Some(name) = name {
        status_item::show(&name, mtm);
    }
}

/// Adds or removes `binary` from the `hide_from_dock` list (Settings ›
/// System › Show in Dock), keeping every other field of the file.
pub fn set_hidden(binary: &str, hidden: bool) {
    let Some(file) = preferences_file() else {
        return;
    };
    if let Err(error) = ferrum_shell::ops::set_hidden(&file, binary, hidden) {
        eprintln!("ferrum: could not save {}: {error}", file.display());
    }
}

/// Shows or hides the Dock icon (and the app in ⌘-Tab) while running.
/// Without it, the app's menus move to a menu bar extra (see the module
/// docs); with it, they are back in the menu bar.
#[cfg(target_os = "macos")]
pub fn set_dock_icon_visible(visible: bool) {
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    // AppKit must be called on the main thread, where GPUI runs.
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    if visible {
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
        if !in_menu_bar_by_preference() {
            status_item::hide();
        }
        // A newly `Regular` app gets the menu bar only once activated.
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    } else {
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        show_status_item(mtm);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_dock_icon_visible(_visible: bool) {}

/// Call after replacing the menu bar (`cx.set_menus`) at runtime, so the
/// menu bar extra of an app hidden from the Dock shows the new menus.
pub fn refresh_status_item() {
    #[cfg(target_os = "macos")]
    if let Some(mtm) = objc2::MainThreadMarker::new() {
        status_item::refresh(mtm);
    }
}

#[cfg(test)]
mod tests {
    use super::{listed, listed_under, status_menu_titles};

    #[test]
    fn the_app_menu_is_named_after_the_app() {
        let titles = |t: &[&str]| t.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            status_menu_titles("FerrumGIT", &titles(&["", "File", "Edit"])),
            titles(&["FerrumGIT", "File", "Edit"])
        );
        assert_eq!(
            status_menu_titles("FerrumGIT", &titles(&["FerrumGIT", "Window"])),
            titles(&["FerrumGIT", "Window"])
        );
        // A later untitled menu keeps its (empty) title.
        assert_eq!(
            status_menu_titles("X", &titles(&["X", ""])),
            titles(&["X", ""])
        );
        assert!(status_menu_titles("X", &[]).is_empty());
    }

    #[test]
    fn reads_the_launchers_list() {
        let text = "{\n  \"hide_from_dock\": [\n    \"ferrumeditor\",\n    \"ferrumgit\"\n  ]\n}";
        assert!(listed(text, "ferrumeditor"));
        assert!(listed(text, "ferrumgit"));
        assert!(!listed(text, "ferrumterm"));
        assert!(!listed("{}", "ferrumeditor"));
        assert!(!listed("{\"hide_from_dock\": []}", "ferrumeditor"));
        let both = r#"{"hide_from_dock": ["ferrumgit"], "show_in_menu_bar": ["ferrumterm"]}"#;
        assert!(listed_under(both, "show_in_menu_bar", "ferrumterm"));
        assert!(!listed_under(both, "show_in_menu_bar", "ferrumgit"));
        assert!(!listed(both, "ferrumterm"));
    }
}
