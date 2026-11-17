//! The sidebar: window controls, the project, its architecture (or the
//! button that creates one) and the layers of the project's diagram. The
//! demos are not listed here: File › Open Demo and Help › Open Demo show
//! them.

use ferrum_ui::prelude::*;

use crate::app::{FerrumSystem, Source};
use crate::icons::rgb;

impl FerrumSystem {
    pub(crate) fn title_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .flex_none()
            .gap_0p5()
            .child(
                ferrum_ui::sidebar_toggle("toggle-sidebar", self.sidebar_open())
                    .tooltip(Tooltip::text(if self.sidebar_open() {
                        "Hide sidebar (⌘B)"
                    } else {
                        "Show sidebar (⌘B)"
                    }))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx))),
            )
            .child(ferrum_ui::settings_button("open-settings"))
            .child(
                ferrum_ui::command_button("switch-project", "Switch project (⌥⌘O)").on_click(
                    cx.listener(|this, _, window, cx| {
                        this.project_bar
                            .update(cx, |bar, cx| bar.toggle_switcher(window, cx));
                    }),
                ),
            )
    }

    pub(crate) fn render_sidebar(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let mut col = v_flex()
            .flex_none()
            .w(px(250.))
            .h_full()
            .child(
                ferrum_ui::title_row("sidebar-title", true, window)
                    .child(div().flex_1())
                    .child(self.title_controls(cx)),
            )
            .child(div().px_1().child(self.project_bar.clone()))
            .child(ferrum_ui::sidebar_label("This project", cx));

        col = match (&self.project, self.project_has_system) {
            (None, _) => col.child(hint(
                "Choose a project folder: its architecture is kept in .architecture/ inside it.",
                cx,
            )),
            (Some(_), true) => {
                let active = matches!(self.source, Source::Project(_));
                let name = match &self.source {
                    Source::Project(_) => self.system.name.clone(),
                    Source::Demo(_) | Source::Empty => "Architecture".into(),
                };
                col.child(
                    ferrum_ui::sidebar_item("project-system", active, cx)
                        .child(Icon::new(IconName::Blocks).size(IconSize::Small))
                        .child(div().truncate().child(name))
                        .on_click(cx.listener(|this, _, _, cx| this.show_project_system(cx))),
                )
            }
            (Some(_), false) => col.child(hint("No .architecture folder yet.", cx)).child(
                v_flex()
                    .px_3()
                    .pb_1()
                    .gap_1()
                    .child(
                        Button::new("create-architecture", "Create architecture")
                            .style(ButtonStyle::Filled)
                            .size(ButtonSize::Compact)
                            .full_width()
                            .on_click(
                                cx.listener(|this, _, _, cx| this.create_architecture(None, cx)),
                            ),
                    )
                    .child(
                        Button::new("generate-architecture", "Generate from project files")
                            .style(ButtonStyle::Outlined)
                            .size(ButtonSize::Compact)
                            .full_width()
                            .tooltip(Tooltip::text(
                                "From docker-compose files, Kubernetes manifests or package manifests",
                            ))
                            .on_click(cx.listener(|this, _, _, cx| this.generate_from_project(cx))),
                    )
                    .child(
                        Button::new("draft-architecture", "Ask agent to draft the architecture")
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .full_width()
                            .tooltip(Tooltip::text(
                                "The agent proposes a diagram from the project's files; you approve it",
                            ))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.ask_agent_to_draft(window, cx)
                            })),
                    )
                    .when(self.is_demo(), |c| {
                        c.child(
                            Button::new("use-demo", "Start from this demo")
                                .style(ButtonStyle::Outlined)
                                .size(ButtonSize::Compact)
                                .full_width()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let demo = this.system.clone();
                                    this.create_architecture(Some(demo), cx)
                                })),
                        )
                    }),
            ),
        };

        // The layers of the user's own diagram only (not a demo's).
        if !matches!(self.source, Source::Project(_)) {
            return col;
        }
        col = col.child(div().h_2()).child(
            h_flex()
                .pr_2()
                .child(div().flex_1().child(ferrum_ui::sidebar_label("Layers", cx)))
                .child(
                    IconButton::new("add-layer", IconName::Plus)
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text("Add a layer"))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.open_layer_dialog(window, cx)),
                        ),
                ),
        );
        for layer in &self.system.layers {
            let count = self
                .system
                .nodes
                .iter()
                .filter(|n| n.layer == layer.id)
                .count();
            col = col.child(
                h_flex()
                    .mx_1p5()
                    .px_2()
                    .h(px(26.))
                    .gap_2()
                    .text_color(colors.text_muted)
                    .child(div().size(px(8.)).rounded(px(2.)).bg(rgb(layer.color)))
                    .child(div().flex_1().truncate().child(layer.name.clone()))
                    .child(div().text_size(px(11.)).child(count.to_string()))
                    .when(count == 0, |row| {
                        let id = layer.id.clone();
                        row.child(
                            IconButton::new(
                                SharedString::from(format!("remove-layer-{}", layer.id)),
                                IconName::Close,
                            )
                            .icon_size(IconSize::XSmall)
                            .tooltip(Tooltip::text("Remove this empty layer"))
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.edit(cx, |s| {
                                        s.remove_layer(&id);
                                    })
                                },
                            )),
                        )
                    }),
            );
        }
        col
    }
}

fn hint(text: &'static str, cx: &App) -> impl IntoElement {
    div()
        .px_3()
        .py_1()
        .text_size(px(12.))
        .text_color(cx.theme().colors().text_muted)
        .child(text)
}
