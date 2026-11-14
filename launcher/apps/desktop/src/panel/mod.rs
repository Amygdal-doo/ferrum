//! The menu bar panel: what clicking the menu bar icon opens, in the
//! manner of JetBrains Toolbox. The apps (open, update, install, with
//! download progress) and recent projects, driven by the same
//! [`FerrumLauncher`] as the main window. It closes when it loses focus or
//! on Escape.

mod apps;
mod chrome;
mod layout;
mod projects;
mod window;

use std::time::Instant;

use ferrum_ui::prelude::*;
use ferrum_ui::project::RecentProject;
use ferrumlauncher_core::AppId;
use semver::Version;

use crate::app::FerrumLauncher;

pub use layout::IconRect;
pub use window::{open, toggle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Apps,
    Projects,
}

pub struct LauncherPanel {
    launcher: Entity<FerrumLauncher>,
    tab: Tab,
    search: Entity<TextInput>,
    projects: Vec<RecentProject>,
    /// Launcher version whose toast was dismissed.
    dismissed_update: Option<Version>,
    /// Opened as a regular window (`FERRUM_LAUNCHER_PANEL_PREVIEW`): it
    /// stays open without focus.
    preview: bool,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl LauncherPanel {
    pub fn new(
        launcher: Entity<FerrumLauncher>,
        preview: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Search")
                .borderless()
        });
        search.update(cx, |input, cx| input.focus(window, cx));
        let subscriptions = vec![
            cx.observe(&launcher, |_, _, cx| cx.notify()),
            cx.subscribe(&search, |_, _, event, cx| {
                if *event == TextInputEvent::Changed {
                    cx.notify();
                }
            }),
            cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() && !this.preview {
                    this.close(window, cx);
                }
            }),
        ];
        Self {
            launcher,
            // `FERRUM_LAUNCHER_PANEL_PREVIEW=projects` starts on Projects.
            tab: if preview
                && std::env::var("FERRUM_LAUNCHER_PANEL_PREVIEW").as_deref() == Ok("projects")
            {
                Tab::Projects
            } else {
                Tab::Apps
            },
            search,
            projects: projects::load(),
            dismissed_update: None,
            preview,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    fn query(&self, cx: &App) -> String {
        self.search.read(cx).text().trim().to_string()
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        self.launcher.update(cx, |launcher, _| {
            launcher.integration.panel = None;
            launcher.integration.panel_closed_at = Some(Instant::now());
        });
    }

    /// Brings up the main window: on `app`'s page, or with Settings open.
    fn show_main(
        &mut self,
        app: Option<AppId>,
        settings: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let launcher = self.launcher.clone();
        if let Some(app) = app {
            launcher.update(cx, |launcher, cx| launcher.select_app(app, cx));
        }
        cx.activate(true);
        if let Some(main) = launcher.read(cx).integration.main_window {
            main.update(cx, |_, window, cx| {
                window.activate_window();
                if settings {
                    launcher.update(cx, |launcher, cx| launcher.open_settings(window, cx));
                }
            })
            .ok();
        }
        self.close(window, cx);
    }
}

impl Render for LauncherPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_rem_size(ferrum_ui::appearance::rem_size(cx));
        let colors = cx.theme().colors().clone();
        let body = match self.tab {
            Tab::Apps => self.render_apps(cx),
            Tab::Projects => self.render_projects(cx),
        };
        ferrum_ui::backdrop(true, cx)
            .track_focus(&self.focus_handle)
            .on_action(
                cx.listener(|this, _: &ferrum_ui::menu::Cancel, window, cx| this.close(window, cx)),
            )
            .size_full()
            .flex()
            .flex_col()
            .p(ferrum_ui::PANE_GAP)
            .gap(ferrum_ui::PANE_GAP)
            .rounded(px(14.))
            .border_1()
            .border_color(colors.border)
            .overflow_hidden()
            .text_color(colors.text)
            .child(self.render_header(cx))
            .child(self.render_tabs(cx))
            .child(
                ferrum_ui::pane_frame(cx).flex_1().child(
                    v_flex()
                        .id("panel-body")
                        .size_full()
                        .overflow_y_scroll()
                        .p_1()
                        .children(body),
                ),
            )
            .children(self.render_toast(cx))
    }
}
