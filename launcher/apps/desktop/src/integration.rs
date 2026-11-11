//! Opening apps without the launcher window: shell commands (`fee .`),
//! Dock icons, global shortcuts and the menu bar icon. The choices live in
//! `Settings::app_prefs` / `menu_bar`; the files they produce (command
//! scripts, `~/.ferrum/preferences.json`) are rewritten from them.

use std::collections::HashMap;

use ferrum_ui::prelude::*;
use ferrumlauncher_core::{AppId, AppPrefs, shared_prefs, shell};

use crate::app::FerrumLauncher;

/// Signals from the menu bar icon and global shortcuts (AppKit callbacks).
#[derive(Debug, Clone)]
pub enum Signal {
    Menu(String),
    Hotkey(u32),
}

/// Per-window state for the integrations.
#[derive(Default)]
pub struct Integration {
    /// App whose shortcut is being recorded.
    pub recording: Option<AppId>,
    /// The command name field of each app page.
    pub command_inputs: HashMap<AppId, Entity<TextInput>>,
    #[cfg(target_os = "macos")]
    tray: Option<tray_icon::TrayIcon>,
    #[cfg(target_os = "macos")]
    hotkeys: Option<global_hotkey::GlobalHotKeyManager>,
    #[cfg(target_os = "macos")]
    registered: Vec<(global_hotkey::hotkey::HotKey, AppId)>,
}

/// `cmd+alt+e` → `⌃⌥⇧⌘E` order, as macOS menus show shortcuts.
pub fn display_hotkey(hotkey: &str) -> String {
    let parts: Vec<&str> = hotkey.split('+').collect();
    let has = |m: &str| parts.contains(&m);
    let mut out = String::new();
    for (name, symbol) in [("ctrl", "⌃"), ("alt", "⌥"), ("shift", "⇧"), ("cmd", "⌘")] {
        if has(name) {
            out.push_str(symbol);
        }
    }
    if let Some(key) = parts.last() {
        out.push_str(&key.to_uppercase());
    }
    out
}

/// A shortcut string from a pressed key: modifiers then the key, e.g.
/// `cmd+alt+e`. Needs ⌘, ⌃ or ⌥ and a letter, digit, F-key or space.
pub fn hotkey_from_keystroke(keystroke: &ferrum_ui::gpui::Keystroke) -> Result<String, String> {
    let m = &keystroke.modifiers;
    if !(m.platform || m.control || m.alt) {
        return Err("Use ⌘, ⌃ or ⌥ with the key".to_string());
    }
    let key = keystroke.key.to_lowercase();
    let valid = (key.len() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric()))
        || key == "space"
        || (key.starts_with('f') && key[1..].parse::<u8>().is_ok_and(|n| (1..=20).contains(&n)));
    if !valid {
        return Err(format!(
            "`{key}` can't be a shortcut key: use a letter, digit, F-key or space"
        ));
    }
    let mut parts = Vec::new();
    for (on, name) in [
        (m.control, "ctrl"),
        (m.alt, "alt"),
        (m.shift, "shift"),
        (m.platform, "cmd"),
    ] {
        if on {
            parts.push(name);
        }
    }
    parts.push(&key);
    Ok(parts.join("+"))
}

impl FerrumLauncher {
    fn prefs(&self, app: AppId) -> AppPrefs {
        self.state
            .settings
            .app_prefs
            .get(&app)
            .cloned()
            .unwrap_or_default()
    }

    fn set_prefs(&mut self, app: AppId, prefs: AppPrefs, cx: &mut Context<Self>) {
        if prefs == AppPrefs::default() {
            self.state.settings.app_prefs.remove(&app);
        } else {
            self.state.settings.app_prefs.insert(app, prefs);
        }
        self.persist(cx);
        cx.notify();
    }

