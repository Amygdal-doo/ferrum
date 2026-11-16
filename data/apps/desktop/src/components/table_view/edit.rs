//! Table tab editing bar (+ Row, Delete, Set NULL) and the pending-changes
//! panel: the exact SQL that Apply runs, with Apply and Discard.

use crate::app::FerrumData;
use crate::commands::edit::script_text;
use ferrum_ui::components::TintColor;
use ferrum_ui::prelude::*;
use ferrumdata_domain::EditorTabId;

impl FerrumData {
    pub(super) fn render_edit_bar(
        &mut self,
        tab_id: EditorTabId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hint = |text: String| Label::new(text).size(LabelSize::Small).color(Color::Muted);
        let read_only = self.state.active_profile().is_some_and(|p| p.read_only);
        if let Err(reason) = self.edit_target(tab_id) {
            return h_flex()
                .px_4()
                .pb_2()
                .when(!reason.is_empty(), |this| this.child(hint(reason)))
                .into_any_element();
        }
        if read_only {
            return h_flex()
                .px_4()
                .pb_2()
                .child(hint("Read-only connection: rows cannot be edited.".into()))
                .into_any_element();
        }
        let selection = self.state.runs.get(&tab_id).and_then(|r| r.selection);
        let single_cell = selection.is_some_and(|s| s.is_single_cell());
        h_flex()
            .px_4()
            .pb_2()
            .gap_2()
            .child(
                Button::new("row-add", "Row")
                    .start_icon(Icon::new(IconName::Plus).size(IconSize::Small))
                    .style(ButtonStyle::Outlined)
                    .tooltip(Tooltip::text("Add a row (column defaults until edited)"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.add_row(tab_id, window, cx);
                    })),
            )
            .child(
                Button::new("row-delete", "Delete")
                    .style(ButtonStyle::Outlined)
                    .disabled(selection.is_none())
                    .tooltip(Tooltip::text(
                        "Mark the selected rows for deletion (again to unmark)",
                    ))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.delete_selected_rows(tab_id, cx);
                    })),
            )
            .child(
                Button::new("cell-null", "Set NULL")
                    .style(ButtonStyle::Outlined)
                    .disabled(!single_cell)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_selected_null(tab_id, cx);
                    })),
            )
            .child(hint(
                "Double-click a cell to edit · Enter keeps the value · Esc cancels".into(),
            ))
            .into_any_element()
    }

    /// The staged changes as SQL, with Apply / Discard; `None` when nothing
    /// is staged.
    pub(super) fn render_pending_edits(
        &mut self,
        tab_id: EditorTabId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let run = self.state.runs.get(&tab_id)?;
        if run.edits.is_empty() {
            return None;
        }
        let count = run.edits.change_count();
        let colors = cx.theme().colors().clone();
        let script = self.edit_script(tab_id);
        let (text, error) = match &script {
            Ok(script) => (script_text(script), None),
            Err(e) => (String::new(), Some(e.clone())),
        };
        let production = self
            .state
            .active_profile()
            .is_some_and(|p| p.env.is_production());
        Some(
            v_flex()
                .mx_4()
                .mb_3()
                .p_2()
                .gap_2()
                .rounded_md()
                .border_1()
                .border_color(colors.border)
                .bg(colors.editor_background)
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Label::new(format!(
                                "{count} pending change{} · runs in one transaction",
                                if count == 1 { "" } else { "s" }
                            ))
                            .size(LabelSize::Small),
                        )
                        .child(div().flex_1())
                        .child(
                            Button::new("edits-discard", "Discard")
                                .style(ButtonStyle::Outlined)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.discard_edits(tab_id, cx);
                                })),
                        )
                        .child(
                            Button::new("edits-apply", "Apply")
                                .style(if production {
                                    ButtonStyle::Tinted(TintColor::Error)
                                } else {
                                    ButtonStyle::Filled
                                })
                                .disabled(error.is_some())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.request_apply(tab_id, cx);
                                })),
                        ),
                )
                .when_some(error, |this, error| {
                    this.child(Label::new(error).size(LabelSize::Small).color(Color::Error))
                })
                .child(
                    div()
                        .id("edits-sql")
                        .max_h(px(120.))
                        .overflow_y_scroll()
                        .font_buffer(cx)
                        .text_xs()
                        .whitespace_nowrap()
                        .child(text),
                )
                .into_any_element(),
        )
    }
}
