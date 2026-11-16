//! Root view and window layout.

use crate::components::onboarding::Onboarding;
use crate::services::Services;
use ferrum_ui::gpui::Image;
use ferrum_ui::prelude::*;
use ferrumlauncher_core::{AppId, State, TokenSource};
use semver::Version;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio_util::sync::CancellationToken;

/// An install in flight. Progress is written by the download task on the
/// runtime and read on each render.
pub struct Download {
    pub version: Version,
    pub downloaded: Arc<AtomicU64>,
    /// `0` while unknown.
    pub total: Arc<AtomicU64>,
    pub cancel: CancellationToken,
    pub _task: Task<()>,
}

impl Download {
    /// Fraction complete, if the size is known.
    pub fn fraction(&self) -> Option<f32> {
        let total = self.total.load(Ordering::Relaxed);
        (total > 0).then(|| self.downloaded.load(Ordering::Relaxed) as f32 / total as f32)
    }
}

/// The one-time announcement of an app that appeared in the catalog.
pub struct NewAppNotice {
    pub app: AppId,
    pub message: SharedString,
}

/// What the main area shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    App,
}

pub struct FerrumLauncher {
    pub services: Services,
    pub state: State,
    pub page: Page,
    pub selected: AppId,
    /// Release whose notes are shown; `None` shows the latest.
    pub selected_release: Option<Version>,
    pub downloads: HashMap<AppId, Download>,
    pub checking: bool,
    /// Launcher version being downloaded to replace the running bundle.
    pub self_update: Option<Version>,
    /// Catalog logos downloaded for apps (decoded once).
    pub logos: HashMap<AppId, Arc<Image>>,
    /// The `.app` this process runs from; `None` for unbundled builds.
    pub running_bundle: Option<std::path::PathBuf>,
    pub token_source: Option<TokenSource>,
    pub onboarding: Option<Entity<Onboarding>>,
    pub(crate) onboarding_subscription: Option<Subscription>,
    pub notice: Option<Notice>,
    notice_task: Option<Task<()>>,
    /// Shown until dismissed or acted on.
    pub new_app_notice: Option<NewAppNotice>,
    /// The one-time usage-count notice is showing.
    pub usage_notice: bool,
    pub(crate) usage_loop: Task<()>,
    /// The open "Uninstall …?" confirmation.
    pub uninstall_dialog: Option<crate::components::uninstall::UninstallDialog>,
    pub(crate) check_loop: Task<()>,
    progress_ticker: Option<Task<()>>,
    pub focus_handle: FocusHandle,
    /// Shell commands, Dock, shortcuts and the menu bar icon.
    pub integration: crate::integration::Integration,
    /// Subscriptions that live as long as the window.
    kept: Vec<Subscription>,
    _subscriptions: Vec<Subscription>,
}

