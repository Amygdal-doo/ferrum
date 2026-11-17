//! Kubernetes section: context and namespace pickers, Pods / Deployments /
//! Services from `kubectl`, pod logs on click, Restart on a deployment.

use crate::app::FerrumContainers;
use crate::state::{KubePicker, KubeTab, matches_any};
use crate::views::chrome::card_header;
use crate::views::widgets::{button, cell, grow_cell, header_row, muted, row, segmented};
use ferrum_ui::components::{Color, Label, LabelCommon, LabelSize, h_flex, v_flex};
use gpui::{AnyElement, Context, SharedString, Window, prelude::*};

fn columns(tab: KubeTab) -> &'static [(&'static str, Option<f32>)] {
    match tab {
        KubeTab::Pods => &[
            ("NAME", None),
            ("READY", Some(70.)),
            ("STATUS", Some(150.)),
            ("RESTARTS", Some(80.)),
            ("NODE", Some(150.)),
        ],
        KubeTab::Deployments => &[
            ("NAME", None),
            ("READY", Some(70.)),
            ("UP-TO-DATE", Some(90.)),
            ("AVAILABLE", Some(80.)),
            ("IMAGES", Some(200.)),
            ("", Some(80.)),
        ],
        KubeTab::Services => &[
            ("NAME", None),
            ("TYPE", Some(110.)),
            ("CLUSTER-IP", Some(120.)),
            ("EXTERNAL", Some(150.)),
            ("PORTS", Some(170.)),
        ],
    }
}

fn status_color(status: &str) -> Color {
    match status {
        "Running" | "Succeeded" | "Completed" => Color::Success,
        "Pending" | "ContainerCreating" | "PodInitializing" => Color::Warning,
        _ => Color::Error,
    }
}

/// An empty state for a list that is loading or failed, or `None` when loaded.
fn pending<T>(list: Option<&Result<Vec<T>, String>>) -> Option<AnyElement> {
    match list {
        None => Some(ferrum_ui::empty_state("Asking kubectl…", "").into_any_element()),
        Some(Err(error)) => {
            Some(ferrum_ui::empty_state("kubectl failed", error.clone()).into_any_element())
        }
        Some(Ok(_)) => None,
    }
}

