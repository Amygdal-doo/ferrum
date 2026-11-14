use crate::app::FerrumData;
use crate::components::env_dot;
use crate::state::{ConnStatus, Dialog, SchemaLoad};
use ferrum_ui::prelude::*;
use ferrumdata_domain::ConnectionProfile;

impl FerrumData {
    pub fn render_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let needle = self.filter.read(cx).text().to_lowercase();
        let schemas = &self.state.schemas;
        let key_browsers = &self.state.key_browsers;
        let table_match = |p: &ConnectionProfile| match schemas.get(&p.id) {
            Some(SchemaLoad::Loaded(t)) => t
                .iter()
                .any(|t| t.table.name.to_lowercase().contains(&needle)),
            _ => key_browsers.get(&p.id).is_some_and(|b| {
                b.keys
                    .iter()
                    .any(|k| k.name.to_lowercase().contains(&needle))
            }),
        };
        let profiles: Vec<ConnectionProfile> = self
            .state
            .profiles
            .iter()
            .filter(|p| {
                needle.is_empty()
                    || p.display_name.to_lowercase().contains(&needle)
                    || table_match(p)
            })
            .cloned()
            .collect();
        let empty = self.state.profiles.is_empty();
        let mut rows: Vec<AnyElement> = Vec::with_capacity(profiles.len());
        for profile in profiles {
            rows.push(self.render_profile_row(profile, &needle, window, cx));
        }

        v_flex()
            .size_full()
            .child(
                ferrum_ui::title_row("sidebar-title", true, window)
                    .child(div().flex_1())
                    .child(self.render_title_controls(cx)),
            )
            .child(div().px_1().child(self.project_bar.clone()))
            .child(ferrum_ui::sidebar_label("Connections", cx))
            .when(!empty, |this| {
                this.child(div().px_3().pb_2().child(self.filter.clone()))
            })
            .child(
                v_flex()
                    .id("profiles")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_2()
                    .pb_2()
                    .when(empty, |this| {
                        this.child(
                            v_flex()
                                .items_center()
                                .gap_3()
                                .px_2()
                                .py_6()
                                .child(
                                    Label::new("No connections yet.")
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                )
                                .child(
                                    Button::new("add-first", "Add PostgreSQL connection")
                                        .style(ButtonStyle::Outlined)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.open_dialog(
                                                Dialog::ConnectionForm(None),
                                                window,
                                                cx,
                                            )
                                        })),
                                ),
                        )
                    })
                    .children(rows),
            )
            .into_any_element()
    }

    fn render_profile_row(
        &mut self,
        profile: ConnectionProfile,
        filter: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let status_colors = cx.theme().status().clone();
        let id = profile.id;
        let status = self.state.conn_status.get(&id).cloned();
        let active = self.state.active_tab().and_then(|t| t.profile_id) == Some(id);
        let (status_text, status_color) = match &status {
            None => ("", colors.text_muted),
            Some(ConnStatus::Connecting) => ("connecting…", colors.text_muted),
            Some(ConnStatus::Ready(_)) => ("connected", status_colors.success),
            Some(ConnStatus::Failed(_)) => ("failed", status_colors.error),
        };
        let error = match &status {
            Some(ConnStatus::Failed(m)) => Some(m.clone()),
            _ => None,
        };
        let connected = matches!(status, Some(ConnStatus::Ready(_)));
        let expanded = self.state.expanded.contains(&id);
        let group: SharedString = format!("profile-{id}").into();
        let target = profile.target_label();
        let key_value = profile.engine.is_key_value();
        let tree = match (expanded, key_value) {
            (false, _) => None,
            (true, true) => Some(self.render_key_tree(id, filter, window, cx)),
            (true, false) => Some(self.render_table_tree(id, filter, window, cx)),
        };
        let (edit, delete) = (profile.clone(), profile.clone());

        let action = |label: &'static str, key: &'static str| {
            Button::new(SharedString::from(format!("{key}-{id}")), label)
                .label_size(LabelSize::XSmall)
        };

        v_flex()
            .group(group.clone())
            .rounded_md()
            .px_2()
            .py_1p5()
            .rounded(px(8.))
            .when(active, |this| this.bg(ferrum_ui::active_fill(cx)))
            .when(!active, |this| {
                let hover = ferrum_ui::hover_fill(cx);
                this.hover(move |s| s.bg(hover))
            })
            .child(
                h_flex()
                    .id(SharedString::from(format!("profile-row-{id}")))
                    .gap_2()
                    .tooltip(Tooltip::text(target.clone()))
                    .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                        if event.click_count() >= 2 {
                            this.connect(id, cx);
                        } else {
                            this.state.bind_active_tab(Some(id));
                            cx.notify();
                        }
                    }))
                    .child(
                        IconButton::new(
                            SharedString::from(format!("expand-{id}")),
                            if expanded {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
                            },
                        )
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text(match (expanded, key_value) {
                            (true, true) => "Collapse keys",
                            (false, true) => "Expand keys",
                            (true, false) => "Collapse tables",
                            (false, false) => "Expand tables",
                        }))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.toggle_tables(id, cx);
                        })),
                    )
                    .child(env_dot(profile.env, 8., cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .child(profile.display_name.clone()),
                    )
                    .child(div().text_xs().text_color(status_color).child(status_text)),
            )
            .child(
                div()
                    .pl(px(36.))
                    .truncate()
                    .text_xs()
                    .text_color(colors.text_muted)
                    .child(target),
            )
            .when_some(error, |this, err| {
                this.child(
                    div()
                        .mt_1()
                        .pl_4()
                        .text_xs()
                        .text_color(status_colors.error)
                        .child(err),
                )
            })
            .child(
                h_flex()
                    .mt_1()
                    .gap_1()
                    .pl_3()
                    .visible_on_hover(group)
                    .child(if connected {
                        action("Disconnect", "disconnect")
                            .on_click(cx.listener(move |this, _, _, cx| this.disconnect(id, cx)))
                    } else {
                        action("Connect", "connect")
                            .on_click(cx.listener(move |this, _, _, cx| this.connect(id, cx)))
                    })
                    .child(action("Edit", "edit").on_click(cx.listener(
                        move |this, _, window, cx| {
                            this.open_dialog(Dialog::ConnectionForm(Some(edit.clone())), window, cx)
                        },
                    )))
                    .child(
                        action("Delete", "delete")
                            .color(Color::Error)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_dialog(Dialog::DeleteProfile(delete.clone()), window, cx)
                            })),
                    ),
            )
            .children(tree)
            .into_any_element()
    }
}