impl FerrumLauncher {
    pub fn new(
        services: Services,
        state: State,
        startup_notice: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // Settings › Launcher acts on this view.
        cx.set_global(crate::settings_page::LauncherHandle(cx.weak_entity()));
        let mut this = Self {
            services,
            state,
            page: Page::Overview,
            selected: AppId::EDITOR,
            selected_release: None,
            downloads: HashMap::new(),
            checking: false,
            self_update: None,
            logos: HashMap::new(),
            running_bundle: ferrumlauncher_core::self_update::running_bundle(),
            token_source: None,
            onboarding: None,
            onboarding_subscription: None,
            notice: None,
            notice_task: None,
            new_app_notice: None,
            usage_notice: false,
            usage_loop: Task::ready(()),
            uninstall_dialog: None,
            check_loop: Task::ready(()),
            progress_ticker: None,
            focus_handle: cx.focus_handle(),
            integration: crate::integration::Integration::default(),
            kept: Vec::new(),
            _subscriptions: Vec::new(),
        };
        if let Some(message) = startup_notice {
            this.notify_user(message, true, cx);
        }
        // Apps added since the last start (e.g. by this launcher version)
        // are announced; the catalog fetch announces later ones.
        this.announce_new_apps(cx);
        this.show_usage_notice(cx);
        this.start_usage_pings(cx);
        this.load_logos(cx);
        this.detect_token(window, cx);
        this.start_integration(cx);
        // Shell commands and Dock icons may have been changed in an app's
        // Settings › System (or the launcher's own Settings window).
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.reload_shared_prefs(cx);
            }
        });
        this.keep_subscription(activation);
        this
    }

    pub fn keep_subscription(&mut self, subscription: Subscription) {
        self.kept.push(subscription);
    }

    pub fn notify_user(
        &mut self,
        message: impl Into<SharedString>,
        error: bool,
        cx: &mut Context<Self>,
    ) {
        let message = message.into();
        if error {
            tracing::warn!(%message, "launcher error");
        }
        self.notice = Some(if error {
            Notice::error(message)
        } else {
            Notice::info(message)
        });
        self.notice_task = Some(Notice::dismiss_later(cx, |this: &mut Self, _| {
            this.notice = None;
        }));
        cx.notify();
    }

    pub fn select_app(&mut self, app: AppId, cx: &mut Context<Self>) {
        self.page = Page::App;
        self.selected = app;
        self.selected_release = None;
        if self.state.mark_seen(app) {
            self.persist(cx);
        }
        cx.notify();
    }

    pub fn show_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = page;
        cx.notify();
    }

    /// Re-renders while downloads run so progress bars move.
    pub(crate) fn ensure_progress_ticker(&mut self, cx: &mut Context<Self>) {
        if self.progress_ticker.is_some() {
            return;
        }
        self.progress_ticker = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(150))
                    .await;
                let keep_going = this
                    .update(cx, |this, cx| {
                        cx.notify();
                        let active = !this.downloads.is_empty();
                        if !active {
                            this.progress_ticker = None;
                        }
                        active
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        }));
    }

    /// Settings › Launcher in the shared Settings window (⌘,).
    pub fn open_settings(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        ferrum_ui::open_settings(Some("launcher"), cx);
    }

    /// The window's title row: traffic-light room, the update buttons, then
    /// the last check and the check / settings buttons.
    fn render_title_row(&mut self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        use ferrum_ui::components::ButtonSize;
        let last_check = match self.state.last_check {
            Some(time) => format!(
                "Checked {}",
                time.with_timezone(&chrono::Local).format("%b %e, %H:%M")
            ),
            None => "Never checked".to_string(),
        };
        let updates = self.state.app_updates().len();
        let launcher_update = self
            .state
            .update_for(AppId::LAUNCHER)
            .map(|release| (release.version.clone(), release.html_url.clone()));
        let self_updatable = self.running_bundle.is_some();
        ferrum_ui::title_row("title-row", true, window)
            .gap_1()
            .child(div().flex_none().w(px(8.)))
            // First after the traffic lights, as in every Ferrum app; the
            // same as ⌘,.
            .child(ferrum_ui::settings_button("settings"))
            // A bundled launcher swaps itself for the new version and
            // restarts; otherwise (cargo run, Windows, Linux) the release
            // page has the installer.
            .when_some(launcher_update, |this, (version, url)| {
                let updating = self.self_update.is_some();
                let button = Button::new(
                    "launcher-update",
                    if updating {
                        format!("Updating to v{version}…")
                    } else {
                        format!("Update Launcher to v{version}")
                    },
                )
                .size(ButtonSize::Compact)
                .style(ButtonStyle::Tinted(ferrum_ui::components::TintColor::Accent))
                .start_icon(Icon::new(IconName::Download).size(IconSize::Small))
                .disabled(updating);
                this.child(if self_updatable {
                    button
                        .tooltip(Tooltip::text(
                            "Downloads the new version, replaces this app and restarts it",
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.update_launcher(version.clone(), cx)
                        }))
                } else {
                    button
                        .tooltip(Tooltip::text(
                            "Opens the release page to download the new installer",
                        ))
                        .on_click(move |_, _, cx| cx.open_url(&url))
                })
            })
            .when(updates > 0, |this| {
                this.child(
                    Button::new(
                        "update-all",
                        format!("Update All ({updates})"),
                    )
                    .size(ButtonSize::Compact)
                    .style(ButtonStyle::Tinted(ferrum_ui::components::TintColor::Accent))
                    .start_icon(Icon::new(IconName::Download).size(IconSize::Small))
                    .tooltip(Tooltip::text(format!(
                        "{updates} update{} available: downloads and activates the newest version of every app",
                        if updates == 1 { "" } else { "s" }
                    )))
                    .on_click(cx.listener(|this, _, _, cx| this.update_all(cx))),
                )
            })
            .child(div().flex_1())
            .child(
                Label::new(if self.checking {
                    "Checking…".to_string()
                } else {
                    last_check
                })
                .size(LabelSize::Small)
                .color(Color::Muted),
            )
            .child(div().w(px(4.)))
            .child(
                ferrum_ui::title_button("check-updates", IconName::RotateCw, "Check for updates")
                    .disabled(self.checking)
                    .on_click(cx.listener(|this, _, _, cx| this.check_for_updates(true, cx))),
            )
    }
}

