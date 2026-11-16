//! Settings › Account (in every app's Settings window, ⌘,): who is signed
//! in, a link to the account page on the web, switching between the
//! launcher's account and one of the app's own, and sign-out; or **Sign
//! in**.

use ferrum_account::{AppMode, Scope};
use ferrum_ui::components::{Button, ButtonCommon as _, ButtonStyle, Clickable as _, IconName};
use ferrum_ui::gpui::{self, AnyElement, App, Context, Hsla, IntoElement, Window};
use ferrum_ui::settings::SettingsPage;
use ferrum_ui::settings::widgets::{Row, Section};

use crate::avatar::avatar;
use crate::state::AccountState;

fn act(cx: &mut App, f: impl FnOnce(&mut AccountState, &mut Context<AccountState>)) {
    if let Some(state) = crate::state(cx) {
        state.update(cx, f);
        cx.refresh_windows();
    }
}

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
            "sign in", "sign out", "login", "logout", "profile", "github", "switch",
        ]
    }
    fn render(&self, _: &mut Window, cx: &mut App) -> AnyElement {
        let Some(entity) = crate::state(cx) else {
            return gpui::div().into_any_element();
        };
        let state = entity.read(cx);
        if !state.available() {
            return Section::new("Ferrum account")
                .footnote("Signing in needs a home directory for ~/.ferrum/account.json.")
                .into_any_element();
        }
        let app_name = ferrum_ui::app_info(cx).name;
        let status = state.status();
        let launcher = state
            .launcher_user()
            .map(|u| format!("@{}", u.username))
            .unwrap_or_default();
        let mut section = Section::new("Ferrum account");
        if state.signing_in() {
            section = section.row(
                Row::new("account-waiting", "Finish signing in in your browser").control(
                    Button::new("account-page-cancel", "Cancel")
                        .style(ButtonStyle::Subtle)
                        .on_click(|_, _, cx| act(cx, |s, cx| s.cancel_sign_in(cx))),
                ),
            );
            return section.into_any_element();
        }
        match state.user() {
            None => {
                let detail = if status.offers_launcher_account() {
                    format!("FerrumLauncher is signed in as {launcher}.")
                } else {
                    "One Ferrum account signs you in to every Ferrum app on this computer."
                        .to_owned()
                };
                section = section.row(
                    Row::new(
                        "account-signed-out",
                        if state.checking() {
                            "Signing in…"
                        } else {
                            "Not signed in"
                        },
                    )
                    .detail(detail)
                    .control(
                        Button::new("account-page-sign-in", "Sign in")
                            .style(if status.offers_launcher_account() {
                                ButtonStyle::Subtle
                            } else {
                                ButtonStyle::Filled
                            })
                            .on_click(|_, _, cx| act(cx, |s, cx| s.sign_in(cx))),
                    ),
                );
            }
            Some(user) => {
                let mut detail = format!("@{}", user.username);
                if let Some(email) = user.email.as_deref().filter(|e| !e.is_empty()) {
                    detail.push_str(" · ");
                    detail.push_str(email);
                }
                section = section.row(
                    Row::new("account-user", user.display_name().to_owned())
                        .icon(avatar(user, state.avatar(), gpui::px(28.), cx))
                        .detail(detail)
                        .control(
                            Button::new("account-page-web", "Manage account on the web")
                                .style(ButtonStyle::Subtle)
                                .on_click(|_, _, cx| {
                                    if let Some(url) =
                                        crate::state(cx).and_then(|s| s.read(cx).account_url())
                                    {
                                        cx.open_url(&url);
                                    }
                                }),
                        ),
                );
            }
        }
        if status.offers_launcher_account() {
            section = section.row(
                Row::new("account-use-launcher", "Use launcher account")
                    .detail(format!(
                        "Sign {app_name} in as {launcher}, the account FerrumLauncher uses."
                    ))
                    .control(
                        Button::new("account-page-use-launcher", "Use launcher account")
                            .style(ButtonStyle::Filled)
                            .on_click(|_, _, cx| act(cx, |s, cx| s.use_launcher_account(cx))),
                    ),
            );
        }
        if state.user().is_none() {
            return section.into_any_element();
        }
        if !status.app.is_launcher() {
            let detail = if status.mode == AppMode::OwnAccount {
                format!("{app_name} uses its own account. Sign in as someone else in the browser.")
            } else {
                format!("Sign {app_name} in as someone else; other apps keep their account.")
            };
            section = section.row(
                Row::new("account-switch", "Switch account")
                    .detail(detail)
                    .control(
                        Button::new("account-page-switch", "Switch account…")
                            .style(ButtonStyle::Subtle)
                            .on_click(|_, _, cx| act(cx, |s, cx| s.sign_in(cx))),
                    ),
            );
        }
        let here_detail = if status.app.is_launcher() {
            "Apps that use the launcher's account are signed out too.".to_owned()
        } else {
            "Other Ferrum apps stay signed in.".to_owned()
        };
        section
            .row(
                Row::new("account-sign-out", format!("Sign out of {app_name}"))
                    .detail(here_detail)
                    .control(
                        Button::new("account-page-sign-out", "Sign out")
                            .style(ButtonStyle::Subtle)
                            .on_click(|_, _, cx| {
                                act(cx, |s, cx| s.sign_out(Scope::ThisApp, cx));
                            }),
                    ),
            )
            .row(
                Row::new("account-sign-out-all", "Sign out everywhere")
                    .detail(
                        "Every Ferrum app on this computer, including ones signed in on their own.",
                    )
                    .control(
                        Button::new("account-page-sign-out-all", "Sign out everywhere")
                            .style(ButtonStyle::Subtle)
                            .on_click(|_, _, cx| {
                                act(cx, |s, cx| s.sign_out(Scope::Everywhere, cx));
                            }),
                    ),
            )
            .into_any_element()
    }
}
