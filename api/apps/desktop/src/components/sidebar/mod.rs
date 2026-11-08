//! Left sidebar: Collections | Environments | History.

mod environments;
mod history;
mod menu;
mod tree;

use crate::app::FerrumApi;
use crate::components::fields::{FieldSpec, on_change};
use crate::state::SidebarView;
use ferrum_ui::empty_state;
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*};
use ui::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonStyle, Clickable as _, Color, IconButton,
    IconName, IconSize, Label, LabelCommon as _, LabelSize, Toggleable as _, Tooltip, h_flex,
    v_flex,
};

/// Section header with a title and trailing actions.
fn section_header(title: &'static str, actions: Vec<AnyElement>) -> impl IntoElement {
    h_flex()
        .flex_none()
        .h_9()
        .px_3()
        .gap_1()
        .child(
            div().flex_1().child(
                Label::new(title.to_uppercase())
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            ),
        )
        .children(actions)
}

fn icon_action(
    id: &'static str,
    icon: IconName,
    tooltip: &'static str,
    cx: &mut Context<FerrumApi>,
    on_click: impl Fn(&mut FerrumApi, &mut Context<FerrumApi>) + 'static,
) -> AnyElement {
    IconButton::new(id, icon)
        .icon_size(IconSize::Small)
        .tooltip(Tooltip::text(tooltip))
        .on_click(cx.listener(move |this, _, _, cx| {
            on_click(this, cx);
            cx.notify();
        }))
        .into_any_element()
}

impl FerrumApi {
    pub fn render_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let current = self.state.sidebar_view;
        let views = [
            (SidebarView::Collections, "Collections"),
            (SidebarView::Environments, "Environments"),
            (SidebarView::History, "History"),
        ];
        let body = match current {
            SidebarView::Collections => self.render_collections_view(window, cx),
            SidebarView::Environments => self.render_environments_view(window, cx),
            SidebarView::History => self.render_history_view(cx),
        };
        v_flex()
            .size_full()
            .bg(colors.panel_background)
            .child(
                h_flex()
                    .flex_none()
                    .h_10()
                    .gap_1()
                    .px_2()
                    .border_b_1()
                    .border_color(colors.border)
                    .children(views.into_iter().map(|(view, label)| {
                        Button::new(SharedString::from(label), label)
                            .style(ButtonStyle::Subtle)
                            .toggle_state(view == current)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.state.sidebar_view = view;
                                cx.notify();
                            }))
                    })),
            )
            .child(v_flex().flex_1().min_h_0().child(body))
            .into_any_element()
    }

    fn render_collections_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let needle = self.state.collection_filter.to_lowercase();
        let empty = self.state.collections.is_empty();
        let list: Vec<_> = self
            .state
            .collections
            .iter()
            .filter(|c| {
                needle.is_empty()
                    || c.name.to_lowercase().contains(&needle)
                    || c.requests()
                        .iter()
                        .any(|r| r.name.to_lowercase().contains(&needle))
            })
            .cloned()
            .collect();

        let header = section_header(
            "Collections",
            vec![
                icon_action(
                    "import-collection",
                    IconName::Download,
                    "Import Postman collection",
                    cx,
                    |this, cx| this.import_collection(cx),
                ),
                icon_action(
                    "new-collection",
                    IconName::Plus,
                    "New collection",
                    cx,
                    |this, cx| {
                        let id = this.create_collection("New Collection", cx);
                        this.state.renaming = Some(id.0);
                    },
                ),
            ],
        );
        let filter = (!empty).then(|| {
            let value = self.state.collection_filter.clone();
            let input = self.field(
                FieldSpec::new("collection-filter")
                    .value(value)
                    .placeholder("Filter collections and requests"),
                on_change(|this, text, _| this.state.collection_filter = text),
                window,
                cx,
            );
            div().px_3().pb_2().child(input)
        });

        let mut rows = Vec::new();
        for c in &list {
            self.render_collection_node(c, &mut rows, window, cx);
        }

        v_flex()
            .size_full()
            .child(header)
            .children(filter)
            .child(
                v_flex()
                    .id("collections-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_2()
                    .pb_2()
                    .when(empty, |this| {
                        this.child(
                            v_flex()
                                .py_6()
                                .gap_3()
                                .items_center()
                                .child(empty_state("No collections yet.", ""))
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .child(
                                            Button::new("empty-new-collection", "New collection")
                                                .style(ButtonStyle::Outlined)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    let id = this
                                                        .create_collection("New Collection", cx);
                                                    this.state.renaming = Some(id.0);
                                                    cx.notify();
                                                })),
                                        )
                                        .child(
                                            Button::new("empty-import", "Import…")
                                                .style(ButtonStyle::Subtle)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.import_collection(cx);
                                                })),
                                        ),
                                ),
                        )
                    })
                    .children(rows),
            )
            .into_any_element()
    }
}
