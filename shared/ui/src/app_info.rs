//! What an app tells `ferrum-ui` about itself for onboarding and Settings:
//! name, binary, the keys worth knowing, the permissions it needs, where
//! users migrate from, the notifications it sends and its own settings
//! pages. Pass it with [`crate::AppConfig::app`].

use std::rc::Rc;

use gpui::{App, Global};

use crate::components::IconName;
use crate::settings::SettingsPage;

/// A shortcut shown on the last onboarding step, in GPUI keystroke syntax
/// (`"cmd-shift-p"`, `"cmd-k right"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyHint {
    pub label: &'static str,
    pub keys: &'static str,
}

impl KeyHint {
    pub const fn new(label: &'static str, keys: &'static str) -> Self {
        Self { label, keys }
    }
}

/// A macOS privacy permission and why the app wants it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permission {
    pub kind: PermissionKind,
    pub reason: &'static str,
}

impl Permission {
    pub const fn new(kind: PermissionKind, reason: &'static str) -> Self {
        Self { kind, reason }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionKind {
    FullDiskAccess,
    FilesAndFolders,
    Notifications,
    Automation,
    LocalNetwork,
    Accessibility,
}

/// Where users come from and what an agent can bring over for them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    /// Tools to migrate from, e.g. `["iTerm2", "Ghostty", "tmux"]`.
    pub from: &'static [&'static str],
    /// One sentence on what comes over (and what does not).
    pub summary: &'static str,
    /// The task for the agent: where the other tools keep their config and
    /// where this app keeps its own.
    pub task: &'static str,
}

/// Something the app notifies about (FerrumTerm: Working, Blocked, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotificationEvent {
    pub id: &'static str,
    pub title: &'static str,
    pub icon: IconName,
    /// `0xRRGGBB`.
    pub tint: u32,
    /// Channels turned on until the user changes them.
    pub defaults: crate::notify::Channels,
}

/// See the module docs.
#[derive(Clone)]
pub struct AppInfo {
    pub name: &'static str,
    /// Executable name (`ferrumterm`), used for per-app preference files.
    pub binary: &'static str,
    /// One line under "Get Started with …".
    pub tagline: &'static str,
    pub icon: IconName,
    /// Brand colour, `0xRRGGBB`.
    pub tint: u32,
    pub keys: Vec<KeyHint>,
    pub permissions: Vec<Permission>,
    pub migration: Option<Migration>,
    pub events: Vec<NotificationEvent>,
    pub pages: Vec<Rc<dyn SettingsPage>>,
    /// Show the walkthrough on first launch.
    pub onboarding: bool,
}

impl AppInfo {
    pub fn new(name: &'static str, binary: &'static str) -> Self {
        Self {
            name,
            binary,
            tagline: "",
            icon: IconName::Sparkle,
            tint: 0x5c78e2,
            keys: Vec::new(),
            permissions: Vec::new(),
            migration: None,
            events: Vec::new(),
            pages: Vec::new(),
            onboarding: true,
        }
    }

    pub fn tagline(mut self, tagline: &'static str) -> Self {
        self.tagline = tagline;
        self
    }

    pub fn icon(mut self, icon: IconName, tint: u32) -> Self {
        self.icon = icon;
        self.tint = tint;
        self
    }

    pub fn key(mut self, label: &'static str, keys: &'static str) -> Self {
        self.keys.push(KeyHint::new(label, keys));
        self
    }

    pub fn permission(mut self, kind: PermissionKind, reason: &'static str) -> Self {
        self.permissions.push(Permission::new(kind, reason));
        self
    }

    pub fn migration(mut self, migration: Migration) -> Self {
        self.migration = Some(migration);
        self
    }

    pub fn event(mut self, event: NotificationEvent) -> Self {
        self.events.push(event);
        self
    }

    pub fn page(mut self, page: impl SettingsPage + 'static) -> Self {
        self.pages.push(Rc::new(page));
        self
    }

    /// Turns the first-launch walkthrough off (it stays in Settings).
    pub fn without_onboarding(mut self) -> Self {
        self.onboarding = false;
        self
    }
}

pub(crate) struct AppInfoGlobal(pub(crate) Rc<AppInfo>);

impl Global for AppInfoGlobal {}

/// The running app's [`AppInfo`] (a bare one named after the executable
/// when the app gave none).
pub fn app_info(cx: &App) -> Rc<AppInfo> {
    match cx.try_global::<AppInfoGlobal>() {
        Some(global) => global.0.clone(),
        None => Rc::new(AppInfo::new("Ferrum", "ferrum").without_onboarding()),
    }
}
