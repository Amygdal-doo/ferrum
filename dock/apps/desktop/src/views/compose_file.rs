//! Compose tab of a project panel: the compose file in a YAML-highlighted
//! editor (Save, Validate) and the output of the last command run for the
//! project (Validate, or Up / Restart / Pull / Down from the panel header).

use crate::app::FerrumContainers;
use crate::views::widgets::{button, muted};
use ferrum_ui::components::{
    Color, Icon, IconName, IconSize, Label, LabelCommon, LabelSize, StyledTypography, h_flex,
    v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

fn output(app: &FerrumContainers, name: &str, cx: &mut Context<FerrumContainers>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let Some(run) = app.state.compose_runs.get(name) else {
        return div()
            .flex_none()
            .px_3()
            .py_1()
            .border_t_1()
            .border_color(colors.border_variant)
            .child(muted(
                "Output of Validate, Up, Restart, Pull and Down shows here.",
            ))
            .into_any_element();
    };
    let (status, color) = match &run.outcome {
        None => ("running…".to_owned(), Color::Muted),
        Some(Ok(message)) => (message.clone(), Color::Success),
        Some(Err(error)) => (
            error.lines().last().unwrap_or("failed").to_owned(),
            Color::Error,
        ),
    };
    v_flex()
        .flex_none()
        .h(px(220.))
        .border_t_1()
        .border_color(colors.border_variant)
        .child(
            h_flex()
                .flex_none()
                .px_3()
                .py_1()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(muted(run.command.clone()).truncate()),
                )
                .child(Label::new(status).size(LabelSize::Small).color(color)),
        )
        .child(
            v_flex()
                .id(SharedString::from(format!("compose-output-{name}")))
                .flex_1()
                .min_h_0()
                .px_3()
                .pb_1()
                .overflow_y_scroll()
                .font_buffer(cx)
                .text_xs()
                .children(run.lines.iter().map(|line| div().child(line.clone()))),
        )
        .into_any_element()
}

pub fn render(app: &FerrumContainers, cx: &mut Context<FerrumContainers>) -> AnyElement {
    let Some(detail) = app.state.detail.as_ref() else {
        return div().into_any_element();
    };
    let Some(project) = detail.project.clone() else {
        return div().into_any_element();
    };
    let editor = &detail.compose;
    let file_label = editor
        .file
        .as_ref()
        .map_or_else(|| "no compose file".to_owned(), |f| f.display().to_string());
    let toolbar = h_flex()
        .flex_none()
        .px_3()
        .py_1()
        .gap_2()
        .child(
            Icon::new(IconName::File)
                .size(IconSize::Small)
                .color(Color::Muted),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(Label::new(file_label).size(LabelSize::Small).truncate()),
        )
        .when(editor.dirty, |d| d.child(muted("edited")))
        .child(button(
            "compose-save",
            "Save",
            !editor.dirty,
            cx,
            |app, cx| app.save_compose_file(cx),
        ))
        .child(button(
            "compose-validate",
            "Validate",
            editor.saved.is_none(),
            cx,
            |app, cx| app.validate_compose(cx),
        ));
    let body: AnyElement = match &editor.error {
        Some(error) => {
            ferrum_ui::empty_state("Compose file not available", error.clone()).into_any_element()
        }
        None => div()
            .id("compose-editor")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_3()
            .py_1()
            .child(app.inputs.compose_editor.clone())
            .into_any_element(),
    };
    v_flex()
        .flex_1()
        .min_h_0()
        .child(toolbar)
        .when(editor.dirty, |d| {
            d.child(div().px_3().child(muted(
                "Validate checks the edited text without saving it. Up, Restart and Pull use the saved file.",
            )))
        })
        .child(body)
        .child(output(app, &project.name, cx))
        .into_any_element()
}
