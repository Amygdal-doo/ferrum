//! Add / edit / delete a saved SSH host and edit any host's port forwards.
//! Hosts from `~/.ssh/config` are shown read-only (only their forwards,
//! kept in FerrumTerm's own file, can change).

use super::{Dialog, error_text, field, hint, modal};
use crate::view::TermView;
use ferrum_ui::TextInput;
use ferrum_ui::gpui::{AnyElement, Context, Entity, Window, prelude::*};
use ferrum_ui::ui::{Button, ButtonCommon, ButtonStyle, Clickable, TintColor, h_flex};
use ferrumterm_net::HostSource;
use ferrumterm_net::hosts::{HostFormFields, SavedHost, find_host};
use ferrumterm_protocol::{ForwardSpec, ForwardState};

pub struct HostForm {
    /// Name of the saved host being edited; `None` = a new host.
    pub editing: Option<String>,
    /// Alias of the `~/.ssh/config` host whose forwards are edited.
    pub config_alias: Option<String>,
    pub name: Entity<TextInput>,
    pub user: Entity<TextInput>,
    pub host: Entity<TextInput>,
    pub port: Entity<TextInput>,
    pub identity: Entity<TextInput>,
    pub jump: Entity<TextInput>,
    pub command: Entity<TextInput>,
    pub forwards: Entity<TextInput>,
    pub error: Option<String>,
    pub confirm_delete: bool,
}

impl TermView {
    /// Opens the form for `name` (saved or ssh-config host), or a blank one.
    pub(crate) fn open_host_form(
        &mut self,
        name: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self._dialog_events.clear();
        let host =
            name.as_deref().and_then(|n| find_host(&self.app.ssh.store, &self.app.ssh.config, n));
        let (saved, config) = match host.as_ref().map(|h| &h.source) {
            Some(HostSource::Saved(h)) => (h.clone(), None),
            Some(HostSource::SshConfig(c)) => (
                SavedHost {
                    name: c.alias.clone(),
                    user: c.user.clone(),
                    host: c.hostname.clone().unwrap_or_default(),
                    port: c.port,
                    identity_file: c.identity_file.clone(),
                    jump_host: c.proxy_jump.clone(),
                    startup_command: None,
                },
                Some(c.alias.clone()),
            ),
            None => (SavedHost::default(), None),
        };
        let ro = config.is_some();
        let forwards = name
            .as_deref()
            .map(|n| {
                self.app
                    .ssh
                    .store
                    .forwards_of(n)
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
            .join(", ");
        let port = saved.port.map(|p| p.to_string()).unwrap_or_default();
        let opt = |o: &Option<String>| o.clone().unwrap_or_default();
        let form = HostForm {
            editing: name.clone().filter(|_| !ro && host.is_some()),
            config_alias: config,
            name: self.dialog_input("prod-db", &saved.name, ro, window, cx),
            user: self.dialog_input("user (optional)", &opt(&saved.user), ro, window, cx),
            host: self.dialog_input("host.example.com or 10.0.0.5", &saved.host, ro, window, cx),
            port: self.dialog_input("22", &port, ro, window, cx),
            identity: self.dialog_input(
                "~/.ssh/id_ed25519 (optional)",
                &opt(&saved.identity_file),
                ro,
                window,
                cx,
            ),
            jump: self.dialog_input("bastion (optional)", &opt(&saved.jump_host), ro, window, cx),
            command: self.dialog_input(
                "e.g. tmux new -A -s main (optional)",
                &opt(&saved.startup_command),
                ro,
                window,
                cx,
            ),
            forwards: self.dialog_input(
                "L 5432:localhost:5432, R 9000:localhost:3000, D 1080",
                &forwards,
                false,
                window,
                cx,
            ),
            error: None,
            confirm_delete: false,
        };
        let focus = if ro { form.forwards.clone() } else { form.name.clone() };
        self.open_dialog(Dialog::Host(form), Some(focus), window, cx);
    }

    pub(crate) fn save_host_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Host(form)) = &self.dialog else { return };
        let text = |e: &Entity<TextInput>| e.read(cx).text().to_owned();
        let mut store = self.app.ssh.store.clone();
        let result = ForwardSpec::parse_list(&text(&form.forwards)).and_then(|specs| {
            let name = match &form.config_alias {
                Some(alias) => alias.clone(),
                None => {
                    let host = SavedHost::from_form(&HostFormFields {
                        name: &text(&form.name),
                        user: &text(&form.user),
                        host: &text(&form.host),
                        port: &text(&form.port),
                        identity_file: &text(&form.identity),
                        jump_host: &text(&form.jump),
                        startup_command: &text(&form.command),
                    })?;
                    let name = host.name.clone();
                    store.upsert(host, form.editing.as_deref())?;
                    name
                }
            };
            store.set_forwards(&name, specs)?;
            Ok(name)
        });
        let saved = result.and_then(|name| {
            let previous = std::mem::replace(&mut self.app.ssh.store, store);
            self.app.save_ssh().map(|()| name).inspect_err(|_| self.app.ssh.store = previous)
        });
        match saved {
            Ok(name) => {
                self.app.notify(format!("Saved {name}"));
                self.close_dialog(window, cx);
                self.after_change(cx);
            }
            Err(e) => {
                if let Some(Dialog::Host(form)) = &mut self.dialog {
                    form.error = Some(e);
                }
                cx.notify();
            }
        }
    }

