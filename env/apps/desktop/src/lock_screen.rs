//! The lock screen: create the master password on first run, enter it on
//! every start and after an auto-lock. Key derivation (Argon2id) runs on
//! a background thread so the window stays responsive.

use ferrum_ui::prelude::*;
use ferrumenv_core::password::{self, Strength};
use ferrumenv_core::store::Session;
use ferrumenv_core::{Error, Vault};
use zeroize::Zeroizing;

use crate::app::{EnvApp, Screen};
use crate::widgets::{error_text, field};

pub(crate) struct LockForm {
    pub password: Entity<TextInput>,
    pub confirm: Entity<TextInput>,
    pub error: Option<SharedString>,
    /// Deriving the key; the form is disabled.
    pub busy: bool,
    _subscriptions: Vec<Subscription>,
}

impl LockForm {
    pub fn new(window: &mut Window, cx: &mut Context<EnvApp>) -> Self {
        let password = cx.new(|cx| {
            TextInput::new(window, cx)
                .masked(true)
                .placeholder("Master password")
        });
        let confirm = cx.new(|cx| {
            TextInput::new(window, cx)
                .masked(true)
                .placeholder("Repeat the master password")
        });
        let subscriptions = [&password, &confirm]
            .into_iter()
            .map(|input| {
                cx.subscribe_in(input, window, |this, _, event, window, cx| match event {
                    TextInputEvent::Submit => this.submit_lock_form(window, cx),
                    TextInputEvent::Changed => cx.notify(),
                    TextInputEvent::Blurred => {}
                })
            })
            .collect();
        Self {
            password,
            confirm,
            error: None,
            busy: false,
            _subscriptions: subscriptions,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        let handle = self.password.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    }

    /// Empties both fields.
    pub fn reset(&mut self, cx: &mut App) {
        self.password.update(cx, |input, cx| input.set_text("", cx));
        self.confirm.update(cx, |input, cx| input.set_text("", cx));
        self.busy = false;
        self.error = None;
    }
}

impl EnvApp {
    pub(crate) fn submit_lock_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.form.busy {
            return;
        }
        let password = Zeroizing::new(self.form.password.read(cx).text().to_string());
        let path = self.config.vault_path.clone();
        let task = match self.screen {
            Screen::Unlocked(_) => return,
            Screen::Setup => {
                let confirm = Zeroizing::new(self.form.confirm.read(cx).text().to_string());
                if let Err(message) = password::validate_new(&password, &confirm) {
                    self.form.error = Some(message.into());
                    cx.notify();
                    return;
                }
                let params = self.config.kdf;
                cx.background_spawn(async move {
                    let vault = Vault::default();
                    Session::create(&path, &password, params, &vault).map(|s| (s, vault))
                })
            }
            Screen::Locked => {
                if password.is_empty() {
                    return;
                }
                cx.background_spawn(async move { Session::unlock(&path, &password) })
            }
        };
        self.form.reset(cx);
        self.form.busy = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                // A vault deleted behind our back: start over.
                if let Err(Error::Io(e)) = &result
                    && e.kind() == std::io::ErrorKind::NotFound
                {
                    this.screen = Screen::Setup;
                }
                this.finish_unlock(result, window, cx);
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn render_lock_screen(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let setup = matches!(self.screen, Screen::Setup);
        let busy = self.form.busy;
        let strength = password::strength(self.form.password.read(cx).text());
        let mut card = ferrum_ui::pane_card(true, cx)
            .w(px(400.))
            .gap_3()
            .p_6()
            .child(
                h_flex()
                    .gap_2()
                    .child(Icon::new(IconName::Lock).color(Color::Accent))
                    .child(Headline::new("FerrumVariables").size(HeadlineSize::Small)),
            );
        if setup {
            card = card
                .child(Label::new(
                    "Create a master password. It encrypts every project, environment and variable in this vault.",
                ).size(LabelSize::Small).color(Color::Muted))
                .child(
                    h_flex()
                        .gap_2()
                        .p_2()
                        .rounded_md()
                        .bg(cx.theme().status().warning_background)
                        .child(Icon::new(IconName::Warning).color(Color::Warning))
                        .child(
                            div().flex_1().min_w_0().child(
                                Label::new(
                                    "There is no way to recover it. If you forget this password, the vault cannot be opened.",
                                )
                                .size(LabelSize::Small),
                            ),
                        ),
                )
                .child(field("Master password", self.form.password.clone()))
                .child(field("Repeat", self.form.confirm.clone()))
                .child(strength_line(strength));
        } else {
            card = card
                .child(
                    Label::new("The vault is locked. Enter your master password.")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .child(self.form.password.clone());
        }
        let label = match (setup, busy) {
            (_, true) => "Deriving key…",
            (true, false) => "Create vault",
            (false, false) => "Unlock",
        };
        card = card
            .when_some(self.form.error.clone(), |card, error| {
                card.child(error_text(error))
            })
            .child(
                Button::new("unlock", label)
                    .style(ButtonStyle::Filled)
                    .full_width()
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, window, cx| this.submit_lock_form(window, cx))),
            );
        // A bare title row (traffic lights, window drag) over the centred card.
        v_flex()
            .size_full()
            .child(ferrum_ui::title_row("lock-title", true, window))
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .justify_center()
                    .pb(ferrum_ui::title_row_height())
                    .child(card),
            )
            .into_any_element()
    }
}

pub(crate) fn strength_line(strength: Strength) -> impl IntoElement {
    let color = match strength {
        Strength::TooShort | Strength::Weak => Color::Error,
        Strength::Fair => Color::Warning,
        Strength::Good | Strength::Strong => Color::Success,
    };
    h_flex()
        .gap_2()
        .child(
            Label::new(strength.label())
                .size(LabelSize::Small)
                .color(color),
        )
        .child(
            Label::new(strength.hint())
                .size(LabelSize::Small)
                .color(Color::Muted),
        )
}
