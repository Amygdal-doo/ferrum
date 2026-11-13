//! Notifications, as set in Settings › Notifications and kept per app in
//! `~/.ferrum/<binary>/notifications.json`: an in-app toast (drawn by the
//! window root at the chosen corner), a system notification, a Dock badge
//! and a sound, each switchable per event.
//!
//! ```ignore
//! ferrum_ui::notify::notify("done", "Tests finished", "cargo test exited 0", cx);
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use gpui::{App, Global, SharedString};
use serde::{Deserialize, Serialize};

use crate::app_info::app_info;
use crate::prefs;

/// Where an event is delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Channels {
    pub in_app: bool,
    pub system: bool,
    pub badge: bool,
    pub sound: bool,
}

impl Channels {
    pub const ALL: Self = Self {
        in_app: true,
        system: true,
        badge: true,
        sound: true,
    };
    pub const IN_APP: Self = Self {
        in_app: true,
        system: false,
        badge: false,
        sound: false,
    };

    /// "Tabs, Notifications, Dock Badge, Sound" style summary.
    pub fn summary(self) -> String {
        let parts: Vec<&str> = [
            (self.in_app, "In app"),
            (self.system, "Notifications"),
            (self.badge, "Dock badge"),
            (self.sound, "Sound"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect();
        if parts.is_empty() {
            "Off".into()
        } else {
            parts.join(", ")
        }
    }
}

/// Corner (or edge centre) of the window where toasts appear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToastPosition {
    TopLeft,
    TopCenter,
    #[default]
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl ToastPosition {
    pub const ALL: [Self; 6] = [
        Self::TopLeft,
        Self::TopCenter,
        Self::TopRight,
        Self::BottomLeft,
        Self::BottomCenter,
        Self::BottomRight,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::TopLeft => "top left",
            Self::TopCenter => "top center",
            Self::TopRight => "top right",
            Self::BottomLeft => "bottom left",
            Self::BottomCenter => "bottom center",
            Self::BottomRight => "bottom right",
        }
    }

    pub fn is_top(self) -> bool {
        matches!(self, Self::TopLeft | Self::TopCenter | Self::TopRight)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationPrefs {
    pub position: ToastPosition,
    /// Master switches; an event's channel only fires when both are on.
    pub in_app: bool,
    pub system: bool,
    pub badge: bool,
    pub sound: bool,
    /// 0.0–1.0.
    pub volume: f32,
    /// Per-event overrides of the app's defaults.
    pub events: BTreeMap<String, Channels>,
    /// App-specific switches (FerrumTerm: `osc_notifications`, …).
    pub flags: BTreeMap<String, bool>,
}

impl Default for NotificationPrefs {
    fn default() -> Self {
        Self {
            position: ToastPosition::default(),
            in_app: true,
            system: true,
            badge: true,
            sound: true,
            volume: 0.7,
            events: BTreeMap::new(),
            flags: BTreeMap::new(),
        }
    }
}

impl NotificationPrefs {
    /// Channels for `event`: the user's choice, else `default`, masked by
    /// the master switches.
    pub fn channels(&self, event: &str, default: Channels) -> Channels {
        let c = self.events.get(event).copied().unwrap_or(default);
        Channels {
            in_app: c.in_app && self.in_app,
            system: c.system && self.system,
            badge: c.badge && self.badge,
            sound: c.sound && self.sound,
        }
    }

    pub fn flag(&self, name: &str, default: bool) -> bool {
        self.flags.get(name).copied().unwrap_or(default)
    }
}

/// `~/.ferrum/<binary>/notifications.json`.
pub fn prefs_file(binary: &str) -> Option<PathBuf> {
    prefs::path(&format!("{binary}/notifications.json"))
}

pub fn load(cx: &App) -> NotificationPrefs {
    prefs_file(app_info(cx).binary)
        .map(|f| prefs::read(&f))
        .unwrap_or_default()
}

pub fn save(prefs: &NotificationPrefs, cx: &App) {
    if let Some(file) = prefs_file(app_info(cx).binary)
        && let Err(error) = prefs::write(&file, prefs)
    {
        eprintln!("ferrum: could not save {}: {error}", file.display());
    }
}

/// Changes the saved preferences with `f`.
pub fn update(cx: &mut App, f: impl FnOnce(&mut NotificationPrefs)) {
    let mut prefs = load(cx);
    f(&mut prefs);
    save(&prefs, cx);
    cx.refresh_windows();
}

/// A toast currently on screen.
#[derive(Debug, Clone)]
pub struct Toast {
    pub id: u64,
    pub title: SharedString,
    pub body: SharedString,
    pub event: &'static str,
    pub shown: Instant,
}

const TOAST_LIFETIME: Duration = Duration::from_secs(6);

#[derive(Default)]
pub(crate) struct Toasts {
    pub(crate) items: Vec<Toast>,
    next: u64,
    badge: usize,
}

impl Global for Toasts {}

/// Delivers `event` (an id from [`crate::AppInfo::events`], or any string
/// for in-app only) through the channels the user chose.
pub fn notify(
    event: &'static str,
    title: impl Into<SharedString>,
    body: impl Into<SharedString>,
    cx: &mut App,
) {
    deliver(event, title.into(), body.into(), true, cx);
}

/// Like [`notify`] without the system notification, for events another
/// process already posts (FerrumTerm's server notifies on its own).
pub fn notify_in_app(
    event: &'static str,
    title: impl Into<SharedString>,
    body: impl Into<SharedString>,
    cx: &mut App,
) {
    deliver(event, title.into(), body.into(), false, cx);
}

fn deliver(
    event: &'static str,
    title: SharedString,
    body: SharedString,
    system: bool,
    cx: &mut App,
) {
    let info = app_info(cx);
    let default = info
        .events
        .iter()
        .find(|e| e.id == event)
        .map_or(Channels::IN_APP, |e| e.defaults);
    let prefs = load(cx);
    let channels = prefs.channels(event, default);
    if channels.system && system {
        system_notification(info.name, &title, &body);
    }
    if channels.sound {
        play_sound(prefs.volume);
    }
    let toasts = cx.default_global::<Toasts>();
    if channels.badge {
        toasts.badge += 1;
        set_dock_badge(Some(toasts.badge));
    }
    if channels.in_app {
        toasts.next += 1;
        let id = toasts.next;
        toasts.items.push(Toast {
            id,
            title,
            body,
            event,
            shown: Instant::now(),
        });
        toasts.items.retain(|t| t.shown.elapsed() < TOAST_LIFETIME);
        cx.spawn(async move |cx| {
            cx.background_executor().timer(TOAST_LIFETIME).await;
            cx.update(|cx| dismiss(id, cx));
        })
        .detach();
    }
    cx.refresh_windows();
}

/// Removes a toast.
pub fn dismiss(id: u64, cx: &mut App) {
    cx.default_global::<Toasts>().items.retain(|t| t.id != id);
    cx.refresh_windows();
}

/// Clears the Dock badge (the app does it when a window becomes active).
pub fn clear_badge(cx: &mut App) {
    let toasts = cx.default_global::<Toasts>();
    if toasts.badge > 0 {
        toasts.badge = 0;
        set_dock_badge(None);
    }
}

/// A notification from the system (Notification Center on macOS).
pub fn system_notification(app: &str, title: &str, body: &str) {
    if cfg!(target_os = "macos") {
        let script = format!(
            "display notification {} with title {} subtitle {}",
            applescript_string(body),
            applescript_string(app),
            applescript_string(title),
        );
        spawn_quiet(Command::new("/usr/bin/osascript").args(["-e", &script]));
    } else if cfg!(target_os = "linux") {
        spawn_quiet(Command::new("notify-send").args([app, &format!("{title}\n{body}")]));
    }
}

/// The system alert sound at `volume` (0–1).
pub fn play_sound(volume: f32) {
    if cfg!(target_os = "macos") {
        let volume = format!("{:.2}", volume.clamp(0., 1.));
        spawn_quiet(Command::new("/usr/bin/afplay").args([
            "-v",
            &volume,
            "/System/Library/Sounds/Glass.aiff",
        ]));
    }
}

fn spawn_quiet(command: &mut Command) {
    let _ = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// `"text"` as an AppleScript string literal.
pub fn applescript_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(target_os = "macos")]
fn set_dock_badge(count: Option<usize>) {
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return;
    };
    let label = count.map(|n| NSString::from_str(&n.to_string()));
    NSApplication::sharedApplication(mtm)
        .dockTile()
        .setBadgeLabel(label.as_deref());
}

#[cfg(not(target_os = "macos"))]
fn set_dock_badge(_count: Option<usize>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn master_switches_mask_event_channels() {
        let mut prefs = NotificationPrefs::default();
        assert_eq!(prefs.channels("done", Channels::ALL), Channels::ALL);
        prefs.sound = false;
        assert!(!prefs.channels("done", Channels::ALL).sound);
        prefs.events.insert("done".into(), Channels::IN_APP);
        assert_eq!(prefs.channels("done", Channels::ALL), Channels::IN_APP);
    }

    #[test]
    fn summaries_and_quoting() {
        assert_eq!(Channels::default().summary(), "Off");
        assert_eq!(
            Channels::ALL.summary(),
            "In app, Notifications, Dock badge, Sound"
        );
        assert_eq!(applescript_string(r#"a "b" \c"#), r#""a \"b\" \\c""#);
    }
}
