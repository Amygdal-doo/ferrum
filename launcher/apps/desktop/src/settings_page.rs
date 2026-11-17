//! Settings › Launcher (in the shared Settings window, ⌘,): how often to
//! check for updates, how many versions to keep, background downloads and
//! the anonymous usage count. The menu bar and Dock
//! switches are under Interface › Window.

use ferrum_ui::components::{Button, ButtonCommon as _, ButtonStyle, Clickable as _, IconName};
use ferrum_ui::gpui::{
    self, AnyElement, App, Global, Hsla, IntoElement, ParentElement, Styled, WeakEntity, Window,
};
use ferrum_ui::settings::SettingsPage;
use ferrum_ui::settings::widgets::{Row, Section, Switch, select};
use ferrumlauncher_core::AppId;

use crate::app::FerrumLauncher;

/// The launcher window's view, for the Settings window to act on.
pub struct LauncherHandle(pub WeakEntity<FerrumLauncher>);

impl Global for LauncherHandle {}

/// Update checks: (hours, label).
const INTERVALS: [(u32, &str); 6] = [
    (1, "Every hour"),
    (6, "Every 6 hours"),
    (12, "Every 12 hours"),
    (24, "Daily"),
    (72, "Every 3 days"),
    (168, "Weekly"),
];

/// Versions kept per app (0 keeps all).
const KEEP: [(u32, &str); 5] = [
    (1, "Only the active one"),
    (2, "2"),
    (3, "3"),
    (5, "5"),
    (0, "All"),
];

fn launcher(cx: &App) -> Option<ferrum_ui::gpui::Entity<FerrumLauncher>> {
    cx.try_global::<LauncherHandle>()
        .and_then(|h| h.0.upgrade())
}

/// Interface › Window: the menu bar icon (macOS only).
pub fn menu_bar_row(_: &mut Window, cx: &mut App) -> AnyElement {
    let Some(on) = cfg!(target_os = "macos")
        .then(|| launcher(cx))
        .flatten()
        .map(|l| l.read(cx).state.settings.menu_bar)
    else {
        return gpui::Empty.into_any_element();
    };
    Row::new("menu-bar", "Show in menu bar")
        .detail(
            "Its icon lists every app, updates and recent projects. \
             Closing the window keeps the launcher running there.",
        )
        .control(Switch::new("menu-bar-switch", on).on_change(|on, _, cx| {
            if let Some(l) = launcher(cx) {
                l.update(cx, |l, cx| {
                    let mut settings = l.state.settings.clone();
                    settings.menu_bar = on;
                    l.apply_settings(settings, cx);
                });
            }
        }))
        .into_any_element()
}

/// Interface › Window › Show in Dock: without a Dock icon the launcher
/// turns its menu bar icon on, so it can still be reached.
pub fn set_in_dock(on: bool, cx: &mut App) {
    if let Some(l) = launcher(cx) {
        l.update(cx, |l, cx| l.set_hide_from_dock(AppId::LAUNCHER, !on, cx));
    }
}

#[derive(Default)]
pub struct LauncherPage;

impl SettingsPage for LauncherPage {
    fn id(&self) -> &'static str {
        "launcher"
    }
    fn title(&self) -> &'static str {
        "Launcher"
    }
    fn icon(&self) -> IconName {
        IconName::Download
    }
    fn tint(&self) -> Hsla {
        gpui::rgb(0xe8743b).into()
    }
    fn keywords(&self) -> &'static [&'static str] {
        &[
            "updates",
            "check",
            "versions",
            "keep",
            "automatic",
            "background",
            "privacy",
            "usage",
            "analytics",
            "telemetry",
        ]
    }
    fn render(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        let Some(launcher) = cx
            .try_global::<LauncherHandle>()
            .and_then(|h| h.0.upgrade())
        else {
            return gpui::div().into_any_element();
        };
        let settings = launcher.read(cx).state.settings.clone();
        let edit =
            move |cx: &mut App, f: fn(&mut ferrumlauncher_core::Settings, u32), value: u32| {
                if let Some(l) = cx
                    .try_global::<LauncherHandle>()
                    .and_then(|h| h.0.upgrade())
                {
                    l.update(cx, |l, cx| {
                        let mut settings = l.state.settings.clone();
                        f(&mut settings, value);
                        l.apply_settings(settings, cx);
                    });
                    cx.refresh_windows();
                }
            };
        let interval = INTERVALS
            .iter()
            .position(|(h, _)| *h == settings.check_interval_hours)
            .unwrap_or(2);
        let keep = KEEP
            .iter()
            .position(|(k, _)| *k == settings.keep_versions)
            .unwrap_or(0);
        gpui::div()
            .flex()
            .flex_col()
            .gap_6()
            .child(
                Section::new("Updates")
                    .row(Row::new("check-every", "Check for updates").control(select(
                        "check-every-select",
                        &INTERVALS.map(|(_, label)| label),
                        interval,
                        move |i, _, cx| edit(cx, |s, v| s.check_interval_hours = v, INTERVALS[i].0),
                        window,
                        cx,
                    )))
                    .row(
                        Row::new("keep", "Versions to keep")
                            .detail("Older versions stay on disk for rolling back.")
                            .control(select(
                                "keep-select",
                                &KEEP.map(|(_, label)| label),
                                keep,
                                move |i, _, cx| edit(cx, |s, v| s.keep_versions = v, KEEP[i].0),
                                window,
                                cx,
                            )),
                    )
                    .row(
                        Row::new("auto-update", "Download updates in the background")
                            .detail(
                                "After each check. An app switches to its new version once it \
                                 isn't running; a running one shows Restart to update.",
                            )
                            .control(
                                Switch::new("auto-update-switch", settings.auto_update).on_change(
                                    |on, _, cx| {
                                        if let Some(l) = self::launcher(cx) {
                                            l.update(cx, |l, cx| {
                                                let mut settings = l.state.settings.clone();
                                                settings.auto_update = on;
                                                l.apply_settings(settings, cx);
                                            });
                                            cx.refresh_windows();
                                        }
                                    },
                                ),
                            ),
                    )
                    .footnote(
                        "Update channels (Stable, Beta or Alpha) are set per app on its page.",
                    ),
            )
            .child(privacy_section(settings.share_usage_counts))
            .into_any_element()
    }
}

/// Privacy: the anonymous daily usage count.
fn privacy_section(on: bool) -> Section {
    use ferrumlauncher_core::usage;
    Section::new("Privacy")
        .row(
            Row::new("share-usage", "Share anonymous usage counts")
                .detail(
                    "Once a day: a random install ID, the launcher and app versions, OS and CPU \
                     type. Nothing that identifies you, and no IP addresses are kept.",
                )
                .control(
                    Switch::new("share-usage-switch", on).on_change(|on, _, cx| {
                        if let Some(l) = launcher(cx) {
                            l.update(cx, |l, cx| l.set_share_usage_counts(on, cx));
                            cx.refresh_windows();
                        }
                    }),
                ),
        )
        .row(
            Row::new("usage-privacy", "What is sent and why").control(
                Button::new("usage-privacy-open", "Open")
                    .style(ButtonStyle::Subtle)
                    .on_click(|_, _, cx| cx.open_url(usage::PRIVACY_URL)),
            ),
        )
        .footnote("FERRUM_NO_ANALYTICS=1 or DO_NOT_TRACK=1 in the environment also turns it off.")
}
