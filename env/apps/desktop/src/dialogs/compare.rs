//! Compare the current environment with another one of the project:
//! keys missing on either side ("staging lacks STRIPE_KEY") and keys whose
//! values differ. Only key names are shown.

use ferrum_ui::prelude::*;
use ferrumenv_core::diff;

use super::Dialog;
use crate::app::EnvApp;
use crate::widgets::{muted, primary_button};

pub(crate) struct CompareDialog {
    /// The environment compared against the current one.
    pub(crate) other: usize,
}

impl EnvApp {
    pub(crate) fn open_compare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(u) = self.vault() else {
            return;
        };
        let Some(project) = u.project() else {
            return;
        };
        let Some(other) = (0..project.environments.len()).find(|&i| i != u.env) else {
            self.show_error("Add a second environment to compare with.", cx);
            return;
        };
        self.open_dialog(
            Dialog::Compare(CompareDialog { other }),
            Vec::new(),
            None,
            window,
            cx,
        );
    }

    pub(crate) fn render_compare(&self, d: &CompareDialog, cx: &mut Context<Self>) -> Modal {
        let modal = self.modal("Compare environments", cx).width(px(560.));
        let Some(u) = self.vault() else {
            return modal;
        };
        let (Some(project), Some(left)) = (u.project(), u.environment()) else {
            return modal;
        };
        let Some(right) = project.environments.get(d.other) else {
            return modal;
        };
        let mut picker = h_flex()
            .gap_1()
            .flex_wrap()
            .child(muted(format!("{} vs", left.name)));
        for (i, env) in project.environments.iter().enumerate() {
            if i == u.env {
                continue;
            }
            picker = picker.child(
                Button::new(SharedString::from(format!("cmp-{i}")), env.name.clone())
                    .size(ButtonSize::Compact)
                    .label_size(LabelSize::Small)
                    .toggle_state(i == d.other)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(Dialog::Compare(d)) = this.dialog_mut() {
                            d.other = i;
                        }
                        cx.notify();
                    })),
            );
        }
        let comparison = diff::compare(&left.variables, &right.variables);
        let report = comparison.missing_report(&left.name, &right.name);
        let mut body = v_flex().gap_1();
        if report.is_empty() {
            body = body.child(
                Label::new("Both environments define the same keys.")
                    .size(LabelSize::Small)
                    .color(Color::Success),
            );
        }
        for line in report {
            body = body.child(
                Label::new(line)
                    .size(LabelSize::Small)
                    .color(Color::Warning),
            );
        }
        if !comparison.differing.is_empty() {
            body = body.child(muted(format!(
                "Different values: {}",
                comparison.differing.join(", ")
            )));
        }
        body = body.child(muted(format!(
            "{} keys with the same value",
            comparison.same
        )));
        modal.child(picker).child(body).footer(primary_button(
            "compare-done",
            "Done",
            cx,
            |this, window, cx| {
                this.close_dialog(window, cx);
            },
        ))
    }
}
