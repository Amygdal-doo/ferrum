//! Remote machines: the machine picker in the table header, the "Add a
//! machine" dialog, and the host list (`~/.ferrum/ports/ssh-hosts.json`,
//! plus `~/.ssh/config` aliases, as in FerrumTerm).

use ferrum_ui::components::IconPosition;
use ferrum_ui::prelude::*;
use ferrumports_core::remote;
use ferrumterm_net::Host;

use crate::app::FerrumPorts;

impl FerrumPorts {
    /// The remote host the table shows, `None` for this Mac.
    pub fn target_host(&self) -> Option<Host> {
        let name = self.target.as_deref()?;
        remote::hosts(&self.hosts, &self.config_hosts)
            .into_iter()
            .find(|h| h.name == name)
    }

    /// Shows `target`'s ports (`None`: this Mac) and scans it now.
    pub fn set_target(&mut self, target: Option<String>, cx: &mut Context<Self>) {
        if self.target == target {
            return;
        }
        self.target = target;
        self.listeners.clear();
        self.selected = None;
        self.loaded = false;
        self.error = None;
        self.warnings.clear();
        self.start_scan(cx);
    }

    fn save_hosts(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.hosts_path.clone() else {
            return;
        };
        if let Err(e) = self.hosts.save(&path) {
            self.show_notice(Notice::error(e), cx);
        }
    }

    /// Adds a `~/.ssh/config` alias and shows its ports.
    pub fn add_config_host(&mut self, alias: &str, cx: &mut Context<Self>) {
        self.hosts.add_config_host(alias);
        self.save_hosts(cx);
        self.add_host = None;
        self.set_target(Some(alias.to_string()), cx);
    }

    /// Adds the host typed in the dialog (`user@host[:port]`).
    pub fn add_typed_host(&mut self, cx: &mut Context<Self>) {
        let Some(input) = self.add_host.clone() else {
            return;
        };
        let text = input.read(cx).text().to_string();
        match self.hosts.add_typed(&text) {
            Ok(name) => {
                self.save_hosts(cx);
                self.add_host = None;
                self.set_target(Some(name), cx);
            }
            Err(e) => self.show_notice(Notice::error(e), cx),
        }
    }

    pub fn remove_host(&mut self, name: &str, cx: &mut Context<Self>) {
        if self.hosts.remove_machine(name) {
            self.save_hosts(cx);
        }
        if self.target.as_deref() == Some(name) {
            self.set_target(None, cx);
        }
        cx.notify();
    }

    pub fn open_add_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input =
            cx.new(|cx| TextInput::new(window, cx).placeholder("user@host or user@host:2222"));
        self._subscriptions
            .push(cx.subscribe(&input, |app: &mut Self, _, event, cx| {
                if *event == TextInputEvent::Submit {
                    app.add_typed_host(cx);
                }
            }));
        input.update(cx, |input, cx| input.focus(window, cx));
        self.add_host = Some(input);
        cx.notify();
    }
}

