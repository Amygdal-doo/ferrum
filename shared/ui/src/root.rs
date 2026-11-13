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
                    // Hand focus back to the app's view.
                    window.focus_next(cx);
                    cx.notify();
                }
            }
        });
        let focus = view.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        self.onboarding = Some((view, subscription));
        cx.notify();
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
        let toasts = self.render_toasts(cx);
        div()
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
