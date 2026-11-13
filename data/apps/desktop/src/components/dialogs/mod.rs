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
