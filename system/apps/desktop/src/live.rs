//! The Runtime tab: Kubernetes pods and Docker containers as they are now,
//! next to the components they belong to (by the `app` label or the
//! container name; see `ferrumsystem_core::runtime`).

use std::collections::HashSet;

use ferrum_ui::prelude::*;
use ferrumsystem_core::DeployTarget;
use ferrumsystem_core::runtime::{self, Pod};

use crate::app::FerrumSystem;
use crate::icons::rgb;
use crate::panel::{heading, para};

fn status_color(status: &str) -> gpui::Hsla {
    rgb(match status.to_ascii_lowercase().as_str() {
        "running" | "succeeded" => 0x22c55e,
        "pending" | "containercreating" | "created" | "restarting" => 0xeab308,
        "exited" | "paused" => 0x8b8b90,
        _ => 0xef4444,
    })
}

fn dot(color: gpui::Hsla) -> impl IntoElement {
    div().flex_none().size(px(8.)).rounded_full().bg(color)
}

impl FerrumSystem {
    pub(crate) fn runtime_tab(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let namespace = self
            .system
            .cluster
            .as_ref()
            .map(|c| {
                if c.namespace.is_empty() {
                    "default".to_owned()
                } else {
                    c.namespace.clone()
                }
            })
            .unwrap_or_else(|| "default".into());
        let mut col = v_flex().pt_3().gap_1().child(
            h_flex()
                .gap_2()
                .child(para(
                    format!("Pods in namespace {namespace}, and Docker containers on this Mac."),
                    cx,
                ))
                .child(div().flex_1())
                .child(
                    Button::new(
                        "refresh-live",
                        if self.live.loading {
                            "Reading…"
                        } else {
                            "Refresh"
                        },
                    )
                    .size(ButtonSize::Compact)
                    .style(ButtonStyle::Outlined)
                    .disabled(self.live.loading)
                    .on_click(cx.listener(|this, _, _, cx| this.refresh_live(cx))),
                ),
        );

        col = col.child(heading("Kubernetes", cx));
        match &self.live.pods {
            None => col = col.child(para("Not read yet.", cx)),
            Some(Err(e)) => col = col.child(para(format!("kubectl: {e}"), cx)),
            Some(Ok(pods)) => {
                let mut matched: HashSet<&str> = HashSet::new();
                for node in self
                    .system
                    .nodes
                    .iter()
                    .filter(|n| n.deploy.as_ref().map(|d| d.target) == Some(DeployTarget::Pod))
                {
                    let mine = runtime::pods_of(node, pods);
                    matched.extend(mine.iter().map(|p| p.name.as_str()));
                    let want = node.deploy.as_ref().and_then(|d| d.replicas).unwrap_or(1);
                    let ready = mine
                        .iter()
                        .filter(|p| p.ready.0 == p.ready.1 && p.ready.1 > 0)
                        .count() as u32;
                    let health = if mine.is_empty() {
                        rgb(0x8b8b90)
                    } else if ready >= want {
                        rgb(0x22c55e)
                    } else {
                        rgb(0xeab308)
                    };
                    col = col.child(
                        v_flex()
                            .py_1()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(dot(health))
                                    .child(div().flex_1().child(node.name.clone()))
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(colors.text_muted)
                                            .child(format!(
                                                "{ready}/{want} ready · app={}",
                                                runtime::selector(node)
                                            )),
                                    ),
                            )
                            .children(mine.into_iter().map(|p| pod_row(p, cx))),
                    );
                }
                let other: Vec<&Pod> = pods
                    .iter()
                    .filter(|p| !matched.contains(p.name.as_str()))
                    .collect();
                if !other.is_empty() {
                    col = col
                        .child(heading("Other pods", cx))
                        .children(other.into_iter().map(|p| pod_row(p, cx)));
                }
                if pods.is_empty() {
                    col = col.child(para("No pods in this namespace.", cx));
                }
            }
        }

        col = col.child(heading("Docker", cx));
        match &self.live.containers {
            None => col = col.child(para("Not read yet.", cx)),
            Some(Err(e)) => col = col.child(para(format!("docker: {e}"), cx)),
            Some(Ok(containers)) => {
                let named: Vec<_> = self
                    .system
                    .nodes
                    .iter()
                    .filter(|n| {
                        n.deploy.as_ref().map(|d| d.target) == Some(DeployTarget::Container)
                    })
                    .collect();
                for node in named {
                    let mine = runtime::containers_of(node, containers);
                    let up = mine.iter().any(|c| c.state == "running");
                    col = col.child(
                        h_flex()
                            .py_1()
                            .gap_2()
                            .child(dot(if up { rgb(0x22c55e) } else { rgb(0x8b8b90) }))
                            .child(div().flex_1().child(node.name.clone()))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(colors.text_muted)
                                    .child(match mine.first() {
                                        Some(c) => format!("{} · {}", c.name, c.status),
                                        None => format!(
                                            "no container named {}",
                                            runtime::selector(node)
                                        ),
                                    }),
                            ),
                    );
                }
                if containers.is_empty() {
                    col = col.child(para("No containers.", cx));
                } else {
                    col = col.child(heading("All containers", cx)).children(
                        containers.iter().take(40).map(|c| {
                            h_flex()
                                .gap_2()
                                .py_0p5()
                                .child(dot(status_color(&c.state)))
                                .child(
                                    div()
                                        .flex_1()
                                        .truncate()
                                        .text_size(px(12.))
                                        .child(c.name.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(colors.text_muted)
                                        .truncate()
                                        .child(format!("{} · {}", c.image, c.status)),
                                )
                        }),
                    );
                }
            }
        }
        col.into_any_element()
    }
}

fn pod_row(p: &Pod, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    h_flex()
        .pl_4()
        .gap_2()
        .child(dot(status_color(&p.status)))
        .child(
            div()
                .flex_1()
                .truncate()
                .text_size(px(12.))
                .child(p.name.clone()),
        )
        .child(
            div()
                .text_size(px(11.))
                .text_color(colors.text_muted)
                .child(format!(
                    "{} · {}/{} · {} restart{}",
                    p.status,
                    p.ready.0,
                    p.ready.1,
                    p.restarts,
                    if p.restarts == 1 { "" } else { "s" }
                )),
        )
}
