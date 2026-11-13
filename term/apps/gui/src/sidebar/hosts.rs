//! Sidebar "SSH hosts" section: saved and `~/.ssh/config` hosts (click to
//! open an ssh tab), their port forwards (click to start / stop; a dot shows
//! running, failed or stopped), and the "What's using a port?" action.

use super::heading;
use crate::view::TermView;
use ferrum_ui::components::{
    ButtonCommon, Clickable, IconButton, IconName, IconSize, Tooltip, h_flex, v_flex,
};
use ferrum_ui::gpui::{Context, Hsla, SharedString, div, prelude::*, px};
use ferrum_ui::theme::ActiveTheme;
use ferrumterm_protocol::{ForwardInfo, ForwardSpec, ForwardState};

impl TermView {
    pub(crate) fn render_hosts(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let hosts = self.app.ssh.hosts();
        let mut section = v_flex().mt_2().child(
            h_flex()
                .pr_1()
                .child(heading("SSH hosts", cx))
                .child(div().flex_1())
                .child(
                    IconButton::new("add-ssh-host", IconName::Plus)
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text("Add SSH host"))
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.open_host_form(None, window, cx);
                        })),
                ),
        );
        if hosts.is_empty() {
            section = section.child(
                div()
                    .px_3()
                    .py_1()
                    .text_size(px(12.))
                    .text_color(colors.text_placeholder)
                    .child("No hosts. Add one with +, or in ~/.ssh/config"),
            );
        }
        for (i, host) in hosts.iter().enumerate() {
            let name = host.name.clone();
            let edit_name = host.name.clone();
            section = section.child(
                h_flex()
                    .id(SharedString::from(format!("ssh-host-{i}")))
                    .group("ssh-host")
                    .px_3()
                    .py(px(3.))
                    .gap_1p5()
                    .cursor_pointer()
                    .hover(|style| style.bg(colors.ghost_element_hover))
                    .tooltip(Tooltip::text(format!(
                        "Open ssh {} in a new tab",
                        host.name
                    )))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        window.focus(&view.focus_handle, cx);
                        view.app.open_ssh(&name);
                        view.after_change(cx);
                    }))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().truncate().child(host.name.clone()))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.))
                                    .text_color(colors.text_muted)
                                    .child(if host.is_saved() {
                                        host.address()
                                    } else {
                                        format!("{} · ssh config", host.address())
                                    }),
                            ),
                    )
                    .child(
                        IconButton::new(
                            SharedString::from(format!("edit-host-{i}")),
                            IconName::Pencil,
                        )
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text(if host.is_saved() {
                            "Edit host and port forwards"
                        } else {
                            "Port forwards (host is read-only)"
                        }))
                        .on_click(cx.listener(
                            move |view, _, window, cx| {
                                cx.stop_propagation();
                                view.open_host_form(Some(edit_name.clone()), window, cx);
                            },
                        )),
                    ),
            );
            for (j, spec) in self
                .app
                .ssh
                .store
                .forwards_of(&host.name)
                .iter()
                .enumerate()
            {
                let info = self.app.ssh.forward(&host.name, spec);
                section = section.child(self.forward_row(
                    format!("fwd-{i}-{j}"),
                    &host.name,
                    spec,
                    info,
                    cx,
                ));
            }
        }
        section.child(
            h_flex()
                .id("whats-on-port")
                .mt_1()
                .px_3()
                .py(px(3.))
                .gap_1p5()
                .cursor_pointer()
                .text_color(colors.text_muted)
                .hover(|style| style.bg(colors.ghost_element_hover).text_color(colors.text))
                .on_click(cx.listener(|view, _, window, cx| view.open_ports(window, cx)))
                .child(
                    ferrum_ui::components::Icon::new(IconName::MagnifyingGlass)
                        .size(IconSize::XSmall),
                )
                .child("What's using a port?"),
        )
    }

    fn forward_row(
        &self,
        id: String,
        host: &str,
        spec: &ForwardSpec,
        info: Option<&ForwardInfo>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let status = cx.theme().status().clone();
        let (dot, state, tip): (Hsla, &str, &str) = match info.map(|f| f.state) {
            Some(ForwardState::Running) => (status.success, "on", "Running: click to stop"),
            Some(ForwardState::Failed) => (status.error, "failed", "Failed: click to retry"),
            None => (colors.text_muted, "off", "Stopped: click to start"),
        };
        let message = info.and_then(|f| f.message.clone());
        let failed_key = info
            .filter(|f| f.state == ForwardState::Failed)
            .map(|f| f.key.clone());
        let label = spec.describe();
        let (host, spec) = (host.to_owned(), spec.clone());
        v_flex()
            .id(SharedString::from(id.clone()))
            .pl(px(22.))
            .pr_1()
            .py(px(2.))
            .cursor_pointer()
            .hover(|style| style.bg(colors.ghost_element_hover))
            .tooltip(Tooltip::text(format!("{spec} · {tip}")))
            .on_click(cx.listener(move |view, _, _, cx| {
                view.app.toggle_forward(&host, &spec);
                view.after_change(cx);
            }))
            .child(
                h_flex()
                    .gap_1p5()
                    .min_w_0()
                    .text_size(px(12.))
                    .child(div().flex_none().size(px(7.)).rounded_full().bg(dot))
                    .child(div().flex_1().min_w_0().truncate().child(label))
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(11.))
                            .text_color(dot)
                            .child(state),
                    )
                    .when_some(failed_key, |row, key| {
                        row.child(
                            IconButton::new(
                                SharedString::from(format!("{id}-clear")),
                                IconName::Close,
                            )
                            .icon_size(IconSize::XSmall)
                            .tooltip(Tooltip::text("Dismiss error"))
                            .on_click(cx.listener(
                                move |view, _, _, cx| {
                                    cx.stop_propagation();
                                    view.app.clear_forward(key.clone());
                                },
                            )),
                        )
                    }),
            )
            .when_some(message, |row, m| {
                row.child(
                    div()
                        .pl(px(14.))
                        .min_w_0()
                        .truncate()
                        .text_size(px(11.))
                        .text_color(status.error)
                        .child(m),
                )
            })
    }
}
