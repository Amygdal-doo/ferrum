//! Multipart form-data rows with a Text / File toggle.

use crate::app::FerrumApi;
use crate::commands::files::file_name;
use crate::components::widgets::{RowEditor, header_cell, row_border};
use crate::components::{checkbox, select};
use crate::state::kv::{self, RowEdit};
use ferrumapi_domain::{FormField, FormFieldKind, RequestBody, TabId, VariableSet};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};
use std::rc::Rc;
use ui::{
    ActiveTheme as _, ButtonCommon as _, Clickable as _, IconButton, IconName, IconSize,
    StyledTypography as _, Tooltip, VisibleOnHover as _, h_flex, v_flex,
};

impl FerrumApi {
    pub(super) fn render_form_data(
        &mut self,
        tab_id: TabId,
        fields: Vec<FormField>,
        vars: &VariableSet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let border = row_border(cx);
        let edit: RowEditor = Rc::new(move |this, i, e, _| {
            this.state.edit_request(tab_id, |r| {
                if let RequestBody::FormData { fields } = &mut r.body {
                    kv::apply(fields, i, e);
                }
            });
        });
        let scope = format!("formdata:{tab_id}");
        let n = fields.len();
        let mut display = fields;
        display.push(FormField::default());

        let mut body = Vec::new();
        for (i, row) in display.into_iter().enumerate() {
            let blank = i == n;
            let is_file = row.kind == FormFieldKind::File;
            let bad = if is_file {
                Vec::new()
            } else {
                vars.unresolved(&row.value)
            };
            let key_cell = self.kv_cell(
                format!("{scope}:{i}:key"),
                &row.key,
                if blank { "Key" } else { "" },
                &[],
                {
                    let edit = edit.clone();
                    move |this, text, cx| edit(this, i, RowEdit::Key(text), cx)
                },
                window,
                cx,
            );
            let kind_select = {
                let edit = edit.clone();
                select(
                    SharedString::from(format!("{scope}-kind-{i}")),
                    if is_file { "File" } else { "Text" },
                    vec![("Text".into(), false), ("File".into(), true)],
                    move |this, file, cx| edit(this, i, RowEdit::File(file), cx),
                    window,
                    cx,
                )
            };
            let value_cell = if is_file {
                let shown = if row.value.is_empty() {
                    "Select file…".to_owned()
                } else {
                    file_name(&row.value)
                };
                let edit = edit.clone();
                div()
                    .id(SharedString::from(format!("{scope}-file-{i}")))
                    .h(px(28.))
                    .px_2()
                    .flex()
                    .items_center()
                    .truncate()
                    .text_xs()
                    .font_buffer(cx)
                    .cursor_pointer()
                    .hover(|s| s.bg(colors.element_hover))
                    .when(!row.value.is_empty(), |this| {
                        this.tooltip(Tooltip::text(row.value.clone()))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let edit = edit.clone();
                        this.pick_then(cx, "Choose a file", false, move |this, path, cx| {
                            edit(
                                this,
                                i,
                                RowEdit::Value(path.to_string_lossy().into_owned()),
                                cx,
                            );
                        });
                    }))
                    .child(shown)
                    .into_any_element()
            } else {
                let edit = edit.clone();
                self.kv_cell(
                    format!("{scope}:{i}:value"),
                    &row.value,
                    if blank { "Value" } else { "" },
                    &bad,
                    move |this, text, cx| edit(this, i, RowEdit::Value(text), cx),
                    window,
                    cx,
                )
            };
            let group: SharedString = format!("{scope}-row-{i}").into();
            body.push(
                h_flex()
                    .group(group.clone())
                    .border_b_1()
                    .border_color(border)
                    .when(!row.enabled && !blank, |this| this.opacity(0.5))
                    .child(div().w_8().flex().justify_center().when(!blank, |this| {
                        let edit = edit.clone();
                        this.child(checkbox(
                            SharedString::from(format!("{scope}-enabled-{i}")),
                            row.enabled,
                            move |this, on, cx| edit(this, i, RowEdit::Enabled(on), cx),
                            cx,
                        ))
                    }))
                    .child(div().flex_1().min_w_0().child(key_cell))
                    .child(div().w(px(88.)).px_1().child(kind_select))
                    .child(div().flex_1().min_w_0().child(value_cell))
                    .child(div().w_8().flex().justify_center().when(!blank, |this| {
                        let edit = edit.clone();
                        this.child(
                            div().visible_on_hover(group).child(
                                IconButton::new(
                                    SharedString::from(format!("{scope}-delete-{i}")),
                                    IconName::Close,
                                )
                                .icon_size(IconSize::XSmall)
                                .tooltip(Tooltip::text("Delete row"))
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        edit(this, i, RowEdit::Remove, cx);
                                        cx.notify();
                                    },
                                )),
                            ),
                        )
                    }))
                    .into_any_element(),
            );
        }

        v_flex()
            .w_full()
            .child(
                h_flex()
                    .border_b_1()
                    .border_color(border)
                    .child(div().w_8())
                    .child(div().flex_1().child(header_cell("Key")))
                    .child(div().w(px(88.)).child(header_cell("Type")))
                    .child(div().flex_1().child(header_cell("Value")))
                    .child(div().w_8()),
            )
            .children(body)
            .into_any_element()
    }
}
