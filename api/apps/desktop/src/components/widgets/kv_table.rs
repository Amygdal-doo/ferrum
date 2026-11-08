//! Key / value table (params, headers, urlencoded) with an enable checkbox,
//! delete button and an auto-appended blank row.

use super::{header_cell, row_border, unresolved_title};
use crate::app::FerrumApi;
use crate::components::checkbox;
use crate::components::fields::{FieldSpec, on_request_edit};
use crate::state::kv::RowEdit;
use ferrumapi_domain::{KeyValue, VariableSet};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};
use std::rc::Rc;
use ui::{
    ActiveTheme as _, ButtonCommon as _, Clickable as _, IconButton, IconName, IconSize, Tooltip,
    VisibleOnHover as _, h_flex, v_flex,
};

/// Applies one row edit to the latest state.
pub type RowEditor = Rc<dyn Fn(&mut FerrumApi, usize, RowEdit, &mut Context<FerrumApi>)>;

impl FerrumApi {
    #[allow(clippy::too_many_arguments)]
    pub fn render_kv_table(
        &mut self,
        scope: String,
        rows: Vec<KeyValue>,
        vars: &VariableSet,
        key_placeholder: &'static str,
        on_edit: RowEditor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let border = row_border(cx);
        let n = rows.len();
        let mut display = rows;
        display.push(KeyValue::default());

        let mut body = Vec::new();
        for (i, row) in display.into_iter().enumerate() {
            let blank = i == n;
            let key_bad = vars.unresolved(&row.key);
            let val_bad = vars.unresolved(&row.value);
            let key_cell = self.kv_cell(
                format!("{scope}:{i}:key"),
                &row.key,
                if blank { key_placeholder } else { "" },
                &key_bad,
                {
                    let on_edit = on_edit.clone();
                    move |this, text, cx| on_edit(this, i, RowEdit::Key(text), cx)
                },
                window,
                cx,
            );
            let value_cell = self.kv_cell(
                format!("{scope}:{i}:value"),
                &row.value,
                if blank { "Value" } else { "" },
                &val_bad,
                {
                    let on_edit = on_edit.clone();
                    move |this, text, cx| on_edit(this, i, RowEdit::Value(text), cx)
                },
                window,
                cx,
            );
            let group: SharedString = format!("{scope}-row-{i}").into();
            body.push(
                h_flex()
                    .group(group.clone())
                    .border_b_1()
                    .border_color(border)
                    .when(!row.enabled && !blank, |this| this.opacity(0.5))
                    .child(div().w_8().flex().justify_center().when(!blank, |this| {
                        let on_edit = on_edit.clone();
                        this.child(checkbox(
                            SharedString::from(format!("{scope}-enabled-{i}")),
                            row.enabled,
                            move |this, on, cx| on_edit(this, i, RowEdit::Enabled(on), cx),
                            cx,
                        ))
                    }))
                    .child(div().flex_1().min_w_0().child(key_cell))
                    .child(div().flex_1().min_w_0().child(value_cell))
                    .child(div().w_8().flex().justify_center().when(!blank, |this| {
                        let on_edit = on_edit.clone();
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
                                        on_edit(this, i, RowEdit::Remove, cx);
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
                    .child(div().flex_1().child(header_cell("Value")))
                    .child(div().w_8()),
            )
            .children(body)
            .into_any_element()
    }

    /// Borderless mono cell; unresolved `{{variables}}` get an error underline.
    #[allow(clippy::too_many_arguments)]
    pub fn kv_cell(
        &mut self,
        key: String,
        value: &str,
        placeholder: &'static str,
        unresolved: &[String],
        apply: impl Fn(&mut FerrumApi, String, &mut Context<FerrumApi>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let error = cx.theme().status().error;
        let title = unresolved_title(unresolved);
        let input = self.field(
            FieldSpec::new(key.clone())
                .value(value)
                .placeholder(placeholder)
                .mono()
                .borderless(),
            on_request_edit(apply),
            window,
            cx,
        );
        div()
            .id(SharedString::from(format!("{key}-cell")))
            .h(px(28.))
            .px_2()
            .flex()
            .items_center()
            .when(!title.is_empty(), |this| {
                this.border_b_1()
                    .border_color(error)
                    .tooltip(Tooltip::text(title))
            })
            .child(input)
            .into_any_element()
    }
}
