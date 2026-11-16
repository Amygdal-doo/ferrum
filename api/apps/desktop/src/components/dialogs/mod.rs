//! In-app modal dialogs.

mod code;
mod collection_settings;
mod cookies;
mod draft_tests;
mod environment;
mod import;
mod load_test;
mod mock_editor;
mod mock_server;
mod new_request;
mod runner;
mod save_request;
mod webhooks;

pub use new_request::new_request_menu;

use crate::app::FerrumApi;
use crate::state::Dialog;
use ferrum_ui::Modal;
use ferrum_ui::components::{
    Button, ButtonCommon as _, ButtonStyle, Clickable as _, Disableable as _, Label,
    LabelCommon as _, LabelSize, TintColor,
};
use gpui::{AnyElement, ClickEvent, Context, Pixels, SharedString, Window, prelude::*, px};

/// Dialog shell: title, width, closes on backdrop / close button.
pub fn modal(title: impl Into<SharedString>, width: Pixels, cx: &mut Context<FerrumApi>) -> Modal {
    Modal::new(title)
        .width(width)
        .on_dismiss(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.state.dialog = None;
            cx.notify();
        }))
}

/// Footer buttons: Cancel and a primary action.
pub fn actions(
    modal: Modal,
    confirm: &'static str,
    destructive: bool,
    disabled: bool,
    on_confirm: impl Fn(&mut FerrumApi, &mut Context<FerrumApi>) + 'static,
    cx: &mut Context<FerrumApi>,
) -> Modal {
    modal
        .footer(
            Button::new("dialog-cancel", "Cancel")
                .style(ButtonStyle::Subtle)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.state.dialog = None;
                    cx.notify();
                })),
        )
        .footer(
            Button::new("dialog-confirm", confirm)
                .style(if destructive {
                    ButtonStyle::Tinted(TintColor::Error)
                } else {
                    ButtonStyle::Filled
                })
                .disabled(disabled)
                .on_click(cx.listener(move |this, _, _, cx| {
                    on_confirm(this, cx);
                    cx.notify();
                })),
        )
}

fn body_text(text: String) -> Label {
    Label::new(text).size(LabelSize::Default)
}

impl FerrumApi {
    pub fn render_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let open = self.state.dialog.is_some();
        if open && !self.dialog_focused {
            // Keys must not keep going to the field behind the backdrop.
            window.focus(&self.root_focus(), cx);
        }
        self.dialog_focused = open;
        let dialog = self.state.dialog.clone()?;
        let element = match dialog {
            Dialog::SaveRequest {
                tab_id,
                name,
                new_collection,
                target,
            } => self.render_save_request(tab_id, name, new_collection, target, window, cx),
            Dialog::CollectionSettings { draft, pane } => {
                self.render_collection_settings(*draft, pane, window, cx)
            }
            Dialog::EditEnvironment(env) => self.render_environment_dialog(*env, window, cx),
            Dialog::Import { curl, error } => self.render_import_dialog(curl, error, window, cx),
            Dialog::Code { tab_id, target } => self.render_code_dialog(tab_id, target, window, cx),
            Dialog::Cookies(jar) => self.render_cookies_dialog(*jar, cx),
            Dialog::Runner => self.render_runner_dialog(cx),
            Dialog::MockServer(collection) => self.render_mock_dialog(collection, window, cx),
            Dialog::Webhooks => self.render_webhooks_dialog(window, cx),
            Dialog::NewRequest { target } => self.render_new_request_dialog(target, cx),
            Dialog::LoadTest => self.render_load_test_dialog(window, cx),
            Dialog::DraftTests => self.render_draft_tests_dialog(window, cx),
            Dialog::DeleteItem {
                collection,
                item,
                name,
            } => {
                let what = if item.is_some() { "item" } else { "collection" };
                actions(
                    modal(format!("Delete {what}?"), px(480.), cx).child(body_text(format!(
                        "“{name}” and everything inside it will be deleted. Open tabs are kept as unsaved requests."
                    ))),
                    "Delete",
                    true,
                    false,
                    move |this, cx| {
                        this.state.dialog = None;
                        this.delete_item(collection, item, cx);
                    },
                    cx,
                )
                .into_any_element()
            }
            Dialog::DeleteEnvironment(id, name) => actions(
                modal("Delete environment?", px(480.), cx).child(body_text(format!(
                    "“{name}” and its variables will be deleted."
                ))),
                "Delete",
                true,
                false,
                move |this, cx| {
                    this.state.dialog = None;
                    this.delete_environment(id, cx);
                },
                cx,
            )
            .into_any_element(),
            Dialog::ClearHistory => actions(
                modal("Clear history?", px(480.), cx).child(body_text(
                    "All history entries will be deleted. This cannot be undone.".to_owned(),
                )),
                "Clear history",
                true,
                false,
                |this, cx| {
                    this.state.dialog = None;
                    this.clear_history(cx);
                },
                cx,
            )
            .into_any_element(),
        };
        Some(element)
    }
}
