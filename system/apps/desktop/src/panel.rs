//! The right panel: what explains the design, one tab at a time.

use ferrum_ui::prelude::*;
use ferrumsystem_core::DeployTarget;

use crate::app::{Dialog, FerrumSystem, Tab};

pub(crate) fn heading(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .pt_3()
        .pb_1()
        .text_size(px(12.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(cx.theme().colors().text_muted)
        .child(text.into())
}

pub(crate) fn para(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .text_size(px(13.))
        .line_height(px(20.))
        .text_color(cx.theme().colors().text)
        .child(text.into())
}

fn bullets(items: &[String], icon: IconName, cx: &App) -> impl IntoElement {
    v_flex().gap_1p5().children(items.iter().map(|item| {
        h_flex()
            .items_start()
            .gap_2()
            .child(
                div()
                    .pt(px(3.))
                    .child(Icon::new(icon).size(IconSize::XSmall).color(Color::Accent)),
            )
            .child(para(item.clone(), cx))
    }))
}

pub(crate) fn chip(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    div()
        .px_2()
        .py_0p5()
        .rounded(px(5.))
        .border_1()
        .border_color(colors.border)
        .text_size(px(12.))
        .child(text.into())
}

fn mono(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .font_family(
            ferrum_ui::theme::theme_settings(cx)
                .buffer_font
                .family
                .clone(),
        )
        .text_size(px(12.))
        .child(text.into())
}

impl FerrumSystem {
    pub(crate) fn node_name(&self, id: &str) -> String {
        self.system
            .node(id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| id.to_owned())
    }

    pub(crate) fn render_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let tabs = h_flex()
            .id("panel-tabs")
            .flex_none()
            .px_2()
            .gap_0p5()
            .h(px(40.))
            .overflow_x_scroll()
            .border_b_1()
            .border_color(colors.border_variant)
            .children(
                Tab::ALL
                    .into_iter()
                    .filter(|t| *t != Tab::Component || self.selected.is_some())
                    .map(|tab| {
                        ferrum_ui::header_tab(SharedString::from(tab.label()), self.tab == tab, cx)
                            .child(tab.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.tab = tab;
                                if tab == Tab::Runtime && this.live.pods.is_none() {
                                    this.refresh_live(cx);
                                }
                                cx.notify();
                            }))
                    }),
            );
        let body: AnyElement = match self.tab {
            Tab::Add => self.add_tab(cx),
            Tab::Overview => self.overview_tab(cx).into_any_element(),
            Tab::Component => self.component_tab(window, cx),
            Tab::Apis => self.apis_tab(cx).into_any_element(),
            Tab::Sockets => self.sockets_tab(cx).into_any_element(),
            Tab::Data => self.data_tab(cx).into_any_element(),
            Tab::Flows => self.flows_tab(cx).into_any_element(),
            Tab::Reliability => self.reliability_tab(cx).into_any_element(),
            Tab::Deploy => self.deploy_tab(cx).into_any_element(),
            Tab::Runtime => self.runtime_tab(cx),
        };
        v_flex().size_full().child(tabs).child(
            div()
                .id("panel-body")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px_4()
                .pb_4()
                .child(body),
        )
    }

    fn overview_tab(&self, cx: &App) -> impl IntoElement {
        let s = &self.system;
        v_flex()
            .pt_3()
            .child(para(s.overview.clone(), cx))
            .when(!s.concepts.is_empty(), |c| {
                c.child(heading("Concepts", cx)).child(
                    h_flex()
                        .flex_wrap()
                        .gap_1p5()
                        .children(s.concepts.iter().map(|x| chip(x.clone(), cx))),
                )
            })
            .when(!s.functional.is_empty(), |c| {
                c.child(heading("Functional requirements", cx))
                    .child(bullets(&s.functional, IconName::CircleCheck, cx))
            })
            .when(!s.non_functional.is_empty(), |c| {
                c.child(heading("Non-functional requirements", cx))
                    .child(bullets(&s.non_functional, IconName::Info, cx))
            })
    }

    fn apis_tab(&self, cx: &App) -> impl IntoElement {
        let colors = cx.theme().colors();
        v_flex()
            .pt_3()
            .gap_2()
            .when(self.system.apis.is_empty(), |c| {
                c.child(para("No APIs described yet.", cx))
            })
            .children(self.system.apis.iter().map(|a| {
                v_flex()
                    .gap_0p5()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(colors.border_variant)
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .px_1p5()
                                    .rounded(px(4.))
                                    .bg(method_color(&a.method).opacity(0.18))
                                    .text_color(method_color(&a.method))
                                    .text_size(px(11.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(a.method.clone()),
                            )
                            .child(mono(a.path.clone(), cx)),
                    )
                    .child(para(a.summary.clone(), cx))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(colors.text_muted)
                            .child(format!("Served by {}", self.node_name(&a.service))),
                    )
            }))
    }

    fn sockets_tab(&self, cx: &App) -> impl IntoElement {
        let colors = cx.theme().colors();
        v_flex()
            .pt_3()
            .gap_2()
            .when(self.system.sockets.is_empty(), |c| {
                c.child(para("No sockets or streams described yet.", cx))
            })
            .children(self.system.sockets.iter().map(|s| {
                v_flex()
                    .gap_0p5()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(colors.border_variant)
                    .child(mono(s.channel.clone(), cx))
                    .child(para(s.summary.clone(), cx))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(colors.text_muted)
                            .child(format!("{} · {}", self.node_name(&s.service), s.direction)),
                    )
            }))
    }

    fn data_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let hover = ferrum_ui::hover_fill(cx);
        v_flex()
            .pt_3()
            .gap_2()
            .child(para(
                "Double-click a database on the diagram, or click one here, to see its tables.",
                cx,
            ))
            .children(self.system.databases.iter().map(|db| {
                let id = db.id.clone();
                let host = self
                    .system
                    .nodes
                    .iter()
                    .find(|n| n.database.as_deref() == Some(&db.id))
                    .and_then(|n| n.tech.clone())
                    .and_then(|t| crate::icons::tech_info(&self.system, &t))
                    .map(|(name, _, _)| name)
                    .unwrap_or_default();
                v_flex()
                    .id(SharedString::from(format!("db-{}", db.id)))
                    .p_2()
                    .gap_1()
                    .rounded_md()
                    .border_1()
                    .border_color(colors.border_variant)
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.dialog = Some(Dialog::Schema(id.clone()));
                        cx.notify();
                    }))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(Icon::new(IconName::DatabaseZap).size(IconSize::Small))
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(db.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(colors.text_muted)
                                    .child(host),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(colors.text_muted)
                            .child(
                                db.tables
                                    .iter()
                                    .map(|t| t.name.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            ),
                    )
            }))
    }

    fn flows_tab(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let mut col = v_flex().pt_3().gap_3();
        if self.system.flows.is_empty() {
            return col.child(para("No flows described yet.", cx));
        }
        for (fi, flow) in self.system.flows.iter().enumerate() {
            let walking = self.walk.filter(|(f, _)| *f == fi).map(|(_, s)| s);
            let last = flow.steps.len().saturating_sub(1);
            let mut card =
                v_flex()
                    .p_2()
                    .gap_1()
                    .rounded_md()
                    .border_1()
                    .border_color(if walking.is_some() {
                        colors.text_accent
                    } else {
                        colors.border_variant
                    })
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(flow.name.clone()),
                            )
                            .child(match walking {
                                None => Button::new(("play", fi), "Walk through")
                                    .size(ButtonSize::Compact)
                                    .style(ButtonStyle::Outlined)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.walk_to(Some((fi, 0)), cx)
                                    }))
                                    .into_any_element(),
                                Some(s) => h_flex()
                                    .gap_0p5()
                                    .child(
                                        IconButton::new(("prev", fi), IconName::ChevronLeft)
                                            .disabled(s == 0)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.walk_to(Some((fi, s.saturating_sub(1))), cx)
                                            })),
                                    )
                                    .child(div().text_size(px(11.)).child(format!(
                                        "{}/{}",
                                        s + 1,
                                        last + 1
                                    )))
                                    .child(
                                        IconButton::new(("next", fi), IconName::ChevronRight)
                                            .disabled(s >= last)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.walk_to(Some((fi, s + 1)), cx)
                                            })),
                                    )
                                    .child(IconButton::new(("stop", fi), IconName::Close).on_click(
                                        cx.listener(|this, _, _, cx| this.walk_to(None, cx)),
                                    ))
                                    .into_any_element(),
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(colors.text_muted)
                            .child(flow.summary.clone()),
                    );
            for (si, step) in flow.steps.iter().enumerate() {
                let current = walking == Some(si);
                card = card.child(
                    h_flex()
                        .id(SharedString::from(format!("step-{fi}-{si}")))
                        .items_start()
                        .gap_2()
                        .p_1()
                        .rounded(px(5.))
                        .cursor_pointer()
                        .when(current, |r| r.bg(colors.text_accent.opacity(0.12)))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.walk_to(Some((fi, si)), cx)),
                        )
                        .child(
                            div()
                                .flex_none()
                                .size(px(18.))
                                .rounded_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(10.))
                                .bg(if current {
                                    colors.text_accent
                                } else {
                                    colors.text.opacity(0.1)
                                })
                                .text_color(if current {
                                    colors.background
                                } else {
                                    colors.text_muted
                                })
                                .child((si + 1).to_string()),
                        )
                        .child(
                            v_flex()
                                .min_w_0()
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(colors.text_muted)
                                        .child(format!(
                                            "{} → {}",
                                            self.node_name(&step.from),
                                            self.node_name(&step.to)
                                        )),
                                )
                                .child(para(step.text.clone(), cx)),
                        ),
                );
            }
            col = col.child(card);
        }
        col
    }

    fn reliability_tab(&self, cx: &App) -> impl IntoElement {
        v_flex()
            .pt_3()
            .gap_3()
            .when(self.system.reliability.is_empty(), |c| {
                c.child(para("Nothing yet.", cx))
            })
            .children(self.system.reliability.iter().map(|n| {
                v_flex()
                    .gap_0p5()
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(n.title.clone()),
                    )
                    .child(para(n.text.clone(), cx))
            }))
    }

    fn deploy_tab(&self, cx: &App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let targets = [
            DeployTarget::Pod,
            DeployTarget::Container,
            DeployTarget::Vm,
            DeployTarget::Serverless,
            DeployTarget::Managed,
            DeployTarget::Device,
        ];
        let mut col = v_flex().pt_1();
        if let Some(cluster) = &self.system.cluster {
            col = col.child(heading("Kubernetes", cx)).child(para(
                format!(
                    "Namespace {} on {}",
                    if cluster.namespace.is_empty() {
                        "default"
                    } else {
                        &cluster.namespace
                    },
                    cluster
                        .context
                        .as_deref()
                        .unwrap_or("the current kubectl context")
                ),
                cx,
            ));
        }
        for target in targets {
            let nodes: Vec<_> = self
                .system
                .nodes
                .iter()
                .filter(|n| n.deploy.as_ref().map(|d| d.target) == Some(target))
                .collect();
            if nodes.is_empty() {
                continue;
            }
            col = col.child(heading(target.label(), cx));
            for n in nodes {
                let d = n.deploy.as_ref().expect("filtered");
                let mut detail = Vec::new();
                if let Some(r) = d.replicas {
                    detail.push(format!("{r} replica{}", if r == 1 { "" } else { "s" }));
                }
                if let Some(img) = &d.image {
                    detail.push(img.clone());
                }
                if let Some(p) = d.port {
                    detail.push(format!(":{p}"));
                }
                col = col.child(
                    h_flex()
                        .py_1()
                        .gap_2()
                        .child(div().flex_1().truncate().child(n.name.clone()))
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(colors.text_muted)
                                .child(detail.join(" · ")),
                        ),
                );
            }
        }
        col
    }
}

fn method_color(method: &str) -> gpui::Hsla {
    crate::icons::rgb(match method {
        "GET" => 0x22c55e,
        "POST" => 0x38bdf8,
        "PUT" | "PATCH" => 0xeab308,
        "DELETE" => 0xef4444,
        _ => 0xa855f7,
    })
}
