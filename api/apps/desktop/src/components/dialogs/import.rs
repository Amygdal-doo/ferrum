//! Import a collection file (Postman v2.1, Insomnia, OpenAPI 3) or pasted
//! text: a cURL command opens as a request, a collection / spec is added.

use super::modal;
use crate::app::FerrumApi;
use crate::components::fields::{FieldSpec, on_change};
use crate::components::hint;
use crate::state::Dialog;
use ferrum_ui::components::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonStyle, Clickable as _, Color,
    Disableable as _, Divider, Label, LabelCommon as _, LabelSize, h_flex, v_flex,
};
use ferrumapi_application::interop::detect::Imported;
use gpui::{AnyElement, Context, Window, div, prelude::*, px};

impl FerrumApi {
    fn import_curl(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Import { curl, .. }) = &self.state.dialog else {
            return;
        };
        let text = curl.trim().to_owned();
        if text.is_empty() {
            return;
        }
        let svc = self.services.clone();
        self.io(
            cx,
            async move { svc.import_pasted(text).await },
            |this, result, cx| match result {
                Ok(Imported::Request(req)) => {
                    this.state.dialog = None;
                    this.open_request(String::new(), req, None);
                }
                Ok(Imported::Collection(c, format)) => {
                    this.state.dialog = None;
                    this.add_imported(c, format, cx);
                }
                Err(e) => {
                    if let Some(Dialog::Import { error, .. }) = &mut this.state.dialog {
                        *error = Some(e);
                    }
                }
            },
        );
    }

    pub(super) fn render_import_dialog(
        &mut self,
        curl: String,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let empty = curl.trim().is_empty();
        let input = self.field(
            FieldSpec::new("import-curl")
                .value(curl)
                .multiline(6)
                .mono()
                .placeholder("curl -X POST https://api.example.com/items, or paste a collection / OpenAPI document"),
            on_change(|this, text, _| {
                if let Some(Dialog::Import { curl, error }) = &mut this.state.dialog {
                    *curl = text;
                    *error = None;
                }
            }),
            window,
            cx,
        );
        let error_color = cx.theme().status().error;
        let section = |title: &'static str| {
            Label::new(title)
                .size(LabelSize::XSmall)
                .color(Color::Muted)
        };

        modal("Import", px(576.), cx)
            .child(
                v_flex()
                    .gap_2()
                    .child(section("COLLECTION FILE"))
                    .child(hint("A Postman Collection v2.1, an Insomnia export (v4 JSON / YAML, v5 YAML) or an OpenAPI 3 document (JSON / YAML). It is added to your collections."))
                    .child(
                        div().child(
                            Button::new("import-file", "Choose file…")
                                .style(ButtonStyle::Outlined)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.state.dialog = None;
                                    this.import_collection(cx);
                                    cx.notify();
                                })),
                        ),
                    ),
            )
            .child(Divider::horizontal())
            .child(
                v_flex()
                    .gap_2()
                    .child(section("PASTE A CURL COMMAND OR A DOCUMENT"))
                    .child(input)
                    .when_some(error, |this, error| {
                        this.child(div().text_xs().text_color(error_color).child(error))
                    })
                    .child(
                        h_flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("import-cancel", "Cancel")
                                    .style(ButtonStyle::Subtle)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.state.dialog = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("import-curl-confirm", "Import")
                                    .style(ButtonStyle::Filled)
                                    .disabled(empty)
                                    .on_click(cx.listener(|this, _, _, cx| this.import_curl(cx))),
                            ),
                    ),
            )
            .into_any_element()
    }
}
