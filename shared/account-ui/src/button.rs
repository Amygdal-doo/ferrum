//! The account control for the right end of an app's title row: **Sign
//! in** while signed out (a small menu when the launcher's account can be
//! used); the user's avatar once signed in, opening the account menu
//! (username → the account page, organizations with their plan, Settings,
//! the app's own entries, Switch account…, Use launcher account, sign-out).

use std::rc::Rc;

use ferrum_account::{Me, Scope};
use ferrum_ui::components::{ButtonSize, ContentButton, IconPosition};
use ferrum_ui::gpui::{Anchor, ElementId};
use ferrum_ui::prelude::*;

use crate::avatar::avatar;
use crate::model::{MenuItem, SignInChoice, Status};
use crate::state::AccountState;

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

/// An app-specific menu entry (e.g. FerrumIDE's Keymap and Themes).
#[derive(Clone)]
struct Extra {
    label: SharedString,
    keys: Option<SharedString>,
    handler: Handler,
}

/// The title-row control. Renders nothing until [`crate::init`] ran (and
/// when there is no home directory), so views shared with tests or
/// embedded in another app can include it unconditionally.
///
/// ```ignore
/// row.children(AccountButton::new()
///     .entry("Themes", Some("⌘K ⌘T"), |window, cx| { … })
///     .render(cx))
/// ```
#[derive(Default)]
pub struct AccountButton {
    id: Option<SharedString>,
    extras: Vec<Extra>,
}

impl AccountButton {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A distinct element id, for windows that show the control twice.
    #[must_use]
    pub fn id(mut self, id: impl Into<SharedString>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Adds an entry after Settings in the menu; `keys` is the shortcut
    /// shown on the right.
    #[must_use]
    pub fn entry(
        mut self,
        label: impl Into<SharedString>,
        keys: Option<&str>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.extras.push(Extra {
            label: label.into(),
            keys: keys.map(|k| SharedString::from(k.to_owned())),
            handler: Rc::new(handler),
        });
        self
    }

    /// The control, or `None` when the account is not set up in this app.
    pub fn render(self, cx: &mut App) -> Option<AnyElement> {
        let entity = crate::state(cx)?;
        let state = entity.read(cx);
        if !state.available() {
            return None;
        }
        let id = self.id.clone().unwrap_or_else(|| "account".into());
        let weak = entity.downgrade();
        if state.signing_in() {
            return Some(
                h_flex()
                    .flex_none()
                    .gap_1()
                    .child(
                        Label::new("Finish signing in in your browser…")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Button::new(sub(&id, "cancel-sign-in"), "Cancel")
                            .size(ButtonSize::Compact)
                            .style(ButtonStyle::Subtle)
                            .on_click(move |_, _, cx| {
                                weak.update(cx, |s, cx| s.cancel_sign_in(cx)).ok();
                            }),
                    )
                    .into_any_element(),
            );
        }
        let status = state.status();
        let Some(user) = state.user().cloned() else {
            return Some(sign_in_button(id, &status, state, weak));
        };
        let trigger = ContentButton::new(
            sub(&id, "avatar"),
            avatar(&user, state.avatar(), px(20.), cx),
        )
        .style(ButtonStyle::Subtle)
        .tooltip(Tooltip::text(format!("Signed in as @{}", user.username)));
        let launcher = state.launcher_user().map(|u| u.username.clone());
        let extras = Rc::new(self.extras);
        Some(
            div()
                .flex_none()
                .child(
                    PopoverMenu::new(sub(&id, "menu"))
                        .anchor(Anchor::TopRight)
                        .trigger(trigger)
                        .menu(move |window, cx| {
                            let (weak, user, extras, launcher) =
                                (weak.clone(), user.clone(), extras.clone(), launcher.clone());
                            Some(ContextMenu::build(window, cx, move |menu, _, cx| {
                                let name = ferrum_ui::app_info(cx).name;
                                let menu_context = MenuContext {
                                    user: &user,
                                    app_name: name,
                                    launcher: launcher.as_deref(),
                                    extras: &extras,
                                };
                                account_menu(menu, &status, &menu_context, &weak)
                            }))
                        }),
                )
                .into_any_element(),
        )
    }
}

/// `account-avatar` and the like.
fn sub(id: &SharedString, part: &str) -> ElementId {
    ElementId::Name(format!("{id}-{part}").into())
}

/// The plain control: [`AccountButton`] without app entries.
pub fn account_button(cx: &mut App) -> Option<AnyElement> {
    AccountButton::new().render(cx)
}

fn sign_in_button(
    id: SharedString,
    status: &Status,
    state: &AccountState,
    weak: WeakEntity<AccountState>,
) -> AnyElement {
    let label = if state.checking() {
        "Signing in…"
    } else {
        "Sign in"
    };
    let button = Button::new(sub(&id, "sign-in"), label)
        .size(ButtonSize::Compact)
        .style(ButtonStyle::Subtle)
        .disabled(state.checking());
    match status.sign_in_choice() {
        SignInChoice::Browser => div()
            .flex_none()
            .child(
                button
                    .tooltip(Tooltip::text(
                        "Sign in to your Ferrum account in the browser",
                    ))
                    .on_click(move |_, _, cx| {
                        weak.update(cx, |s, cx| s.sign_in(cx)).ok();
                    }),
            )
            .into_any_element(),
        SignInChoice::LauncherOrBrowser => {
            let launcher = state
                .launcher_user()
                .map(|u| format!("Use launcher account (@{})", u.username))
                .unwrap_or_else(|| "Use launcher account".into());
            div()
                .flex_none()
                .child(
                    PopoverMenu::new(sub(&id, "sign-in-menu"))
                        .anchor(Anchor::TopRight)
                        .trigger(button)
                        .menu(move |window, cx| {
                            let (weak, launcher) = (weak.clone(), launcher.clone());
                            Some(ContextMenu::build(window, cx, move |menu, _, _| {
                                let browser = weak.clone();
                                menu.entry(launcher, None, move |_, cx| {
                                    weak.update(cx, |s, cx| s.use_launcher_account(cx)).ok();
                                })
                                .entry(
                                    "Sign in with another account…",
                                    None,
                                    move |_, cx| {
                                        browser.update(cx, |s, cx| s.sign_in(cx)).ok();
                                    },
                                )
                            }))
                        }),
                )
                .into_any_element()
        }
    }
}

struct MenuContext<'a> {
    user: &'a Me,
    app_name: &'a str,
    launcher: Option<&'a str>,
    extras: &'a [Extra],
}

