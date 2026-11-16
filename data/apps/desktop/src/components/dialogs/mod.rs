//! In-app modal dialogs.

use crate::app::FerrumData;
use crate::state::Dialog;
use ferrum_ui::components::TintColor;
use ferrum_ui::prelude::*;

impl FerrumData {
    pub fn render_dialog(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.state.dialog.clone()?;
        fn close(
            cx: &mut Context<FerrumData>,
        ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
            cx.listener(|this, _: &ClickEvent, window, cx| this.close_dialog(window, cx))
        }
        match dialog {
            Dialog::ConnectionForm(_) => self.form.clone().map(IntoElement::into_any_element),
            Dialog::DeleteProfile(profile) => {
                let id = profile.id;
                Some(
                    Modal::new("Delete connection?")
                        .on_dismiss(close(cx))
                        .child(Label::new(format!(
                            "“{}” and its saved password will be removed. Editor tabs are kept.",
                            profile.display_name
                        )))
                        .footer(Button::new("delete-cancel", "Cancel").on_click(close(cx)))
                        .footer(
                            Button::new("delete-confirm", "Delete")
                                .style(ButtonStyle::Tinted(TintColor::Error))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.close_dialog(window, cx);
                                    this.delete_profile(id, cx);
                                })),
                        )
                        .into_any_element(),
                )
            }
            Dialog::ConfirmRun {
                tab_id,
                profile,
                sql,
            } => {
                let colors = cx.theme().colors().clone();
                let preview: String = sql.chars().take(600).collect();
                let target = format!(
                    "This statement may modify data or schema on {} ({}), tagged Production.",
                    profile.display_name,
                    profile.target_label()
                );
                Some(
                    Modal::new("Run on production?")
                        .on_dismiss(close(cx))
                        .child(Label::new(target))
                        .child(
                            div()
                                .id("confirm-preview")
                                .max_h(px(192.))
                                .overflow_y_scroll()
                                .rounded_md()
                                .p_3()
                                .bg(colors.editor_background)
                                .font_buffer(cx)
                                .text_xs()
                                .child(preview),
                        )
                        .footer(Button::new("confirm-cancel", "Cancel").on_click(close(cx)))
                        .footer(
                            Button::new("confirm-run", "Run on production")
                                .style(ButtonStyle::Tinted(TintColor::Error))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.close_dialog(window, cx);
                                    this.start_run(tab_id, profile.clone(), sql.clone(), cx);
                                })),
                        )
                        .into_any_element(),
                )
            }
            Dialog::ConfirmApply {
                tab_id,
                profile,
                sql,
            } => {
                let colors = cx.theme().colors().clone();
                let preview: String = sql.chars().take(2000).collect();
                let target = format!(
                    "These changes will be written to {} ({}), tagged Production.",
                    profile.display_name,
                    profile.target_label()
                );
                Some(
                    Modal::new("Apply changes on production?")
                        .on_dismiss(close(cx))
                        .child(Label::new(target))
                        .child(
                            div()
                                .id("apply-preview")
                                .max_h(px(192.))
                                .overflow_y_scroll()
                                .rounded_md()
                                .p_3()
                                .bg(colors.editor_background)
                                .font_buffer(cx)
                                .text_xs()
                                .child(preview),
                        )
                        .footer(Button::new("apply-cancel", "Cancel").on_click(close(cx)))
                        .footer(
                            Button::new("apply-confirm", "Apply on production")
                                .style(ButtonStyle::Tinted(TintColor::Error))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.close_dialog(window, cx);
                                    this.apply_edits(tab_id, cx);
                                })),
                        )
                        .into_any_element(),
                )
            }
            Dialog::ConfirmAnalyze {
                tab_id,
                profile,
                sql,
            } => {
                let colors = cx.theme().colors().clone();
                let preview: String = sql.chars().take(600).collect();
                let mutating = ferrumdata_application::safety::may_mutate(&sql);
                let text = if mutating {
                    "EXPLAIN ANALYZE executes the statement. This one may change data or schema, \
                     and the change is kept."
                } else {
                    "EXPLAIN ANALYZE executes the statement to measure it, which takes as long \
                     as running it and puts load on the server."
                };
                Some(
                    Modal::new("Run EXPLAIN ANALYZE?")
                        .on_dismiss(close(cx))
                        .child(Label::new(text))
                        .when(profile.env.is_production(), |this| {
                            this.child(Label::new(format!(
                                "{} is tagged Production.",
                                profile.display_name
                            )))
                        })
                        .child(
                            div()
                                .id("analyze-preview")
                                .max_h(px(192.))
                                .overflow_y_scroll()
                                .rounded_md()
                                .p_3()
                                .bg(colors.editor_background)
                                .font_buffer(cx)
                                .text_xs()
                                .child(preview),
                        )
                        .footer(Button::new("analyze-cancel", "Cancel").on_click(close(cx)))
                        .footer(
                            Button::new("analyze-confirm", "Run and analyze")
                                .style(if mutating || profile.env.is_production() {
                                    ButtonStyle::Tinted(TintColor::Error)
                                } else {
                                    ButtonStyle::Filled
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.close_dialog(window, cx);
                                    this.start_explain(
                                        tab_id,
                                        profile.clone(),
                                        sql.clone(),
                                        true,
                                        cx,
                                    );
                                })),
                        )
                        .into_any_element(),
                )
            }
            Dialog::DeleteKey { profile, key } => {
                let id = profile.id;
                Some(
                    Modal::new("Delete key?")
                        .on_dismiss(close(cx))
                        .child(Label::new(format!(
                            "“{key}” will be deleted from {} ({}). This cannot be undone.",
                            profile.display_name,
                            profile.target_label()
                        )))
                        .footer(Button::new("delete-key-cancel", "Cancel").on_click(close(cx)))
                        .footer(
                            Button::new("delete-key-confirm", "Delete key")
                                .style(ButtonStyle::Tinted(TintColor::Error))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.close_dialog(window, cx);
                                    this.delete_key(id, key.clone(), cx);
                                })),
                        )
                        .into_any_element(),
                )
            }
            Dialog::ClearHistory(profile) => {
                let id = profile.as_ref().map(|p| p.id);
                let text = profile.map_or_else(
                    || "The query history of every connection will be deleted.".to_owned(),
                    |p| format!("The query history of “{}” will be deleted.", p.display_name),
                );
                Some(
                    Modal::new("Clear query history?")
                        .on_dismiss(close(cx))
                        .child(Label::new(text))
                        .footer(Button::new("clear-history-cancel", "Cancel").on_click(close(cx)))
                        .footer(
                            Button::new("clear-history-confirm", "Clear history")
                                .style(ButtonStyle::Tinted(TintColor::Error))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.close_dialog(window, cx);
                                    this.clear_history(id, cx);
                                })),
                        )
                        .into_any_element(),
                )
            }
        }
    }
}
