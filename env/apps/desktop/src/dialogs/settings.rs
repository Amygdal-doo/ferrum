//! Settings: the auto-lock delay and changing the master password (which
//! re-encrypts the vault with a key from a new salt).

use std::time::Instant;

use ferrum_ui::prelude::*;
use ferrumenv_core::model::AUTO_LOCK_CHOICES;
use ferrumenv_core::password;
use zeroize::Zeroizing;

use super::{Dialog, input};
use crate::app::EnvApp;
use crate::lock_screen::strength_line;
use crate::widgets::{field, muted, primary_button};

pub(crate) struct SettingsDialog {
    pub(crate) current: Entity<TextInput>,
    pub(crate) new: Entity<TextInput>,
    pub(crate) confirm: Entity<TextInput>,
    /// Re-keying on a background thread.
    pub(crate) busy: bool,
}

impl EnvApp {
    pub(crate) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_unlocked() {
            return;
        }
        let mut subscriptions = Vec::new();
        let mut masked = |placeholder: &'static str| {
            input(window, cx, &mut subscriptions, move |t| {
                t.masked(true).placeholder(placeholder)
            })
        };
        let current = masked("Current master password");
        let new = masked("New master password");
        let confirm = masked("Repeat the new password");
        let dialog = SettingsDialog {
            current,
            new,
            confirm,
            busy: false,
        };
        self.open_dialog(Dialog::Settings(dialog), subscriptions, None, window, cx);
    }

    pub(crate) fn set_auto_lock(&mut self, minutes: u32, cx: &mut Context<Self>) {
        self.change(cx, |u| {
            u.vault.settings.auto_lock_minutes = minutes;
            u.auto_lock.set_minutes(minutes, Instant::now());
            Ok(())
        });
    }

    /// Verifies the current password and re-encrypts the vault under the
    /// new one, off the UI thread (Argon2 runs twice).
    pub(crate) fn change_password(&mut self, cx: &mut Context<Self>) {
        let params = self.config.kdf;
        let Some(u) = self.vault() else {
            return;
        };
        let Some(Dialog::Settings(d)) = &u.dialog else {
            return;
        };
        if d.busy {
            return;
        }
        let current = Zeroizing::new(d.current.read(cx).text().to_string());
        let new = Zeroizing::new(d.new.read(cx).text().to_string());
        let confirm = Zeroizing::new(d.confirm.read(cx).text().to_string());
        let mut session = u.session.duplicate();
        let vault = u.vault.clone();
        if let Err(message) = password::validate_new(&new, &confirm) {
            self.set_dialog_error(message, cx);
            return;
        }
        if let Some(Dialog::Settings(d)) = self.dialog_mut() {
            d.busy = true;
        }
        cx.notify();
        let task = cx.background_spawn(async move {
            session
                .change_password(&current, &new, params, &vault)
                .map(|()| session)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                if let Some(Dialog::Settings(d)) = this.dialog_mut() {
                    d.busy = false;
                }
                match result {
                    Ok(session) => {
                        let saved = this.change(cx, |u| {
                            u.session = session;
                            Ok(())
                        });
                        if saved.is_some() {
                            if let Some(Dialog::Settings(d)) = this.dialog_mut() {
                                for input in [&d.current, &d.new, &d.confirm] {
                                    input.update(cx, |input, cx| input.set_text("", cx));
                                }
                            }
                            if let Some(u) = this.vault_mut() {
                                u.dialog_error = None;
                            }
                            this.show_info("Master password changed.", cx);
                        }
                    }
                    Err(e) => this.set_dialog_error(e.to_string(), cx),
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn render_settings(
        &self,
        d: &SettingsDialog,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Modal {
        let minutes = self
            .vault()
            .map_or(30, |u| u.vault.settings.auto_lock_minutes);
        let mut choices = h_flex().gap_1();
        for choice in AUTO_LOCK_CHOICES {
            choices = choices.child(
                Button::new(
                    SharedString::from(format!("lock-{choice}")),
                    format!("{choice} min"),
                )
                .size(ButtonSize::Compact)
                .label_size(LabelSize::Small)
                .toggle_state(choice == minutes)
                .on_click(cx.listener(move |this, _, _, cx| this.set_auto_lock(choice, cx))),
            );
        }
        let strength = password::strength(d.new.read(cx).text());
        self.modal("Settings", cx)
            .width(px(480.))
            .child(field("Lock after inactivity", choices))
            .child(muted(
                "⌘L locks immediately. Locking wipes decrypted values from memory.",
            ))
            .child(Divider::horizontal())
            .child(Label::new("Change master password"))
            .child(d.current.clone())
            .child(d.new.clone())
            .child(d.confirm.clone())
            .child(strength_line(strength))
            .footer(
                primary_button(
                    "change-password",
                    if d.busy {
                        "Re-encrypting…"
                    } else {
                        "Change password"
                    },
                    cx,
                    |this, _, cx| this.change_password(cx),
                )
                .disabled(d.busy),
            )
    }
}
