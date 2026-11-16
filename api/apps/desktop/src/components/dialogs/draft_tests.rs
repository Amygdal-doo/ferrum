//! Review of agent-drafted test scripts: pick a model, draft, edit, tick
//! and save. Nothing is saved before "Save selected".

use super::modal;
use crate::app::FerrumApi;
use crate::components::fields::{FieldSpec, on_change};
use crate::components::{checkbox, hint, select};
use crate::state::drafts::{DraftRow, DraftStatus};
use ferrum_ui::components::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonStyle, Clickable as _, Disableable as _,
    Label, LabelCommon as _, LabelSize, LoadingLabel, StyledTypography as _, h_flex, v_flex,
};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};

const PRIVACY: &str = "Sends each request's name, method, URL (variable names, not values), \
header names, body, description and last response (truncated) to the chosen model. \
Auth and header values are never sent.";

impl FerrumApi {
    pub(super) fn render_draft_tests_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(state) = self.state.drafts.clone() else {
            return div().into_any_element();
        };
        let usable: Vec<(SharedString, _)> = state
            .models
            .iter()
            .filter(|m| m.enabled)
            .map(|m| (SharedString::from(m.label.clone()), m.clone()))
            .collect();
        let no_models = usable.is_empty();
        let picker = select(
            "draft-model",
            state
                .model
                .as_ref()
                .map_or_else(|| "No model".to_owned(), |m| m.label.clone()),
            usable,
            |this, m, _| {
                if let Some(s) = this.state.drafts.as_mut() {
                    s.model = Some(m);
                }
            },
            window,
            cx,
        );
        let soon: Vec<String> = state
            .models
            .iter()
            .filter(|m| !m.enabled)
            .map(|m| format!("{} (coming soon)", m.label))
            .collect();
        let any_ready = state
            .rows
            .iter()
            .any(|r| matches!(r.status, DraftStatus::Ready { .. }));
        let draft_label = if any_ready { "Draft again" } else { "Draft" };
        let header = v_flex()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .child(Label::new("Model").size(LabelSize::Small))
                    .child(picker)
                    .child(
                        Button::new("draft-start", draft_label)
                            .style(ButtonStyle::Filled)
                            .disabled(state.running || state.model.is_none())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.start_drafting(cx);
                                cx.notify();
                            })),
                    )
                    .when(state.running, |this| {
                        this.child(LoadingLabel::new("Drafting").size(LabelSize::Small))
                    }),
            )
            .when(no_models, |this| {
                this.child(hint(
                    "No model is set up: add one in Settings › Models and API keys (shared by every Ferrum app).",
                ))
            })
            .when(!soon.is_empty(), |this| this.child(hint(soon.join(", "))))
            .child(hint(PRIVACY));
        let rows: Vec<AnyElement> = state
            .rows
            .iter()
            .enumerate()
            .map(|(i, row)| self.render_draft_row(i, row, window, cx))
            .collect();
        let accepted = state.accepted().len();
        modal("Draft tests with agent", px(720.), cx)
            .child(header)
            .child(
                div()
                    .id("draft-rows")
                    .max_h(px(440.))
                    .overflow_y_scroll()
                    .child(v_flex().gap_3().children(rows)),
            )
            .child(hint(
                "Saved scripts replace empty Tests, or go below the existing lines.",
            ))
            .footer(
                Button::new("draft-discard", "Discard")
                    .style(ButtonStyle::Subtle)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.drafts = None;
                        this.state.dialog = None;
                        cx.notify();
                    })),
            )
            .footer(
                Button::new("draft-save", format!("Save selected ({accepted})"))
                    .style(ButtonStyle::Filled)
                    .disabled(accepted == 0 || state.running)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.save_drafts(cx);
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    fn render_draft_row(
        &mut self,
        i: usize,
        row: &DraftRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let error = cx.theme().status().error;
        let tick = checkbox(
            SharedString::from(format!("draft-use-{i}")),
            row.selected,
            move |this, on, _| {
                if let Some(r) = this.state.drafts.as_mut().and_then(|s| s.rows.get_mut(i)) {
                    r.selected = on;
                }
            },
            cx,
        )
        .label(row.name.clone());
        let body: AnyElement = match &row.status {
            DraftStatus::Waiting => hint("Not drafted yet.").into_any_element(),
            DraftStatus::Drafting => LoadingLabel::new("Drafting")
                .size(LabelSize::Small)
                .into_any_element(),
            DraftStatus::Failed(e) => div()
                .text_xs()
                .text_color(error)
                .child(e.clone())
                .into_any_element(),
            DraftStatus::Ready { script, problems } => {
                let editor = self.field(
                    FieldSpec::new(format!("draft-script-{i}"))
                        .value(script.clone())
                        .multiline(4)
                        .mono(),
                    on_change(move |this, text, _| {
                        if let Some(r) = this.state.drafts.as_mut().and_then(|s| s.rows.get_mut(i))
                        {
                            r.status = DraftStatus::Ready {
                                problems: ferrumapi_domain::script::check(&text),
                                script: text,
                            };
                        }
                    }),
                    window,
                    cx,
                );
                v_flex()
                    .gap_1()
                    .child(editor)
                    .children(problems.iter().take(5).map(|(line, msg)| {
                        div()
                            .text_xs()
                            .font_buffer(cx)
                            .text_color(error)
                            .child(format!("line {line}: {msg}"))
                    }))
                    .into_any_element()
            }
        };
        v_flex().gap_1().child(tick).child(body).into_any_element()
    }
}
