//! Settings › Account (in the shared Settings window, ⌘,): who is signed
//! in, a link to the account page on the web, and sign-out; or a
//! **Sign in** button.

use ferrum_account::Scope;
use ferrum_ui::components::{Button, ButtonCommon as _, ButtonStyle, Clickable as _, IconName};
use ferrum_ui::gpui::{self, AnyElement, App, Entity, Hsla, IntoElement, Window};
use ferrum_ui::settings::SettingsPage;
use ferrum_ui::settings::widgets::{Row, Section};

use crate::app::FerrumLauncher;
use crate::settings_page::LauncherHandle;

fn launcher(cx: &App) -> Option<Entity<FerrumLauncher>> {
    cx.try_global::<LauncherHandle>()
        .and_then(|h| h.0.upgrade())
}

fn act(cx: &mut App, f: impl FnOnce(&mut FerrumLauncher, &mut gpui::Context<FerrumLauncher>)) {
    if let Some(l) = launcher(cx) {
        l.update(cx, f);
        cx.refresh_windows();
    }
}

#[derive(Default)]
pub struct AccountPage;

impl SettingsPage for AccountPage {
    fn id(&self) -> &'static str {
        "account"
    }
    fn title(&self) -> &'static str {
        "Account"
    }
    fn icon(&self) -> IconName {
        IconName::Person
    }
    fn tint(&self) -> Hsla {
        gpui::rgb(0x5b7cf0).into()
    }
    fn keywords(&self) -> &'static [&'static str] {
        &[
            "sign in", "sign out", "login", "logout", "profile", "github",
        ]
    }
    fn render(&self, _: &mut Window, cx: &mut App) -> AnyElement {
        let Some(entity) = launcher(cx) else {
            return gpui::div().into_any_element();
        };
        let ui = &entity.read(cx).account;
        if ui.client.is_none() {
            return Section::new("Ferrum account")
                .footnote("Signing in needs a home directory for ~/.ferrum/account.json.")
                .into_any_element();
        }
        let section = match (&ui.user, ui.signing_in.is_some()) {
            (_, true) => Section::new("Ferrum account").row(
                Row::new("account-waiting", "Finish signing in in your browser").control(
                    Button::new("account-page-cancel", "Cancel")
                        .style(ButtonStyle::Subtle)
                        .on_click(|_, _, cx| act(cx, |l, cx| l.cancel_sign_in(cx))),
                ),
            ),
            (None, false) => Section::new("Ferrum account").row(
                Row::new("account-signed-out", "Not signed in")
                    .detail("One Ferrum account signs you in to every Ferrum app on this computer.")
                    .control(
                        Button::new("account-page-sign-in", "Sign in")
                            .style(ButtonStyle::Filled)
                            .on_click(|_, _, cx| act(cx, |l, cx| l.sign_in(cx))),
                    ),
            ),
            (Some(user), false) => {
                let mut detail = format!("@{}", user.username);
                if let Some(email) = user.email.as_deref().filter(|e| !e.is_empty()) {
                    detail.push_str(" · ");
                    detail.push_str(email);
                }
                let avatar =
                    crate::components::account::avatar(user, ui.avatar.clone(), gpui::px(28.), cx);
                Section::new("Ferrum account")
                    .row(
                        Row::new("account-user", user.display_name().to_owned())
                            .icon(avatar)
                            .detail(detail)
                            .control(
                                Button::new("account-page-web", "Manage account on the web")
                                    .style(ButtonStyle::Subtle)
                                    .on_click(|_, _, cx| {
                                        if let Some(url) =
                                            launcher(cx).and_then(|l| l.read(cx).account_url())
                                        {
                                            cx.open_url(&url);
                                        }
                                    }),
                            ),
                    )
                    .row(
                        Row::new("account-sign-out", "Sign out of FerrumLauncher")
                            .detail("Apps that use the launcher's account are signed out too.")
                            .control(
                                Button::new("account-page-sign-out", "Sign out")
                                    .style(ButtonStyle::Subtle)
                                    .on_click(|_, _, cx| {
                                        act(cx, |l, cx| l.sign_out(Scope::ThisApp, cx));
                                    }),
                            ),
                    )
                    .row(
                        Row::new("account-sign-out-all", "Sign out everywhere")
                            .detail("Every Ferrum app on this computer, including ones signed in on their own.")
                            .control(
                                Button::new("account-page-sign-out-all", "Sign out everywhere")
                                    .style(ButtonStyle::Subtle)
                                    .on_click(|_, _, cx| {
                                        act(cx, |l, cx| l.sign_out(Scope::Everywhere, cx));
                                    }),
                            ),
                    )
            }
        };
        section.into_any_element()
    }
}