    /// The command name field for `app`, created on first use.
    pub fn command_input(
        &mut self,
        app: AppId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        if let Some(input) = self.integration.command_inputs.get(&app) {
            return input.clone();
        }
        let name = self
            .prefs(app)
            .command
            .or_else(|| shell::default_command(app).map(str::to_string))
            .unwrap_or_default();
        let input = cx.new(|cx| TextInput::new(window, cx).with_text(name).monospace());
        // Enter renames an enabled command.
        let subscription = cx.subscribe(&input, move |this: &mut Self, _, event, cx| {
            if *event == TextInputEvent::Submit && this.prefs(app).command.is_some() {
                this.set_shell_command(app, true, cx);
            }
        });
        self.keep_subscription(subscription);
        self.integration.command_inputs.insert(app, input.clone());
        input
    }

    /// Turns `app`'s shell command on (with the name in its field) or off.
    pub fn set_shell_command(&mut self, app: AppId, enabled: bool, cx: &mut Context<Self>) {
        let info = self.state.info(app);
        let paths = self.services.paths.clone();
        let mut prefs = self.prefs(app);
        let typed = self
            .integration
            .command_inputs
            .get(&app)
            .map(|i| i.read(cx).text().trim().to_string())
            .filter(|t| !t.is_empty())
            .or_else(|| shell::default_command(app).map(str::to_string))
            .unwrap_or_default();
        if let Some(old) = prefs.command.take()
            && let Err(e) = shell::remove(&paths, &old)
        {
            self.notify_user(format!("Could not remove `{old}`: {e}"), true, cx);
        }
        if !enabled {
            self.set_prefs(app, prefs, cx);
            self.notify_user(format!("The `{typed}` command is off"), false, cx);
            return;
        }
        if let Err(e) = shell::validate(&typed) {
            self.set_prefs(app, prefs, cx);
            self.notify_user(e, true, cx);
            return;
        }
        let taken = self
            .state
            .settings
            .app_prefs
            .iter()
            .find(|(other, p)| **other != app && p.command.as_deref() == Some(typed.as_str()))
            .map(|(other, _)| self.state.info(*other).name);
        if let Some(other) = taken {
            self.set_prefs(app, prefs, cx);
            self.notify_user(format!("`{typed}` already opens {other}"), true, cx);
            return;
        }
        if let Err(e) = shell::install(&paths, &info, &typed) {
            self.set_prefs(app, prefs, cx);
            self.notify_user(e, true, cx);
            return;
        }
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let shell_kind = shell::Shell::from_env(std::env::var("SHELL").ok().as_deref());
        let rc = home
            .as_deref()
            .map(|home| shell::ensure_on_path(&paths.bin_dir, home, shell_kind));
        let conflict = shell::conflict(
            &typed,
            &paths.bin_dir,
            &std::env::var("PATH").unwrap_or_default(),
        );
        prefs.command = Some(typed.clone());
        self.set_prefs(app, prefs, cx);
        let usage = if app == AppId::TERM {
            format!("`{typed}` opens FerrumTerm in the current folder")
        } else if matches!(app.as_str(), "editor" | "git") {
            format!("`{typed} .` opens {} on the current folder", info.name)
        } else {
            format!("`{typed}` opens {}", info.name)
        };
        let mut message = format!("{usage}. Open a new terminal window to use it.");
        match rc {
            Some(Ok(files)) if !files.is_empty() => {
                let names: Vec<String> = files
                    .iter()
                    .map(|file| match home.as_deref().map(|h| file.strip_prefix(h)) {
                        Some(Ok(rest)) => format!("~/{}", rest.display()),
                        _ => file.display().to_string(),
                    })
                    .collect();
                message.push_str(&format!(
                    " (Added ~/.ferrum/bin to PATH in {}.)",
                    names.join(" and ")
                ));
            }
            Some(Err(e)) => {
                message = format!(
                    "{usage}, but PATH could not be updated: {e}. Add ~/.ferrum/bin to your PATH."
                );
            }
            _ => {}
        }
        if let Some(other) = conflict {
            message.push_str(&format!(" Note: {} also exists.", other.display()));
        }
        self.notify_user(message, false, cx);
    }