/// "This Mac ▾" / "prod ▾": picks the machine whose ports are listed.
pub fn host_picker(app: &FerrumPorts, cx: &mut Context<FerrumPorts>) -> AnyElement {
    let this = cx.entity().downgrade();
    let current = app.target.clone();
    let hosts: Vec<(String, String)> = remote::hosts(&app.hosts, &app.config_hosts)
        .into_iter()
        .map(|h| {
            let address = h.address();
            (h.name, address)
        })
        .collect();
    let label = current.clone().unwrap_or_else(|| "This Mac".to_string());
    let build = move |window: &mut Window, cx: &mut App| {
        let this = this.clone();
        let hosts = hosts.clone();
        let current = current.clone();
        ContextMenu::build(window, cx, move |mut menu, _, _| {
            menu = menu.header("Ports of");
            let choices = std::iter::once((None, "This Mac".to_string())).chain(hosts.iter().map(
                |(name, address)| {
                    let label = if address == name {
                        name.clone()
                    } else {
                        format!("{name}  ({address})")
                    };
                    (Some(name.clone()), label)
                },
            ));
            for (target, label) in choices {
                let this = this.clone();
                let toggled = target == current;
                menu = menu.toggleable_entry(label, toggled, IconPosition::Start, None, {
                    move |_, cx| {
                        this.update(cx, |app, cx| app.set_target(target.clone(), cx))
                            .ok();
                    }
                });
            }
            menu = menu.separator();
            let add = this.clone();
            menu = menu.icon_entry("Add a machine…", IconName::Plus, move |window, cx| {
                add.update(cx, |app, cx| app.open_add_host(window, cx)).ok();
            });
            if let Some(name) = current.clone() {
                let remove = this.clone();
                menu = menu.icon_entry(format!("Remove {name}"), IconName::Trash, move |_, cx| {
                    remove.update(cx, |app, cx| app.remove_host(&name, cx)).ok();
                });
            }
            menu
        })
    };
    PopoverMenu::new("host-picker")
        .menu(move |window, cx| Some(build(window, cx)))
        .trigger(
            Button::new("host-picker-button", label)
                .style(ButtonStyle::Subtle)
                .size(ButtonSize::Compact)
                .label_size(LabelSize::Small)
                .end_icon(Icon::new(IconName::ChevronDown).size(IconSize::XSmall))
                .tooltip(Tooltip::text("Machine whose ports are listed (ssh)")),
        )
        .into_any_element()
}

/// "Add a machine": `~/.ssh/config` aliases not added yet, or a typed host.
pub fn add_host_dialog(app: &FerrumPorts, cx: &mut Context<FerrumPorts>) -> Option<AnyElement> {
    let input = app.add_host.clone()?;
    let colors = cx.theme().colors().clone();
    let aliases: Vec<String> = app
        .config_hosts
        .iter()
        .map(|c| c.alias.clone())
        .filter(|alias| !app.hosts.is_added(alias))
        .collect();
    let close = |app: &mut FerrumPorts, cx: &mut Context<FerrumPorts>| {
        app.add_host = None;
        cx.notify();
    };
    Some(
        Modal::new("Add a machine")
            .width(px(480.))
            .on_dismiss(cx.listener(move |app, _, _, cx| close(app, cx)))
            .child(
                Label::new(
                    "FerrumPorts lists its ports with ss or lsof over ssh. Keys must be in \
                     ssh-agent or the keychain: nothing can ask for a password.",
                )
                .size(LabelSize::Small)
                .color(Color::Muted),
            )
            .when(!aliases.is_empty(), |modal| {
                modal.child(
                    v_flex()
                        .gap_1()
                        .child(Label::new("From ~/.ssh/config").size(LabelSize::Small))
                        .child(
                            v_flex()
                                .id("config-hosts")
                                .max_h(px(180.))
                                .overflow_y_scroll()
                                .children(aliases.into_iter().map(|alias| {
                                    let pick = alias.clone();
                                    div()
                                        .id(SharedString::from(format!("config-{alias}")))
                                        .px_2()
                                        .py_1()
                                        .rounded_sm()
                                        .cursor_pointer()
                                        .hover(|s| s.bg(colors.element_hover))
                                        .child(Label::new(alias).size(LabelSize::Small))
                                        .on_click(cx.listener(move |app, _, _, cx| {
                                            app.add_config_host(&pick, cx)
                                        }))
                                })),
                        ),
                )
            })
            .child(
                v_flex()
                    .gap_1()
                    .child(Label::new("Or type a host").size(LabelSize::Small))
                    .child(input),
            )
            .footer(
                Button::new("add-host-cancel", "Cancel")
                    .on_click(cx.listener(move |app, _, _, cx| close(app, cx))),
            )
            .footer(
                Button::new("add-host-confirm", "Add")
                    .style(ButtonStyle::Filled)
                    .on_click(cx.listener(|app, _, _, cx| app.add_typed_host(cx))),
            )
            .into_any_element(),
    )
}