fn act(
    weak: &WeakEntity<AccountState>,
    f: impl Fn(&mut AccountState, &mut Context<AccountState>) + 'static,
) -> impl Fn(&mut Window, &mut App) + 'static {
    let weak = weak.clone();
    move |_, cx| {
        weak.update(cx, |s, cx| f(s, cx)).ok();
    }
}

fn account_menu(
    mut menu: ContextMenu,
    status: &Status,
    context: &MenuContext<'_>,
    weak: &WeakEntity<AccountState>,
) -> ContextMenu {
    let mut group = None;
    for item in status.menu() {
        // A separator between groups, as in Zed's user menu.
        if group.is_some_and(|g| g != item.group()) {
            menu = menu.separator();
        }
        group = Some(item.group());
        menu = match item {
            MenuItem::Profile => {
                let page = weak.clone();
                menu.icon_entry(
                    format!("@{}", context.user.username),
                    IconName::Person,
                    move |_, cx| {
                        if let Some(url) = page.upgrade().and_then(|s| s.read(cx).account_url()) {
                            cx.open_url(&url);
                        }
                    },
                )
            }
            MenuItem::Organizations => {
                let mut menu = menu.header("Organization");
                for org in &context.user.organizations {
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
                menu
            }
            MenuItem::Settings => menu.entry(
                "Settings",
                Some(Box::new(ferrum_ui::OpenSettings)),
                |_, cx| ferrum_ui::open_settings(None, cx),
            ),
            MenuItem::AppEntries => {
                for extra in context.extras {
                    let handler = extra.handler.clone();
                    menu = menu.entry(extra.label.clone(), None, move |window, cx| {
                        handler(window, cx);
                    });
                    if let Some(keys) = &extra.keys {
                        menu = menu.with_keys(keys.clone());
                    }
                }
                menu
            }
            MenuItem::SwitchAccount => {
                menu.entry("Switch account…", None, act(weak, |s, cx| s.sign_in(cx)))
            }
            MenuItem::UseLauncherAccount => menu.entry(
                match context.launcher {
                    Some(name) => format!("Use launcher account (@{name})"),
                    None => "Use launcher account".to_owned(),
                },
                None,
                act(weak, |s, cx| s.use_launcher_account(cx)),
            ),
            MenuItem::SignOutHere => menu.entry(
                format!("Sign out of {}", context.app_name),
                None,
                act(weak, |s, cx| s.sign_out(Scope::ThisApp, cx)),
            ),
            MenuItem::SignOutEverywhere => menu.entry(
                "Sign out everywhere",
                None,
                act(weak, |s, cx| s.sign_out(Scope::Everywhere, cx)),
            ),
        };
    }
    menu
}