    /// Runs `app` without a Dock icon (from its next start; the launcher
    /// itself switches at once).
    pub fn set_hide_from_dock(&mut self, app: AppId, hide: bool, cx: &mut Context<Self>) {
        let mut prefs = self.prefs(app);
        prefs.hide_from_dock = hide;
        self.set_prefs(app, prefs, cx);
        self.write_shared_prefs(cx);
        let name = self.state.info(app).name;
        if app == AppId::LAUNCHER {
            ferrum_ui::dock::set_dock_icon_visible(!hide);
            if hide && !self.state.settings.menu_bar {
                // Without a Dock icon the menu bar is the way back in.
                self.state.settings.menu_bar = true;
                self.persist(cx);
                self.refresh_menu_bar(cx);
                self.notify_user(
                    "Ferrum Launcher is now in the menu bar instead of the Dock",
                    false,
                    cx,
                );
            }
        } else {
            let when = if hide { "without" } else { "with" };
            self.notify_user(
                format!("{name} starts {when} a Dock icon from now on (quit and reopen it if it's running)"),
                false,
                cx,
            );
        }
    }

    fn write_shared_prefs(&mut self, cx: &mut Context<Self>) {
        let mut prefs = shared_prefs::SharedPrefs::default();
        for (app, p) in &self.state.settings.app_prefs {
            if p.hide_from_dock {
                prefs.hide_from_dock.insert(self.state.info(*app).binary);
            }
        }
        let Some(path) = shared_prefs::path_for(&self.services.paths.bin_dir) else {
            return;
        };
        if let Err(e) = shared_prefs::write(&path, &prefs) {
            self.notify_user(format!("Could not save {}: {e}", path.display()), true, cx);
        }
    }