impl FerrumLauncher {
    fn render_new_app_notice(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let notice = self.new_app_notice.as_ref()?;
        let app = notice.app;
        let colors = cx.theme().colors();
        Some(
            h_flex()
                .px_4()
                .py_2()
                .gap_3()
                .border_b_1()
                .border_color(colors.border_variant)
                .bg(colors.elevated_surface_background)
                .child(self.logo(app, px(20.)))
                .child(div().flex_1().child(Label::new(notice.message.clone())))
                .child(
                    Button::new("new-app-install", "Install")
                        .style(ButtonStyle::Filled)
                        .start_icon(Icon::new(IconName::Download))
                        .on_click(cx.listener(move |this, _, _, cx| this.install_new_app(app, cx))),
                )
                .child(
                    IconButton::new("new-app-dismiss", IconName::Close)
                        .tooltip(Tooltip::text("Dismiss"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.new_app_notice = None;
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }
}

impl Render for FerrumLauncher {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let downloads = self.downloads.len();

        let colors = colors.clone();
        // Only while something downloads; where releases come from is not
        // the user's concern.
        let footer = (downloads > 0).then(|| {
            h_flex()
                .flex_none()
                .h(px(26.))
                .px_3()
                .justify_end()
                .border_t_1()
                .border_color(colors.border_variant)
                .child(
                    Label::new(format!("{downloads} downloading"))
                        .size(LabelSize::Small)
                        .color(Color::Accent),
                )
        });
        // The app table (or an app's page) in one card under the title row.
        let card = ferrum_ui::pane_card(true, cx)
            .flex_1()
            .min_h_0()
            .w_full()
            .children(self.render_new_app_notice(cx))
            .children(self.render_usage_notice(cx))
            .child(div().flex_1().min_h_0().child(match self.page {
                Page::Overview => self.render_overview(cx).into_any_element(),
                Page::App => self.render_detail(window, cx).into_any_element(),
            }))
            .children(footer);

        v_flex()
            .relative()
            .size_full()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_key_down(
                cx.listener(|this, event: &ferrum_ui::gpui::KeyDownEvent, _, cx| {
                    if this.uninstall_dialog.is_some() && event.keystroke.key == "escape" {
                        this.close_uninstall_dialog(cx);
                    }
                }),
            )
            .text_color(colors.text)
            .child(self.render_title_row(window, cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .px(ferrum_ui::PANE_GAP)
                    .pb(ferrum_ui::PANE_GAP)
                    .child(card),
            )
            .when_some(self.onboarding.clone(), |this, onboarding| {
                this.child(onboarding)
            })
            .when_some(self.render_uninstall_dialog(cx), |this, dialog| {
                this.child(dialog)
            })
            .when_some(self.notice.as_ref(), |this, notice| {
                this.child(render_notice(
                    notice,
                    cx.listener(|this, _, _, cx| {
                        this.notice = None;
                        cx.notify();
                    }),
                    cx,
                ))
            })
    }
}
