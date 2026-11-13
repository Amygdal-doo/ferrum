//! Body mode selector and per-mode editors.

use crate::app::FerrumApi;
use crate::commands::files::file_name;
use crate::components::fields::{FieldSpec, on_request_edit};
use crate::components::widgets::RowEditor;
use crate::components::{hint, select};
use crate::state::kv;
use ferrum_ui::components::{
    Button, ButtonCommon as _, ButtonSize, ButtonStyle, Clickable as _, Color, Label,
    LabelCommon as _, LabelSize, StyledTypography as _, Toggleable as _, Tooltip, h_flex, v_flex,
};
use ferrumapi_domain::text::pretty_json;
use ferrumapi_domain::{RawLanguage, RequestBody, TabId, VariableSet};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};
use std::rc::Rc;

const MODES: [&str; 5] = [
    "none",
    "form-data",
    "x-www-form-urlencoded",
    "raw",
    "binary",
];

fn empty_body(mode: &str) -> RequestBody {
    match mode {
        "form-data" => RequestBody::FormData { fields: Vec::new() },
        "x-www-form-urlencoded" => RequestBody::UrlEncoded { fields: Vec::new() },
        "raw" => RequestBody::Raw {
            language: RawLanguage::Json,
            text: String::new(),
        },
        "binary" => RequestBody::Binary { path: None },
        _ => RequestBody::None,
    }
}

impl FerrumApi {
    pub(super) fn render_body_editor(
        &mut self,
        tab_id: TabId,
        body: RequestBody,
        vars: &VariableSet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mode = body.mode_label();
        let modes = h_flex()
            .gap_1()
            .flex_wrap()
            .children(MODES.into_iter().map(|m| {
                Button::new(SharedString::from(format!("body-mode-{m}")), m)
                    .style(ButtonStyle::Subtle)
                    .size(ButtonSize::Compact)
                    .label_size(LabelSize::Small)
                    .toggle_state(m == mode)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let current = this.state.tab(tab_id).map(|t| t.request.body.mode_label());
                        if current != Some(m) {
                            this.state.edit_request(tab_id, |r| r.body = empty_body(m));
                        }
                        cx.notify();
                    }))
            }));

        let editor = match body {
            RequestBody::None => hint("This request does not have a body.").into_any_element(),
            RequestBody::Raw { language, text } => {
                self.render_raw_editor(tab_id, language, text, window, cx)
            }
            RequestBody::UrlEncoded { fields } => {
                let on_edit: RowEditor = Rc::new(move |this, i, edit, _| {
                    this.state.edit_request(tab_id, |r| {
                        if let RequestBody::UrlEncoded { fields } = &mut r.body {
                            kv::apply(fields, i, edit);
                        }
                    });
                });
                self.render_kv_table(
                    format!("urlencoded:{tab_id}"),
                    fields,
                    vars,
                    "Key",
                    on_edit,
                    window,
                    cx,
                )
            }
            RequestBody::FormData { fields } => {
                self.render_form_data(tab_id, fields, vars, window, cx)
            }
            RequestBody::Binary { path } => h_flex()
                .gap_3()
                .child(
                    Button::new("binary-pick", "Select file…")
                        .style(ButtonStyle::Outlined)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.pick_then(
                                cx,
                                "Choose a file to send",
                                false,
                                move |this, path, _| {
                                    let path = path.to_string_lossy().into_owned();
                                    this.state.edit_request(tab_id, |r| {
                                        r.body = RequestBody::Binary { path: Some(path) };
                                    });
                                },
                            );
                        })),
                )
                .child(match path {
                    Some(p) => div()
                        .id("binary-path")
                        .text_xs()
                        .font_buffer(cx)
                        .tooltip(Tooltip::text(p.clone()))
                        .child(file_name(&p))
                        .into_any_element(),
                    None => Label::new("No file selected")
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .into_any_element(),
                })
                .into_any_element(),
        };

        v_flex()
            .size_full()
            .gap_3()
            .child(modes)
            .child(editor)
            .into_any_element()
    }

    fn render_raw_editor(
        &mut self,
        tab_id: TabId,
        language: RawLanguage,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let language_select = select(
            SharedString::from(format!("raw-language-{tab_id}")),
            language.label(),
            RawLanguage::ALL
                .into_iter()
                .map(|l| (SharedString::from(l.label()), l))
                .collect(),
            move |this, lang, _| {
                this.state.edit_request(tab_id, |r| {
                    if let RequestBody::Raw { language, .. } = &mut r.body {
                        *language = lang;
                    }
                });
            },
            window,
            cx,
        );
        let editor = self.field(
            FieldSpec::new(format!("raw:{tab_id}"))
                .value(text.clone())
                .multiline(10)
                .mono(),
            on_request_edit(move |this, value, _| {
                this.state.edit_request(tab_id, |r| {
                    if let RequestBody::Raw { text, .. } = &mut r.body {
                        *text = value;
                    }
                });
            }),
            window,
            cx,
        );
        v_flex()
            .gap_2()
            .child(h_flex().gap_2().child(language_select).when(
                language == RawLanguage::Json,
                |this| {
                    this.child(
                        Button::new("beautify", "Beautify")
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .tooltip(Tooltip::text("Pretty-print the JSON body"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                match pretty_json(&text) {
                                    Some(pretty) => this.state.edit_request(tab_id, |r| {
                                        if let RequestBody::Raw { text, .. } = &mut r.body {
                                            *text = pretty;
                                        }
                                    }),
                                    None => this.notify("The body is not valid JSON.", true, cx),
                                }
                                cx.notify();
                            })),
                    )
                },
            ))
            .child(div().min_h(px(160.)).child(editor))
            .into_any_element()
    }
}
