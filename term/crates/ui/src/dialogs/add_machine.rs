//! "Add a machine": the hosts of `~/.ssh/config` with an Add button each
//! (or "Added"), and a `user@host` field for anything else. Writes
//! `ssh-hosts.json` directly, so the same dialog serves the sidebar's
//! "+ Add machine" and Settings › Machines. Esc or the close button
//! dismisses it.

use ferrum_ui::components::{
    Button, ButtonCommon, ButtonStyle, Clickable, Color, Icon, IconName, IconSize, Label,
    LabelCommon, LabelSize, h_flex, v_flex,
};
use ferrum_ui::gpui::{
    Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, KeyDownEvent,
    ParentElement, Render, SharedString, Styled, Subscription, Window, div, prelude::*, px,
};
use ferrum_ui::theme::ActiveTheme;
use ferrum_ui::{Modal, TextInput, TextInputEvent};
use ferrumterm_net::HostStore;
use ferrumterm_net::ssh_config::{self, ConfigHost};

/// What the dialog tells its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddMachineEvent {
    /// A machine was added (and saved) under this name.
    Added(String),
    Dismissed,
}

pub struct AddMachine {
    config: Vec<ConfigHost>,
    store: HostStore,
    typed: Entity<TextInput>,
    error: Option<String>,
    focus: FocusHandle,
    _typed_events: Subscription,
}

impl EventEmitter<AddMachineEvent> for AddMachine {}

impl Focusable for AddMachine {
    fn focus_handle(&self, _: &ferrum_ui::gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

fn store_file() -> std::path::PathBuf {
    ferrumterm_client::paths::ssh_hosts_file()
}

/// `user@hostname:port` of a config host (its alias when it says nothing).
pub fn config_address(c: &ConfigHost) -> String {
    let mut s = format!(
        "{}{}",
        c.user.as_ref().map(|u| format!("{u}@")).unwrap_or_default(),
        c.hostname.as_deref().unwrap_or(&c.alias)
    );
    if let Some(p) = c.port.filter(|p| *p != 22) {
        s.push_str(&format!(":{p}"));
    }
    s
}

impl AddMachine {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let typed = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("user@host")
                .monospace()
        });
        let typed_events = cx.subscribe_in(&typed, window, |this, _, event, _, cx| match event {
            TextInputEvent::Submit => this.add_typed(cx),
            TextInputEvent::Changed => {
                this.error = None;
                cx.notify();
            }
            TextInputEvent::Blurred => {}
        });
        let store = HostStore::load(&store_file());
        let error = store.as_ref().err().cloned();
        let this = Self {
            config: ssh_config::load_user(),
            store: store.unwrap_or_default(),
            typed,
            error,
            focus: cx.focus_handle(),
            _typed_events: typed_events,
        };
        this.typed.update(cx, |input, cx| input.focus(window, cx));
        this
    }

    /// Applies `change` to the saved store, writes it, and reports `name`.
    fn save(
        &mut self,
        change: impl FnOnce(&mut HostStore) -> Result<String, String>,
        cx: &mut Context<Self>,
    ) {
        let result = HostStore::load(&store_file()).and_then(|mut store| {
            let name = change(&mut store)?;
            store.save(&store_file())?;
            Ok((store, name))
        });
        match result {
            Ok((store, name)) => {
                self.store = store;
                self.error = None;
                cx.emit(AddMachineEvent::Added(name));
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }

    fn add_config(&mut self, alias: String, cx: &mut Context<Self>) {
        self.save(
            move |store| {
                store.add_config_host(&alias);
                Ok(alias)
            },
            cx,
        );
    }

    fn add_typed(&mut self, cx: &mut Context<Self>) {
        let text = self.typed.read(cx).text().trim().to_owned();
        if text.is_empty() {
            self.error = Some("Type how ssh reaches it, e.g. me@192.168.1.20".into());
            cx.notify();
            return;
        }
        self.save(move |store| store.add_typed(&text), cx);
        if self.error.is_none() {
            self.typed.update(cx, |input, cx| input.set_text("", cx));
        }
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        cx.emit(AddMachineEvent::Dismissed);
    }
}

impl Render for AddMachine {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let mut list = v_flex().gap_0p5();
        if self.config.is_empty() {
            list = list.child(
                Label::new("No hosts in ~/.ssh/config.")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            );
        }
        for (i, host) in self.config.iter().enumerate() {
            let added = self.store.is_added(&host.alias);
            let alias = host.alias.clone();
            let action = if added {
                h_flex()
                    .gap_1()
                    .text_size(px(12.))
                    .text_color(colors.text_muted)
                    .child(
                        Icon::new(IconName::Check)
                            .size(IconSize::XSmall)
                            .color(Color::Success),
                    )
                    .child("Added")
                    .into_any_element()
            } else {
                Button::new(("add-config-machine", i), "Add")
                    .style(ButtonStyle::Subtle)
                    .on_click(cx.listener(move |this, _, _, cx| this.add_config(alias.clone(), cx)))
                    .into_any_element()
            };
            list = list.child(
                h_flex()
                    .id(("config-machine", i))
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .hover(|s| s.bg(colors.element_hover))
                    .child(
                        Icon::new(IconName::Server)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().truncate().child(host.alias.clone()))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.))
                                    .text_color(colors.text_muted)
                                    .child(config_address(host)),
                            ),
                    )
                    .child(action),
            );
        }
        let mut modal = Modal::new("Add a machine")
            .width(px(500.))
            .on_dismiss(cx.listener(|this, _, _, cx| this.dismiss(cx)))
            .child(
                Label::new(
                    "A VPS, a Mac mini, a box at home: FerrumTerm reaches it over ssh. Its \
                     terminals run in the FerrumTerm server on this Mac, so they stay open \
                     when you close the window.",
                )
                .size(LabelSize::Small)
                .color(Color::Muted),
            )
            .child(section_title("From your ssh config"))
            .child(
                div()
                    .id("config-machines")
                    .max_h(px(260.))
                    .overflow_y_scroll()
                    .child(list),
            )
            .child(section_title("Or type how ssh reaches it"))
            .child(
                h_flex()
                    .gap_2()
                    .child(div().flex_1().child(self.typed.clone()))
                    .child(
                        Button::new("add-typed-machine", "Add")
                            .style(ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, _, cx| this.add_typed(cx))),
                    ),
            );
        if let Some(error) = &self.error {
            modal = modal.child(
                Label::new(SharedString::from(error.clone()))
                    .size(LabelSize::Small)
                    .color(Color::Error),
            );
        }
        div()
            .id("add-machine")
            .absolute()
            .inset_0()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    this.dismiss(cx);
                }
            }))
            .child(modal)
    }
}

fn section_title(text: &'static str) -> impl IntoElement {
    Label::new(text)
        .size(LabelSize::Small)
        .weight(ferrum_ui::gpui::FontWeight::SEMIBOLD)
}
