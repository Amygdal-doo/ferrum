//! Running without a Dock icon. Ferrum Launcher records the choice per app
//! in `~/.ferrum/preferences.json` (`hide_from_dock`: binary names); every
//! app reads it when it starts.

use std::path::{Path, PathBuf};

fn preferences_file() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(Path::new(&home).join(".ferrum").join("preferences.json"))
}

/// Whether `binary` is listed in the preferences JSON `text`.
pub fn listed(text: &str, binary: &str) -> bool {
    // A tiny reader so the shared UI crate needs no JSON dependency: the
    // file is `{"hide_from_dock": ["a", "b"]}`, written by the launcher.
    let Some((_, rest)) = text.split_once("\"hide_from_dock\"") else {
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
    let (Some(file), Some(binary)) = (
        preferences_file(),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string())),
    ) else {
        return false;
    };
    std::fs::read_to_string(file).is_ok_and(|text| listed(&text, &binary))
}

/// Shows or hides the Dock icon (and the app in ⌘-Tab) while running.
#[cfg(target_os = "macos")]
pub fn set_dock_icon_visible(visible: bool) {
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    // AppKit must be called on the main thread, where GPUI runs.
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return;
    };
    let policy = if visible {
        NSApplicationActivationPolicy::Regular
    } else {
        NSApplicationActivationPolicy::Accessory
    };
    NSApplication::sharedApplication(mtm).setActivationPolicy(policy);
}

#[cfg(not(target_os = "macos"))]
pub fn set_dock_icon_visible(_visible: bool) {}

#[cfg(test)]
mod tests {
    use super::listed;

    #[test]
    fn reads_the_launchers_list() {
        let text = "{\n  \"hide_from_dock\": [\n    \"ferrumeditor\",\n    \"ferrumgit\"\n  ]\n}";
        assert!(listed(text, "ferrumeditor"));
        assert!(listed(text, "ferrumgit"));
        assert!(!listed(text, "ferrumterm"));
        assert!(!listed("{}", "ferrumeditor"));
        assert!(!listed("{\"hide_from_dock\": []}", "ferrumeditor"));
    }
}
