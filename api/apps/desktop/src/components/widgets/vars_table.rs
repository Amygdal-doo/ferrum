//! Variable table (environments, globals, collection variables): enabled,
//! key, value (masked when secret) and a secret toggle.

use super::{RowEditor, header_cell, row_border};
use crate::app::FerrumApi;
use crate::components::checkbox;
use crate::components::fields::{FieldSpec, on_change};
use crate::state::kv::RowEdit;
use ferrumapi_domain::Variable;
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};
use ui::{
    ButtonCommon as _, Clickable as _, IconButton, IconName, IconSize, Tooltip,
    VisibleOnHover as _, h_flex, v_flex,
};

impl FerrumApi {
    pub fn render_vars_table(
        &mut self,
        scope: String,
        rows: Vec<Variable>,
        on_edit: RowEditor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let border = row_border(cx);
        let n = rows.len();
        let mut display = rows;
        display.push(Variable::default());

        let mut body = Vec::new();
        for (i, row) in display.into_iter().enumerate() {
            let blank = i == n;
            let value_key = format!("{scope}:{i}:value");
            let shown = self.state.revealed.contains(&value_key);
            let key_input = self.field(
                FieldSpec::new(format!("{scope}:{i}:key"))
                    .value(row.key.clone())
                    .placeholder(if blank { "Add variable" } else { "" })
                    .mono()
                    .borderless(),
                on_change({
                    let on_edit = on_edit.clone();
                    move |this, text, cx| on_edit(this, i, RowEdit::Key(text), cx)
                }),
                window,
                cx,
            );
            let value_input = self.field(
                FieldSpec::new(value_key.clone())
                    .value(row.value.clone())
                    .masked(row.secret && !shown)
                    .mono()
                    .borderless(),
                on_change({
                    let on_edit = on_edit.clone();
                    move |this, text, cx| on_edit(this, i, RowEdit::Value(text), cx)
                }),
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
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h(px(28.))
                            .px_2()
                            .flex()
                            .items_center()
                            .child(key_input),
                    )
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .h(px(28.))
                            .px_2()
                            .child(value_input)
                            .when(row.secret, |this| {
                                let value_key = value_key.clone();
                                this.child(
                                    IconButton::new(
                                        SharedString::from(format!("{scope}-reveal-{i}")),
                                        if shown {
                                            IconName::EyeOff
                                        } else {
                                            IconName::Eye
                                        },
                                    )
                                    .icon_size(IconSize::XSmall)
                                    .tooltip(Tooltip::text(if shown {
                                        "Hide value"
                                    } else {
                                        "Show value"
                                    }))
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            if !this.state.revealed.remove(&value_key) {
                                                this.state.revealed.insert(value_key.clone());
                                            }
                                            cx.notify();
                                        },
                                    )),
                                )
                            }),
                    )
                    .child(div().w_16().flex().justify_center().when(!blank, |this| {
                        let on_edit = on_edit.clone();
                        this.child(
                            checkbox(
                                SharedString::from(format!("{scope}-secret-{i}")),
                                row.secret,
                                move |this, on, cx| on_edit(this, i, RowEdit::Secret(on), cx),
                                cx,
                            )
                            .tooltip(Tooltip::text("Mask this value")),
                        )
                    }))
                    .child(div().w_8().flex().justify_center().when(!blank, |this| {
                        let on_edit = on_edit.clone();
                        this.child(
                            div().visible_on_hover(group).child(
                                IconButton::new(
                                    SharedString::from(format!("{scope}-delete-{i}")),
                                    IconName::Close,
                                )
                                .icon_size(IconSize::XSmall)
                                .tooltip(Tooltip::text("Delete variable"))
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
                    .child(div().flex_1().child(header_cell("Variable")))
                    .child(div().flex_1().child(header_cell("Value")))
                    .child(div().w_16().child(header_cell("Secret")))
                    .child(div().w_8()),
            )
            .children(body)
            .into_any_element()
    }
}
