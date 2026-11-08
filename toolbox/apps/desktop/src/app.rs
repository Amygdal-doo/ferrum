//! Root view and window layout.

use crate::components::logo::logo;
use crate::components::onboarding::Onboarding;
use crate::components::settings::{SettingsForm, SettingsFormEvent};
use crate::services::Services;
use ferrum_ui::gpui::Axis;
use ferrum_ui::prelude::*;
use ferrumtoolbox_core::{AppId, Channel, State, TokenSource};
use semver::Version;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio_util::sync::CancellationToken;

const SIDEBAR_SPLITTER: &str = "sidebar";

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

/// What the main area shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Releases,
    App,
}

pub struct FerrumToolbox {
    pub services: Services,
    pub state: State,
    pub page: Page,
    pub selected: AppId,
    /// Releases page filters (`None` = all).
    pub timeline_app: Option<AppId>,
    pub timeline_channel: Option<Channel>,
    /// Release tags whose notes are expanded on the Releases page.
    pub expanded_notes: HashSet<String>,
    /// Release whose notes are shown; `None` shows the latest.
    pub selected_release: Option<Version>,
    pub downloads: HashMap<AppId, Download>,
    pub checking: bool,
    pub token_source: Option<TokenSource>,
    pub settings_form: Option<Entity<SettingsForm>>,
    pub onboarding: Option<Entity<Onboarding>>,
    pub(crate) onboarding_subscription: Option<Subscription>,
    pub sidebar_width: Pixels,
    pub notice: Option<Notice>,
    notice_task: Option<Task<()>>,
    pub(crate) check_loop: Task<()>,
    progress_ticker: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl FerrumToolbox {
    pub fn new(
        services: Services,
        state: State,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            services,
            state,
            page: Page::Overview,
            selected: AppId::Ide,
            timeline_app: None,
            timeline_channel: None,
            expanded_notes: HashSet::new(),
            selected_release: None,
            downloads: HashMap::new(),
            checking: false,
            token_source: None,
            settings_form: None,
            onboarding: None,
            onboarding_subscription: None,
            sidebar_width: px(260.),
            notice: None,
            notice_task: None,
            check_loop: Task::ready(()),
            progress_ticker: None,
            _subscriptions: Vec::new(),
        };
        this.detect_token(window, cx);
        this
    }

    pub fn notify_user(
        &mut self,
        message: impl Into<SharedString>,
        error: bool,
        cx: &mut Context<Self>,
    ) {
        let message = message.into();
        if error {
            tracing::warn!(%message, "toolbox error");
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

    pub fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = self.state.settings.clone();
        let token_source = self.token_source;
        let form = cx.new(|cx| SettingsForm::new(settings, token_source, window, cx));
        let subscription =
            cx.subscribe_in(&form, window, |this, _, event, window, cx| match event {
                SettingsFormEvent::Saved { settings, token } => {
                    this.settings_form = None;
                    this.apply_settings(settings.clone(), token.clone(), window, cx);
                }
                SettingsFormEvent::Cancelled => {
                    this.settings_form = None;
                    cx.notify();
                }
            });
        self._subscriptions = vec![subscription];
        form.update(cx, |form, cx| form.focus_first(window, cx));
        self.settings_form = Some(form);
        cx.notify();
    }

    fn render_toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let last_check = match self.state.last_check {
            Some(time) => format!(
                "Checked {}",
                time.with_timezone(&chrono::Local).format("%b %e, %H:%M")
            ),
            None => "Never checked".to_string(),
        };
        let updates = self.state.updates().len();
        toolbar(cx)
            .justify_between()
            .child(
                h_flex()
                    .gap_2()
                    .child(logo(AppId::Toolbox, px(22.)))
                    .child(Label::new("Ferrum Toolbox"))
                    .when(updates > 0, |this| {
                        this.child(badge(
                            format!("{updates} update{}", if updates == 1 { "" } else { "s" }),
                            cx.theme().status().info,
                            cx,
                        ))
                    }),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Label::new(last_check)
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Button::new(
                            "check-updates",
                            if self.checking {
                                "Checking…"
                            } else {
                                "Check for updates"
                            },
                        )
                        .start_icon(Icon::new(IconName::RotateCw))
                        .disabled(self.checking)
                        .on_click(cx.listener(|this, _, _, cx| this.check_for_updates(true, cx))),
                    )
                    .child(
                        IconButton::new("settings", IconName::Settings)
                            .tooltip(Tooltip::text("Settings"))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.open_settings(window, cx)),
                            ),
                    ),
            )
    }
}

impl Render for FerrumToolbox {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let repo = format!(
            "{}/{}",
            self.state.settings.repo_owner, self.state.settings.repo_name
        );
        let auth = match self.token_source {
            Some(source) => format!("Token: {source}"),
            None => "No token (public repos only)".to_string(),
        };
        let downloads = self.downloads.len();

        v_flex()
            .relative()
            .size_full()
            .bg(colors.background)
            .text_color(colors.text)
            .child(self.render_toolbar(cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .on_drag_move(cx.listener(
                        |this, event: &DragMoveEvent<DraggedSplitter>, _, cx| {
                            this.sidebar_width = (event.event.position.x - event.bounds.left())
                                .clamp(px(200.), px(420.));
                            cx.notify();
                        },
                    ))
                    .child(
                        div()
                            .w(self.sidebar_width)
                            .h_full()
                            .flex_none()
                            .child(self.render_sidebar(cx)),
                    )
                    .child(splitter(SIDEBAR_SPLITTER, Axis::Horizontal, cx))
                    .child(div().flex_1().min_w_0().h_full().child(match self.page {
                        Page::Overview => self.render_overview(cx).into_any_element(),
                        Page::Releases => self.render_releases(cx).into_any_element(),
                        Page::App => self.render_detail(window, cx).into_any_element(),
                    })),
            )
            .child(
                status_bar(cx)
                    .child(Label::new(repo).size(LabelSize::Small).color(Color::Muted))
                    .child(
                        h_flex()
                            .gap_3()
                            .when(downloads > 0, |this| {
                                this.child(
                                    Label::new(format!("{downloads} downloading"))
                                        .size(LabelSize::Small)
                                        .color(Color::Accent),
                                )
                            })
                            .child(Label::new(auth).size(LabelSize::Small).color(Color::Muted)),
                    ),
            )
            .when_some(self.onboarding.clone(), |this, onboarding| {
                this.child(onboarding)
            })
            .when_some(self.settings_form.clone(), |this, form| this.child(form))
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
