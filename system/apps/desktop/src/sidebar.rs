//! The sidebar: window controls, the project, its architecture (or the
//! button that creates one), the demo systems and the layers.

use ferrum_ui::prelude::*;

use crate::app::{FerrumSystem, Source};
use crate::icons::rgb;

impl FerrumSystem {
    pub(crate) fn title_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .flex_none()
            .gap_0p5()
            .child(
                ferrum_ui::sidebar_toggle("toggle-sidebar", self.sidebar_open).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.sidebar_open = !this.sidebar_open;
                        cx.notify();
                    }),
                ),
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
                    Source::Demo(_) => "Architecture".into(),
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

        col = col.child(div().h_2()).child(section_toggle(
            "demos-toggle",
            "Demos",
            self.demos_open,
            cx,
            cx.listener(|this, _, _, cx| {
                this.demos_open = !this.demos_open;
                cx.notify();
            }),
        ));
        for (i, demo) in self.demos.iter().enumerate().filter(|_| self.demos_open) {
            let active = self.source == Source::Demo(i);
            col = col.child(
                ferrum_ui::sidebar_item(("demo", i), active, cx)
                    .py_1()
                    .items_start()
                    .child(
                        v_flex()
                            .min_w_0()
                            .child(div().truncate().child(demo.name.clone()))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(colors.text_muted)
                                    .line_clamp(2)
                                    .child(demo.tagline.clone()),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.show_demo(i, cx))),
            );
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

/// A sidebar group label that folds its group.
fn section_toggle(
    id: &'static str,
    label: &'static str,
    open: bool,
    cx: &App,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    h_flex()
        .id(id)
        .cursor_pointer()
        .child(ferrum_ui::sidebar_label(label, cx))
        .child(
            Icon::new(if open {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            })
            .size(IconSize::XSmall)
            .color(Color::Muted),
        )
        .on_click(on_click)
}