    /// Starts recording `app`'s shortcut: the next key pressed (with ⌘, ⌃
    /// or ⌥) becomes it; Escape cancels.
    pub fn record_hotkey(&mut self, app: AppId, window: &mut Window, cx: &mut Context<Self>) {
        self.integration.recording = Some(app);
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub fn on_key_down(
        &mut self,
        event: &ferrum_ui::gpui::KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(app) = self.integration.recording else {
            return;
        };
        cx.stop_propagation();
        let keystroke = &event.keystroke;
        if keystroke.key == "escape" {
            self.integration.recording = None;
            cx.notify();
            return;
        }
        // Modifier-only presses arrive as their own keys; wait for the key.
        if matches!(
            keystroke.key.as_str(),
            "shift" | "control" | "alt" | "platform" | "function" | "cmd" | "ctrl"
        ) {
            return;
        }
        match hotkey_from_keystroke(keystroke) {
            Ok(hotkey) => {
                self.integration.recording = None;
                let taken = self
                    .state
                    .settings
                    .app_prefs
                    .iter()
                    .find(|(other, p)| {
                        **other != app && p.hotkey.as_deref() == Some(hotkey.as_str())
                    })
                    .map(|(other, _)| self.state.info(*other).name);
                if let Some(other) = taken {
                    self.notify_user(
                        format!("{} already opens {other}", display_hotkey(&hotkey)),
                        true,
                        cx,
                    );
                    return;
                }
                self.set_hotkey(app, Some(hotkey), cx);
            }
            Err(message) => self.notify_user(message, true, cx),
        }
    }

    pub fn set_hotkey(&mut self, app: AppId, hotkey: Option<String>, cx: &mut Context<Self>) {
        let mut prefs = self.prefs(app);
        prefs.hotkey = hotkey;
        self.set_prefs(app, prefs, cx);
        self.refresh_hotkeys(cx);
        self.refresh_menu_bar(cx);
    }

    /// Handles a menu bar choice or a pressed shortcut.
    pub fn on_signal(&mut self, signal: Signal, cx: &mut Context<Self>) {
        let app = match signal {
            Signal::Menu(id) => match id.as_str() {
                "show" => {
                    cx.activate(true);
                    return;
                }
                "quit" => {
                    cx.quit();
                    return;
                }
                other => other.strip_prefix("open:").and_then(AppId::new),
            },
            Signal::Hotkey(id) => self.app_for_hotkey(id),
        };
        match app {
            Some(app) if app == AppId::LAUNCHER => cx.activate(true),
            Some(app) => self.launch(app, cx),
            None => {}
        }
    }
}

#[cfg(target_os = "macos")]
impl FerrumLauncher {
    /// Hooks the AppKit callbacks into this window's signal channel.
    pub fn start_integration(&mut self, cx: &mut Context<Self>) {
        use futures::StreamExt;
        use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
        use tray_icon::menu::MenuEvent;
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<Signal>();
        let menu_tx = tx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            menu_tx.unbounded_send(Signal::Menu(event.id.0)).ok();
        }));
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            if event.state() == HotKeyState::Pressed {
                tx.unbounded_send(Signal::Hotkey(event.id())).ok();
            }
        }));
        cx.spawn(async move |this, cx| {
            while let Some(signal) = rx.next().await {
                if this
                    .update(cx, |this, cx| this.on_signal(signal, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        self.refresh_hotkeys(cx);
        self.refresh_menu_bar(cx);
        self.repair_shell_path();
    }

    /// Adds the PATH line to startup files that lack it while a shell
    /// command is on (older launchers only wrote the login file).
    fn repair_shell_path(&self) {
        let any_command = self
            .state
            .settings
            .app_prefs
            .values()
            .any(|prefs| prefs.command.is_some());
        let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
            return;
        };
        if any_command {
            let shell_kind = shell::Shell::from_env(std::env::var("SHELL").ok().as_deref());
            if let Err(e) = shell::ensure_on_path(&self.services.paths.bin_dir, &home, shell_kind) {
                tracing::warn!(%e, "could not add ~/.ferrum/bin to PATH");
            }
        }
    }

    fn app_for_hotkey(&self, id: u32) -> Option<AppId> {
        self.integration
            .registered
            .iter()
            .find(|(hotkey, _)| hotkey.id() == id)
            .map(|(_, app)| *app)
    }

    /// Registers every app's shortcut (again).
    pub fn refresh_hotkeys(&mut self, cx: &mut Context<Self>) {
        use global_hotkey::hotkey::HotKey;
        let wanted: Vec<(AppId, String)> = self
            .state
            .settings
            .app_prefs
            .iter()
            .filter_map(|(app, p)| p.hotkey.clone().map(|h| (*app, h)))
            .collect();
        if wanted.is_empty() && self.integration.hotkeys.is_none() {
            return;
        }
        if self.integration.hotkeys.is_none() {
            match global_hotkey::GlobalHotKeyManager::new() {
                Ok(manager) => self.integration.hotkeys = Some(manager),
                Err(e) => {
                    self.notify_user(format!("Shortcuts are unavailable: {e}"), true, cx);
                    return;
                }
            }
        }
        let Some(manager) = self.integration.hotkeys.as_ref() else {
            return;
        };
        for (hotkey, _) in self.integration.registered.drain(..) {
            manager.unregister(hotkey).ok();
        }
        let mut failed = Vec::new();
        for (app, text) in wanted {
            match text.parse::<HotKey>() {
                Ok(hotkey) => match manager.register(hotkey) {
                    Ok(()) => self.integration.registered.push((hotkey, app)),
                    Err(_) => failed.push(display_hotkey(&text)),
                },
                Err(_) => failed.push(text),
            }
        }
        if !failed.is_empty() {
            self.notify_user(
                format!(
                    "{} is used by another app; pick a different shortcut",
                    failed.join(", ")
                ),
                true,
                cx,
            );
        }
    }

    /// Shows, updates or removes the menu bar icon.
    pub fn refresh_menu_bar(&mut self, cx: &mut Context<Self>) {
        use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
        if !self.state.settings.menu_bar {
            self.integration.tray = None;
            return;
        }
        let menu = Menu::new();
        let mut apps: Vec<_> = AppId::BUILTIN
            .into_iter()
            .filter(|app| *app != AppId::LAUNCHER && self.state.active(*app).is_some())
            .map(|app| self.state.info(app))
            .collect();
        apps.sort_by_key(|info| (info.order, info.name.clone()));
        for info in &apps {
            let accelerator = self
                .prefs(info.id)
                .hotkey
                .and_then(|h| h.replace("cmd", "super").parse().ok());
            menu.append(&MenuItem::with_id(
                format!("open:{}", info.id),
                format!("Open {}", info.name),
                true,
                accelerator,
            ))
            .ok();
        }
        if apps.is_empty() {
            menu.append(&MenuItem::with_id(
                "none",
                "No apps installed yet",
                false,
                None,
            ))
            .ok();
        }
        menu.append(&PredefinedMenuItem::separator()).ok();
        menu.append(&MenuItem::with_id(
            "show",
            "Open Ferrum Launcher",
            true,
            None,
        ))
        .ok();
        menu.append(&MenuItem::with_id(
            "quit",
            "Quit Ferrum Launcher",
            true,
            None,
        ))
        .ok();
        if let Some(tray) = &self.integration.tray {
            tray.set_menu(Some(Box::new(menu)));
            return;
        }
        let icon = menu_bar_icon();
        let mut builder = tray_icon::TrayIconBuilder::new()
            .with_tooltip("Ferrum Launcher")
            .with_menu(Box::new(menu));
        if let Some(icon) = icon {
            builder = builder.with_icon(icon);
        } else {
            builder = builder.with_title("Fe");
        }
        match builder.build() {
            Ok(tray) => self.integration.tray = Some(tray),
            Err(e) => self.notify_user(format!("Could not add the menu bar icon: {e}"), true, cx),
        }
    }
}

#[cfg(target_os = "macos")]
fn menu_bar_icon() -> Option<tray_icon::Icon> {
    let bytes: &[u8] = include_bytes!("../../../../brand/png/launcher-32.png");
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    buf.truncate(info.buffer_size());
    tray_icon::Icon::from_rgba(buf, info.width, info.height).ok()
}

#[cfg(not(target_os = "macos"))]
impl FerrumLauncher {
    pub fn start_integration(&mut self, _: &mut Context<Self>) {}

    fn app_for_hotkey(&self, _: u32) -> Option<AppId> {
        None
    }

    pub fn refresh_hotkeys(&mut self, _: &mut Context<Self>) {}

    pub fn refresh_menu_bar(&mut self, _: &mut Context<Self>) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_ui::gpui::{Keystroke, Modifiers};

    fn key(key: &str, platform: bool, alt: bool, shift: bool) -> Keystroke {
        Keystroke {
            modifiers: Modifiers {
                platform,
                alt,
                shift,
                ..Default::default()
            },
            key: key.to_string(),
            key_char: None,
        }
    }

    #[test]
    fn shortcuts_from_keys() {
        assert_eq!(
            hotkey_from_keystroke(&key("e", true, true, false)),
            Ok("alt+cmd+e".into())
        );
        assert_eq!(
            hotkey_from_keystroke(&key("F5", false, true, true)),
            Ok("alt+shift+f5".into())
        );
        assert!(hotkey_from_keystroke(&key("e", false, false, true)).is_err());
        assert!(hotkey_from_keystroke(&key("[", true, false, false)).is_err());
        assert_eq!(display_hotkey("alt+cmd+e"), "⌥⌘E");
        assert_eq!(display_hotkey("ctrl+shift+cmd+1"), "⌃⇧⌘1");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn shortcut_strings_parse_for_global_hotkey() {
        for text in ["alt+cmd+e", "ctrl+shift+f5", "cmd+space", "alt+1"] {
            assert!(
                text.parse::<global_hotkey::hotkey::HotKey>().is_ok(),
                "{text}"
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn menu_bar_icon_decodes() {
        assert!(menu_bar_icon().is_some());
    }
}