fn rows(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> AnyElement {
    let kube = &app.state.kube;
    let filter = app.state.filter.as_str();
    let mut out: Vec<AnyElement> = Vec::new();
    match kube.tab {
        KubeTab::Pods => {
            if let Some(empty) = pending(kube.pods.as_ref()) {
                return empty;
            }
            for (i, pod) in kube.pods.iter().flatten().flatten().enumerate() {
                if !matches_any(filter, &[&pod.name, &pod.status]) {
                    continue;
                }
                let name = pod.name.clone();
                out.push(
                    row(SharedString::from(format!("pod-{i}")), false, cx)
                        .cursor_pointer()
                        .child(grow_cell(
                            Label::new(pod.name.clone())
                                .size(LabelSize::Small)
                                .truncate(),
                        ))
                        .child(cell(
                            70.,
                            format!("{}/{}", pod.ready.0, pod.ready.1),
                            Color::Default,
                        ))
                        .child(cell(150., pod.status.clone(), status_color(&pod.status)))
                        .child(cell(80., pod.restarts.to_string(), Color::Muted))
                        .child(cell(
                            150.,
                            pod.node.clone().unwrap_or_default(),
                            Color::Muted,
                        ))
                        .on_click(
                            cx.listener(move |app, _, _, cx| app.open_pod_logs(name.clone(), cx)),
                        )
                        .into_any_element(),
                );
            }
        }
        KubeTab::Deployments => {
            if let Some(empty) = pending(kube.deployments.as_ref()) {
                return empty;
            }
            for (i, d) in kube.deployments.iter().flatten().flatten().enumerate() {
                if !matches_any(filter, &[&d.name, &d.images.join(" ")]) {
                    continue;
                }
                let name = d.name.clone();
                let busy = app.state.busy.contains(&format!("deployment:{}", d.name));
                let ready_color = if d.ready.0 >= d.ready.1 {
                    Color::Success
                } else {
                    Color::Warning
                };
                out.push(
                    row(SharedString::from(format!("deployment-{i}")), false, cx)
                        .child(grow_cell(
                            Label::new(d.name.clone()).size(LabelSize::Small).truncate(),
                        ))
                        .child(cell(
                            70.,
                            format!("{}/{}", d.ready.0, d.ready.1),
                            ready_color,
                        ))
                        .child(cell(90., d.up_to_date.to_string(), Color::Muted))
                        .child(cell(80., d.available.to_string(), Color::Muted))
                        .child(cell(200., d.images.join(", "), Color::Muted))
                        .child(
                            h_flex()
                                .flex_none()
                                .w(gpui::px(80.))
                                .justify_end()
                                .child(button(
                                    SharedString::from(format!("restart-deployment-{i}")),
                                    "Restart",
                                    busy,
                                    cx,
                                    move |app, cx| app.confirm_restart_deployment(name.clone(), cx),
                                )),
                        )
                        .into_any_element(),
                );
            }
        }
        KubeTab::Services => {
            if let Some(empty) = pending(kube.services.as_ref()) {
                return empty;
            }
            for (i, s) in kube.services.iter().flatten().flatten().enumerate() {
                if !matches_any(filter, &[&s.name, &s.kind]) {
                    continue;
                }
                out.push(
                    row(SharedString::from(format!("service-{i}")), false, cx)
                        .child(grow_cell(
                            Label::new(s.name.clone()).size(LabelSize::Small).truncate(),
                        ))
                        .child(cell(110., s.kind.clone(), Color::Default))
                        .child(cell(120., s.cluster_ip.clone(), Color::Muted))
                        .child(cell(150., s.external.join(", "), Color::Muted))
                        .child(cell(170., s.ports.join(", "), Color::Muted))
                        .into_any_element(),
                );
            }
        }
    }
    if out.is_empty() {
        return ferrum_ui::empty_state(
            format!("No {}", kube.tab.label().to_lowercase()),
            format!("in {}", kube.namespace),
        )
        .into_any_element();
    }
    v_flex()
        .id("kube-list")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .children(out)
        .into_any_element()
}

/// The Kubernetes card: its header and its body.
pub fn render(
    app: &FerrumContainers,
    window: &Window,
    cx: &mut Context<FerrumContainers>,
) -> (AnyElement, AnyElement) {
    let kube = &app.state.kube;
    let context = kube.context.clone();
    let mut actions: Vec<AnyElement> = Vec::new();
    if context.is_some() {
        actions.push(
            button(
                "kube-context",
                format!("Context: {}", context.clone().unwrap_or_default()),
                false,
                cx,
                |app, cx| app.open_kube_picker(KubePicker::Context, cx),
            )
            .into_any_element(),
        );
        actions.push(
            button(
                "kube-namespace",
                format!("Namespace: {}", kube.namespace),
                false,
                cx,
                |app, cx| app.open_kube_picker(KubePicker::Namespace, cx),
            )
            .into_any_element(),
        );
        let tabs = KubeTab::ALL.map(|t| (t.label(), kube.tab == t, false, t));
        actions.push(
            segmented("kube-tabs", &tabs, cx, |app, tab, cx| {
                app.set_kube_tab(tab, cx)
            })
            .into_any_element(),
        );
    }
    if kube.loading {
        actions.push(muted("loading…").into_any_element());
    }
    let header = card_header(app, "Kubernetes", kube.count(), actions, window, cx);
    let body: AnyElement = if kube.checked && kube.kubectl.is_none() {
        ferrum_ui::empty_state(
            "kubectl was not found",
            "The Kubernetes section runs kubectl (PATH, Homebrew, OrbStack, Docker Desktop). Install it, then Refresh.",
        )
        .into_any_element()
    } else {
        match &kube.config {
            None => ferrum_ui::empty_state("Reading the kubeconfig…", "").into_any_element(),
            Some(Err(error)) => ferrum_ui::empty_state("kubectl config view failed", error.clone())
                .into_any_element(),
            Some(Ok(config)) if config.contexts.is_empty() => ferrum_ui::empty_state(
                "No Kubernetes contexts",
                "The kubeconfig (KUBECONFIG or ~/.kube/config) has no contexts.",
            )
            .into_any_element(),
            Some(Ok(_)) => v_flex()
                .size_full()
                .child(header_row(columns(kube.tab), cx))
                .child(rows(app, cx))
                .into_any_element(),
        }
    };
    (header, body)
}
