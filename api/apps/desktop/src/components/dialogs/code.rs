//! Code snippet for the request (variables substituted, auth resolved).

use super::modal;
use crate::app::FerrumApi;
use crate::components::fields::FieldSpec;
use crate::components::select;
use crate::state::Dialog;
use ferrumapi_application::code_snippet;
use ferrumapi_application::interop::codegen::CodeTarget;
use ferrumapi_domain::TabId;
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};
use ui::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonStyle, Clickable as _, Disableable as _,
    h_flex,
};

impl FerrumApi {
    pub(super) fn render_code_dialog(
        &mut self,
        tab_id: TabId,
        target: CodeTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(tab) = self.state.tab(tab_id).cloned() else {
            self.state.dialog = None;
            return div().into_any_element();
        };
        let snippet = code_snippet(target, &self.state.prepare_tab(&tab));
        let shown = if snippet.is_empty() {
            "// No snippet could be generated for this request.".to_owned()
        } else {
            snippet.clone()
        };
        let language = select(
            "code-language",
            target.label(),
            CodeTarget::ALL
                .into_iter()
                .map(|t| (SharedString::from(t.label()), t))
                .collect(),
            move |this, t, _| {
                if let Some(Dialog::Code { target, .. }) = &mut this.state.dialog {
                    *target = t;
                }
            },
            window,
            cx,
        );
        let view = self.field(
            FieldSpec::new(format!("code:{tab_id}"))
                .value(shown)
                .multiline(12)
                .mono()
                .read_only(),
            |_, _, _, _| {},
            window,
            cx,
        );
        let warning = cx.theme().status().warning;
        let empty = snippet.is_empty();

        modal("Code snippet", px(768.), cx)
            .child(
                h_flex()
                    .gap_2()
                    .child(language)
                    .child(div().flex_1())
                    .child(
                        Button::new("copy-snippet", "Copy")
                            .style(ButtonStyle::Outlined)
                            .disabled(empty)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.copy_text(snippet.clone(), cx);
                            })),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(warning)
                    .child("Variables are substituted, so the snippet may contain secrets."),
            )
            .child(view)
            .footer(
                Button::new("close-code", "Close")
                    .style(ButtonStyle::Subtle)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.dialog = None;
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}
