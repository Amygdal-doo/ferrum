//! Kubernetes dialogs: the context / namespace picker and a pod's logs.

use crate::app::FerrumContainers;
use crate::commands::kube::logs_caption;
use crate::state::KubePicker;
use crate::views::widgets::muted;
use ferrum_ui::Modal;
use ferrum_ui::components::{
    Button, Clickable, Color, Icon, IconName, IconSize, Label, LabelCommon, LabelSize, h_flex,
    v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

fn close(
    cx: &mut Context<FerrumContainers>,
) -> impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static {
    cx.listener(|app, _, _, cx| {
        app.state.dialog = None;
        cx.notify();
    })
}

/// A row of the picker: (name, detail, selected).
type Choice = (String, String, bool);

pub fn picker(
    app: &FerrumContainers,
    picker: KubePicker,
    cx: &mut Context<FerrumContainers>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let kube = &app.state.kube;
    let (title, items, note): (&str, Result<Vec<Choice>, String>, &str) = match picker {
        KubePicker::Context => (
            "Kubernetes context",
            match &kube.config {
                Some(Ok(config)) => Ok(config
                    .contexts
                    .iter()
                    .map(|c| {
                        let current = config.current.as_deref() == Some(c.name.as_str());
                        let detail = format!(
                            "cluster {}{}",
                            c.cluster,
                            if current {
                                " · kubeconfig current"
                            } else {
                                ""
                            }
                        );
                        (
                            c.name.clone(),
                            detail,
                            kube.context.as_deref() == Some(&c.name),
                        )
                    })
                    .collect()),
                Some(Err(e)) => Err(e.clone()),
                None => Ok(Vec::new()),
            },
            "Only what FerrumContainers shows changes; the kubeconfig's current context stays as it is.",
        ),
        KubePicker::Namespace => (
            "Namespace",
            match &kube.namespaces {
                Some(Ok(list)) => Ok(list
                    .iter()
                    .map(|n| (n.clone(), String::new(), *n == kube.namespace))
                    .collect()),
                Some(Err(e)) => Err(e.clone()),
                None => Ok(Vec::new()),
            },
            "",
        ),
    };
    let loading = picker == KubePicker::Namespace && kube.namespaces.is_none();
    let body: AnyElement = match items {
        Err(error) => Label::new(error)
            .size(LabelSize::Small)
            .color(Color::Error)
            .into_any_element(),
        Ok(items) => v_flex()
            .id("kube-picker")
            .max_h(px(420.))
            .overflow_y_scroll()
            .gap_0p5()
            .children(
                items
                    .into_iter()
                    .enumerate()
                    .map(|(i, (name, detail, selected))| {
                        let pick = name.clone();
                        h_flex()
                            .id(SharedString::from(format!("kube-pick-{i}")))
                            .px_2()
                            .py_1()
                            .gap_2()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|s| s.bg(colors.element_hover))
                            .when(selected, |d| d.bg(colors.element_selected))
                            .child(
                                Icon::new(if selected {
                                    IconName::Check
                                } else {
                                    IconName::Cloud
                                })
                                .size(IconSize::Small)
                                .color(if selected {
                                    Color::Success
                                } else {
                                    Color::Muted
                                }),
                            )
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(Label::new(name).size(LabelSize::Small).truncate())
                                    .when(!detail.is_empty(), |d| d.child(muted(detail))),
                            )
                            .on_click(cx.listener(move |app, _, _, cx| match picker {
                                KubePicker::Context => app.pick_kube_context(pick.clone(), cx),
                                KubePicker::Namespace => app.pick_kube_namespace(pick.clone(), cx),
                            }))
                    }),
            )
            .into_any_element(),
    };
    Modal::new(title)
        .width(px(520.))
        .on_dismiss(close(cx))
        .when(loading, |m| m.child(muted("Asking kubectl…")))
        .child(body)
        .when(!note.is_empty(), |m| m.child(muted(note)))
        .footer(Button::new("kube-picker-close", "Close").on_click(close(cx)))
        .into_any_element()
}

pub fn logs(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let Some((pod, result)) = app.state.kube.logs.as_ref() else {
        return div().into_any_element();
    };
    let status: AnyElement = match result {
        None => muted("Reading logs…").into_any_element(),
        Some(Err(_)) => Label::new("kubectl logs failed")
            .size(LabelSize::Small)
            .color(Color::Error)
            .into_any_element(),
        Some(Ok(_)) => muted(logs_caption(pod)).into_any_element(),
    };
    let reload = pod.clone();
    Modal::new(format!("Logs of {pod}"))
        .width(px(900.))
        .on_dismiss(close(cx))
        .child(status)
        .child(
            div()
                .id("pod-logs")
                .h(px(460.))
                .overflow_y_scroll()
                .px_2()
                .rounded_md()
                .border_1()
                .border_color(colors.border_variant)
                .bg(colors.editor_background)
                .child(app.inputs.pod_logs.clone()),
        )
        .footer(
            Button::new("pod-logs-refresh", "Refresh")
                .on_click(cx.listener(move |app, _, _, cx| app.open_pod_logs(reload.clone(), cx))),
        )
        .footer(Button::new("pod-logs-close", "Close").on_click(close(cx)))
        .into_any_element()
}
