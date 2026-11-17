//! The project's team: your public key (to give to teammates) and the
//! members' public keys that team bundles are encrypted to. Public keys
//! are not secret; checking a pasted key's fingerprint with its owner is
//! what makes it trustworthy.

use ferrum_ui::prelude::*;
use ferrumenv_core::PublicKey;

use super::{Dialog, input};
use crate::app::EnvApp;
use crate::widgets::{field, icon_button, muted, primary_button, text_button};

pub(crate) struct TeamDialog {
    pub(crate) name: Entity<TextInput>,
    pub(crate) key: Entity<TextInput>,
}

impl EnvApp {
    /// Your public key, creating your key pair (and saving the vault) the
    /// first time.
    pub(crate) fn my_public_key(&mut self, cx: &mut Context<Self>) -> Option<PublicKey> {
        if let Some(key) = self
            .vault()
            .and_then(|u| u.vault.identity.as_ref())
            .and_then(|i| i.public_key().ok())
        {
            return Some(key);
        }
        self.change(cx, |u| {
            let identity = u.vault.ensure_identity().map_err(|e| e.to_string())?;
            identity.public_key().map_err(|e| e.to_string())
        })
    }

    pub(crate) fn open_team(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.vault().and_then(|u| u.project()).is_none() || self.my_public_key(cx).is_none() {
            return;
        }
        let mut subscriptions = Vec::new();
        let name = input(window, cx, &mut subscriptions, |t| {
            t.placeholder("Name, e.g. Ana")
        });
        let key = input(window, cx, &mut subscriptions, |t| {
            t.monospace().placeholder("fepk1_…")
        });
        let dialog = TeamDialog {
            name: name.clone(),
            key,
        };
        self.open_dialog(Dialog::Team(dialog), subscriptions, Some(name), window, cx);
    }

    pub(crate) fn add_member(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Team(d)) = self.dialog_mut() else {
            return;
        };
        let (name_input, key_input) = (d.name.clone(), d.key.clone());
        let name = name_input.read(cx).text().to_string();
        let text = key_input.read(cx).text().to_string();
        if text.trim().is_empty() {
            self.set_dialog_error("Paste the public key your teammate sent you.", cx);
            return;
        }
        let key = match PublicKey::parse(&text) {
            Ok(key) => key,
            Err(e) => {
                self.set_dialog_error(e.to_string(), cx);
                return;
            }
        };
        let me = self.my_public_key(cx);
        let added = self.change(cx, |u| {
            let project = u.project_mut().ok_or("No project selected")?;
            project
                .add_member(&name, key, me.as_ref())
                .map_err(|e| e.to_string())
        });
        if added.is_some() {
            name_input.update(cx, |input, cx| input.set_text("", cx));
            key_input.update(cx, |input, cx| input.set_text("", cx));
            if let Some(u) = self.vault_mut() {
                u.dialog_error = None;
            }
            self.show_info(
                format!(
                    "Added. Check the fingerprint {} with them before sharing.",
                    key.fingerprint()
                ),
                cx,
            );
        }
    }

    fn remove_member(&mut self, index: usize, cx: &mut Context<Self>) {
        self.change(cx, |u| {
            let project = u.project_mut().ok_or("No project selected")?;
            if index >= project.members.len() {
                return Err("That member no longer exists".into());
            }
            project.members.remove(index);
            Ok(())
        });
    }

    pub(crate) fn render_team(&self, d: &TeamDialog, cx: &mut Context<Self>) -> Modal {
        let colors = cx.theme().colors().clone();
        let modal = self.modal("Team keys", cx).width(px(600.));
        let Some(u) = self.vault() else {
            return modal;
        };
        let (Some(project), Some(me)) = (
            u.project(),
            u.vault.identity.as_ref().and_then(|i| i.public_key().ok()),
        ) else {
            return modal;
        };
        let mut members = v_flex().gap_1();
        if project.members.is_empty() {
            members = members.child(muted(
                "No teammates yet. Team bundles of this project can only be opened by you.",
            ));
        }
        for (i, member) in project.members.iter().enumerate() {
            members = members.child(
                h_flex()
                    .gap_2()
                    .child(div().w(px(140.)).truncate().child(member.name.clone()))
                    .child(
                        div()
                            .flex_1()
                            .font_buffer(cx)
                            .text_ui_xs(cx)
                            .child(member.public_key.fingerprint()),
                    )
                    .child(icon_button(
                        SharedString::from(format!("remove-member-{i}")),
                        IconName::Trash,
                        "Remove from the team",
                        cx,
                        move |this, _, cx| this.remove_member(i, cx),
                    )),
            );
        }
        modal
            .child(muted(
                "Team bundles are encrypted to each member's public key, so there is no shared password. Only people listed here (and you) can open them.",
            ))
            .child(field(
                "Your public key (not secret: send it to your teammates)",
                h_flex()
                    .gap_2()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.editor_background)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_buffer(cx)
                            .text_ui_xs(cx)
                            .child(me.encode()),
                    )
                    .child(text_button("copy-public-key", "Copy", cx, |this, _, cx| {
                        let key = this
                            .vault()
                            .and_then(|u| u.vault.identity.as_ref())
                            .and_then(|i| i.public_key().ok());
                        if let Some(key) = key {
                            this.copy_plain(key.encode(), "Public key", cx);
                        }
                    })),
            ))
            .child(muted(format!("Your fingerprint: {}", me.fingerprint())))
            .child(field(format!("Members of {}", project.name), members))
            .child(field(
                "Add a teammate (compare the fingerprint with them, e.g. on a call)",
                h_flex()
                    .gap_2()
                    .child(div().w(px(140.)).child(d.name.clone()))
                    .child(div().flex_1().child(d.key.clone())),
            ))
            .footer(primary_button("add-member", "Add member", cx, |this, _, cx| {
                this.add_member(cx);
            }))
    }
}
