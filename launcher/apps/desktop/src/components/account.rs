//! The account control at the right end of the title row: **Sign in**
//! while signed out; the user's avatar once signed in, opening a menu
//! (username → the account page, organizations with their plan, Settings,
//! sign-out).

use crate::app::FerrumLauncher;
use ferrum_account::{Me, Scope};
use ferrum_ui::components::{ContentButton, IconPosition};
use ferrum_ui::gpui::{Anchor, FontWeight, Image, ObjectFit, StyledImage, img};
use ferrum_ui::prelude::*;
use std::sync::Arc;

impl FerrumLauncher {
    pub(crate) fn render_account(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.account.client.as_ref()?;
        if self.account.signing_in.is_some() {
            return Some(
                h_flex()
                    .gap_1()
                    .child(
                        Label::new("Finish signing in in your browser…")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Button::new("account-cancel-sign-in", "Cancel")
                            .size(ButtonSize::Compact)
                            .style(ButtonStyle::Subtle)
                            .on_click(cx.listener(|this, _, _, cx| this.cancel_sign_in(cx))),
                    )
                    .into_any_element(),
            );
        }
        let Some(user) = self.account.user.clone() else {
            return Some(
                Button::new("account-sign-in", "Sign in")
                    .size(ButtonSize::Compact)
                    .style(ButtonStyle::Subtle)
                    .tooltip(Tooltip::text(
                        "Sign in to your Ferrum account in the browser",
                    ))
                    .on_click(cx.listener(|this, _, _, cx| this.sign_in(cx)))
                    .into_any_element(),
            );
        };
        let weak = cx.weak_entity();
        let trigger = ContentButton::new(
            "account-avatar",
            avatar(&user, self.account.avatar.clone(), px(20.), cx),
        )
        .style(ButtonStyle::Subtle)
        .tooltip(Tooltip::text(format!("Signed in as @{}", user.username)));
        Some(
            PopoverMenu::new("account-menu")
                .anchor(Anchor::TopRight)
                .trigger(trigger)
                .menu(move |window, cx| {
                    let (weak, user) = (weak.clone(), user.clone());
                    Some(ContextMenu::build(window, cx, move |menu, _, _| {
                        account_menu(menu, &user, weak)
                    }))
                })
                .into_any_element(),
        )
    }
}

fn account_menu(menu: ContextMenu, user: &Me, weak: WeakEntity<FerrumLauncher>) -> ContextMenu {
    let (page, this_app, everywhere) = (weak.clone(), weak.clone(), weak);
    let mut menu = menu
        .icon_entry(
            format!("@{}", user.username),
            IconName::Person,
            move |_, cx| {
                if let Some(url) = page.upgrade().and_then(|l| l.read(cx).account_url()) {
                    cx.open_url(&url);
                }
            },
        )
        .separator()
        .header("Organization");
    for org in &user.organizations {
        menu = menu
            .toggleable_entry(
                org.name.clone(),
                org.personal,
                IconPosition::Start,
                None,
                |_, _| {},
            )
            .with_badge(org.plan_label());
    }
    menu.separator()
        .entry(
            "Settings",
            Some(Box::new(ferrum_ui::OpenSettings)),
            |_, cx| ferrum_ui::open_settings(None, cx),
        )
        .separator()
        .entry("Sign out of FerrumLauncher", None, move |_, cx| {
            this_app
                .update(cx, |this, cx| this.sign_out(Scope::ThisApp, cx))
                .ok();
        })
        .entry("Sign out everywhere", None, move |_, cx| {
            everywhere
                .update(cx, |this, cx| this.sign_out(Scope::Everywhere, cx))
                .ok();
        })
}

/// A round avatar: the downloaded picture, else the user's initials.
pub fn avatar(user: &Me, image: Option<Arc<Image>>, size: Pixels, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let initials: SharedString = user.initials().into();
    let fallback = {
        let (bg, fg) = (colors.element_selected, colors.text);
        move || {
            div()
                .size_full()
                .rounded_full()
                .bg(bg)
                .text_color(fg)
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .flex()
                .items_center()
                .justify_center()
                .child(initials.clone())
                .into_any_element()
        }
    };
    let circle = div()
        .flex_none()
        .size(size)
        .rounded_full()
        .overflow_hidden();
    match image {
        Some(image) => circle
            .child(
                img(image)
                    .size(size)
                    .rounded_full()
                    .object_fit(ObjectFit::Cover)
                    .with_fallback(fallback),
            )
            .into_any_element(),
        None => circle.child(fallback()).into_any_element(),
    }
}
