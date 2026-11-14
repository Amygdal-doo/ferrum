//! Settings › Launcher (in the shared Settings window, ⌘,): the menu bar
//! icon, the Dock icon, how often to check for updates and how many
//! versions to keep.

use ferrum_ui::components::IconName;
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
            "menu bar",
            "dock",
            "tray",
            "updates",
            "check",
            "versions",
            "keep",
            "status bar",
        ]
    }
    fn render(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        let Some(launcher) = cx
            .try_global::<LauncherHandle>()
            .and_then(|h| h.0.upgrade())
        else {
            return gpui::div().into_any_element();
        };
        let (settings, hide_dock) = {
            let l = launcher.read(cx);
            (
                l.state.settings.clone(),
                l.prefs(AppId::LAUNCHER).hide_from_dock,
            )
        };
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
        let menu_bar = cfg!(target_os = "macos").then(|| {
            Section::new("Menu bar and Dock")
                .row(
                    Row::new("menu-bar", "Show Ferrum Launcher in the menu bar")
                        .detail(
                            "Click its icon for every app, updates and recent projects. \
                             Closing the window keeps the launcher running there.",
                        )
                        .control(Switch::new("menu-bar-switch", settings.menu_bar).on_change(
                            move |on, _, cx| edit(cx, |s, v| s.menu_bar = v == 1, u32::from(on)),
                        )),
                )
                .row(
                    Row::new("hide-dock", "Hide from the Dock")
                        .detail("No Dock icon or ⌘-Tab entry; the menu bar icon opens it.")
                        .control(Switch::new("hide-dock-switch", hide_dock).on_change(
                            |on, _, cx| {
                                if let Some(l) = cx
                                    .try_global::<LauncherHandle>()
                                    .and_then(|h| h.0.upgrade())
                                {
                                    l.update(cx, |l, cx| {
                                        l.set_hide_from_dock(AppId::LAUNCHER, on, cx);
                                    });
                                    cx.refresh_windows();
                                }
                            },
                        )),
                )
        });
        gpui::div()
            .flex()
            .flex_col()
            .gap_6()
            .children(menu_bar)
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
                    .footnote(
                        "Update channels (Stable, Beta or Alpha) are set per app on its page.",
                    ),
            )
            .into_any_element()
    }
}
