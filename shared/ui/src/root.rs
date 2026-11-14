//! The view every Ferrum window's content sits in: the app's own view, the
//! onboarding walkthrough over it when pending, and notification toasts.

use gpui::{
    AnyView, App, Context, Entity, Focusable, IntoElement, ParentElement, Render, Styled,
    Subscription, Window, div, prelude::*, px,
};

use crate::app_info::app_info;
use crate::components::{
    Clickable, Color, IconButton, IconName, Label, LabelCommon, LabelSize, h_flex, v_flex,
};
use crate::notify::{self, ToastPosition, Toasts};
use crate::onboarding::{self, Onboarding, OnboardingEvent};
use crate::settings::ShowOnboarding;
use crate::theme::ActiveTheme;

pub struct Root {
    app: AnyView,
    /// Window size before onboarding enlarged it (restored afterwards).
    before_onboarding: Option<gpui::Size<gpui::Pixels>>,
    /// Window background last set for the interface style.
    background: Option<gpui::WindowBackgroundAppearance>,
    onboarding: Option<(Entity<Onboarding>, Subscription)>,
    _activation: Subscription,
}

impl Root {
    pub fn new(app: AnyView, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let activation = cx.observe_window_activation(window, |_, window, cx| {
            if window.is_window_active() {
                notify::clear_badge(cx);
            }
        });
        let mut root = Self {
            app,
            background: None,
            before_onboarding: None,
            onboarding: None,
            _activation: activation,
        };
        let info = app_info(cx);
        if info.onboarding && onboarding::pending(info.binary) {
            root.show_onboarding(window, cx);
        }
        root
    }

    pub fn show_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.new(Onboarding::new);
        let subscription = cx.subscribe_in(&view, window, |this, _, event, window, cx| {
            match event {
                OnboardingEvent::Finished => {
                    this.onboarding = None;
                    if let Some(size) = this.before_onboarding.take() {
                        window.resize(size);
                    }
                    // Hand focus back to the app's view.
                    window.focus_next(cx);
                    cx.notify();
                }
            }
        });
        self.enlarge_for_onboarding(window, cx);
        let focus = view.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        self.onboarding = Some((view, subscription));
        cx.notify();
    }

    /// Onboarding needs room: the window grows 20% wider and 8% taller, or
    /// fills the screen when that would not fit.
    fn enlarge_for_onboarding(&mut self, window: &mut Window, cx: &App) {
        if self.before_onboarding.is_some() || window.is_fullscreen() || window.is_maximized() {
            return;
        }
        let current = window.bounds().size;
        let wanted = gpui::size(current.width * 1.2, current.height * 1.08);
        let fits = window.display(cx).is_none_or(|display| {
            let screen = display.visible_bounds().size;
            wanted.width <= screen.width * 0.95 && wanted.height <= screen.height * 0.95
        });
        if fits {
            self.before_onboarding = Some(current);
            window.resize(wanted);
        } else {
            window.zoom_window();
        }
    }

    fn render_toasts(&self, cx: &App) -> Option<impl IntoElement> {
        let toasts = cx.try_global::<Toasts>()?;
        if toasts.items.is_empty() {
            return None;
        }
        let position = notify::load(cx).position;
        let colors = cx.theme().colors();
        let info = app_info(cx);
        let stack = v_flex()
            .gap_2()
            .w(px(320.))
            .children(toasts.items.iter().map(|toast| {
                let id = toast.id;
                let event = info.events.iter().find(|e| e.id == toast.event);
                h_flex()
                    .items_start()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(colors.border_variant)
                    .bg(colors.elevated_surface_background)
                    .shadow_lg()
                    .when_some(event, |this, event| {
                        this.child(crate::settings::widgets::icon_tile(
                            event.icon,
                            gpui::rgb(event.tint).into(),
                            px(24.),
                        ))
                    })
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                Label::new(toast.title.clone()).weight(gpui::FontWeight::SEMIBOLD),
                            )
                            .child(
                                Label::new(toast.body.clone())
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            ),
                    )
                    .child(
                        IconButton::new(("dismiss-toast", id as usize), IconName::Close)
                            .on_click(move |_, _, cx| notify::dismiss(id, cx)),
                    )
            }));
        let layer = div().absolute().inset_0().p_4().flex().flex_col();
        let layer = if position.is_top() {
            layer.justify_start()
        } else {
            layer.justify_end()
        };
        let layer = match position {
            ToastPosition::TopLeft | ToastPosition::BottomLeft => layer.items_start(),
            ToastPosition::TopCenter | ToastPosition::BottomCenter => layer.items_center(),
            ToastPosition::TopRight | ToastPosition::BottomRight => layer.items_end(),
        };
        Some(layer.child(stack))
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_rem_size(crate::appearance::rem_size(cx));
        let background = crate::appearance::backdrop(cx).window_background();
        if self.background != Some(background) {
            window.set_background_appearance(background);
            self.background = Some(background);
        }
        let toasts = self.render_toasts(cx);
        crate::surface::backdrop(false, cx)
            .size_full()
            .relative()
            .on_action(
                cx.listener(|this, _: &ShowOnboarding, window, cx| {
                    this.show_onboarding(window, cx)
                }),
            )
            .child(self.app.clone())
            .children(self.onboarding.as_ref().map(|(view, _)| view.clone()))
            .children(toasts)
    }
}
