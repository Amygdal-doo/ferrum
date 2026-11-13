//! "Save to collection": choose a collection or folder and a name.

use super::{actions, modal};
use crate::app::FerrumApi;
use crate::components::fields::{FieldSpec, on_change};
use crate::components::hint;
use crate::state::Dialog;
use ferrum_ui::components::{ActiveTheme as _, v_flex};
use ferrumapi_domain::{Collection, CollectionId, Item, ItemId, TabId};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};

/// One choosable destination: (collection, folder, indented label).
type Target = (CollectionId, Option<ItemId>, String);

fn targets(collections: &[Collection]) -> Vec<Target> {
    fn walk(c: CollectionId, items: &[Item], depth: usize, out: &mut Vec<Target>) {
        for it in items {
            if let Item::Folder(f) = it {
                out.push((c, Some(f.id), format!("{}▸ {}", "  ".repeat(depth), f.name)));
                walk(c, &f.items, depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    for c in collections {
        out.push((c.id, None, c.name.clone()));
        walk(c.id, &c.items, 1, &mut out);
    }
    out
}

impl FerrumApi {
    pub(super) fn render_save_request(
        &mut self,
        tab_id: TabId,
        name: String,
        new_collection: String,
        target: Option<(CollectionId, Option<ItemId>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let all = targets(&self.state.collections);
        let none = all.is_empty();
        let name_input = self.field(
            FieldSpec::new(format!("save-name:{tab_id}"))
                .value(name.clone())
                .autofocus(),
            on_change(|this, text, _| {
                if let Some(Dialog::SaveRequest { name, .. }) = &mut this.state.dialog {
                    *name = text;
                }
            }),
            window,
            cx,
        );
        let destination = if none {
            let input = self.field(
                FieldSpec::new(format!("save-new-collection:{tab_id}")).value(new_collection),
                on_change(|this, text, _| {
                    if let Some(Dialog::SaveRequest { new_collection, .. }) = &mut this.state.dialog
                    {
                        *new_collection = text;
                    }
                }),
                window,
                cx,
            );
            v_flex()
                .gap_1()
                .child(hint("No collections yet. Create one named:"))
                .child(input)
                .into_any_element()
        } else {
            v_flex()
                .gap_1()
                .child(hint("Save to"))
                .child(
                    v_flex()
                        .id("save-targets")
                        .max_h(px(256.))
                        .overflow_y_scroll()
                        .rounded_md()
                        .border_1()
                        .border_color(colors.border)
                        .p_1()
                        .children(all.into_iter().enumerate().map(|(i, (c, folder, label))| {
                            let selected = target == Some((c, folder));
                            div()
                                .id(SharedString::from(format!("save-target-{i}")))
                                .px_2()
                                .py_1()
                                .rounded_sm()
                                .text_sm()
                                .cursor_pointer()
                                .map(|this| {
                                    if selected {
                                        this.bg(colors.element_selected)
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                    } else {
                                        this.hover(|s| s.bg(colors.element_hover))
                                    }
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(Dialog::SaveRequest { target, .. }) =
                                        &mut this.state.dialog
                                    {
                                        *target = Some((c, folder));
                                    }
                                    cx.notify();
                                }))
                                .child(label)
                        })),
                )
                .into_any_element()
        };

        actions(
            modal("Save request", px(576.), cx)
                .child(
                    v_flex()
                        .gap_1()
                        .child(hint("Request name"))
                        .child(name_input),
                )
                .child(destination),
            "Save",
            false,
            name.trim().is_empty(),
            move |this, cx| {
                let Some(Dialog::SaveRequest {
                    name,
                    new_collection,
                    target,
                    ..
                }) = this.state.dialog.clone()
                else {
                    return;
                };
                let name = name.trim().to_owned();
                if name.is_empty() {
                    return;
                }
                let destination = if this.state.collections.is_empty() {
                    Some((this.create_collection(new_collection.trim(), cx), None))
                } else {
                    target
                };
                if let Some((c, parent)) = destination {
                    this.state.dialog = None;
                    this.save_tab_as(tab_id, c, parent, name, cx);
                }
            },
            cx,
        )
        .into_any_element()
    }
}