    /// First click asks; the second deletes the host and stops its forwards.
    fn delete_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Host(form)) = &mut self.dialog else { return };
        let Some(name) = form.editing.clone() else { return };
        if !form.confirm_delete {
            form.confirm_delete = true;
            cx.notify();
            return;
        }
        let running: Vec<String> = self
            .app
            .ssh
            .forwards
            .iter()
            .filter(|f| f.host == name && f.state == ForwardState::Running)
            .map(|f| f.key.clone())
            .collect();
        self.app.ssh.store.remove(&name);
        if let Err(e) = self.app.save_ssh() {
            self.app.notify(e);
            self.app.reload_ssh();
        } else {
            for key in running {
                self.app.clear_forward(key);
            }
            self.app.notify(format!("Deleted {name}"));
        }
        self.close_dialog(window, cx);
        self.after_change(cx);
    }

    pub(crate) fn render_host_form(&self, form: &HostForm, cx: &mut Context<Self>) -> AnyElement {
        let title = match (&form.config_alias, &form.editing) {
            (Some(alias), _) => format!("{alias} (from ~/.ssh/config)"),
            (None, Some(name)) => format!("Edit {name}"),
            (None, None) => "New SSH host".to_owned(),
        };
        let mut m = modal(title, cx).width(ferrum_ui::gpui::px(520.));
        if form.config_alias.is_some() {
            m = m.child(hint(
                "This host comes from ~/.ssh/config, which FerrumTerm never edits. \
                 Only its port forwards are saved here.",
            ));
        }
        m = m
            .child(field("Name", form.name.clone()))
            .child(
                h_flex()
                    .gap_2()
                    .child(field("User", form.user.clone()).w(ferrum_ui::gpui::px(130.)))
                    .child(field("Host", form.host.clone()).flex_1())
                    .child(field("Port", form.port.clone()).w(ferrum_ui::gpui::px(70.))),
            )
            .child(field("Identity file", form.identity.clone()))
            .child(field("Jump host (ssh -J)", form.jump.clone()))
            .child(field("Startup command", form.command.clone()))
            .child(field("Port forwards", form.forwards.clone()))
            .child(hint(
                "No passwords or keys are stored: ssh and ssh-agent handle passphrases. \
                 Forwards run in the background with BatchMode, so their key must be in ssh-agent.",
            ));
        if let Some(e) = &form.error {
            m = m.child(error_text(e.clone()));
        }
        if form.editing.is_some() {
            m = m.footer(
                Button::new(
                    "host-delete",
                    if form.confirm_delete { "Really delete?" } else { "Delete" },
                )
                .style(ButtonStyle::Tinted(TintColor::Error))
                .on_click(cx.listener(|view, _, window, cx| view.delete_host(window, cx))),
            );
        }
        m.footer(
            Button::new("host-cancel", "Cancel")
                .on_click(cx.listener(|view, _, window, cx| view.close_dialog(window, cx))),
        )
        .footer(
            Button::new("host-save", "Save")
                .style(ButtonStyle::Filled)
                .on_click(cx.listener(|view, _, window, cx| view.save_host_form(window, cx))),
        )
        .into_any_element()
    }
}
