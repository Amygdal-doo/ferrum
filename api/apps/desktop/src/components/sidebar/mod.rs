//! Left sidebar: Collections | Environments | History.

mod environments;
mod groups;
mod history;
mod menu;
mod tree;

use crate::app::FerrumApi;
use crate::components::fields::{FieldSpec, on_change};
use crate::state::SidebarView;
use ferrum_ui::components::{
    Button, ButtonCommon as _, ButtonStyle, Clickable as _, Color, IconButton, IconName, IconSize,
    Label, LabelCommon as _, LabelSize, Tooltip, h_flex, v_flex,
};
use ferrum_ui::empty_state;
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*};

/// Section header: a quiet label and trailing actions.
fn section_header(title: &'static str, actions: Vec<AnyElement>) -> impl IntoElement {
    h_flex()
        .flex_none()
        .h_8()
        .px_3p5()
        .gap_1()
        .child(
            div()
                .flex_1()
                .child(Label::new(title).size(LabelSize::Small).color(Color::Muted)),
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
    /// The sidebar, on the window backdrop: title row (traffic lights,
    /// ◧ ⌘ +), the project, the Collections / Environments / History
    /// switcher, then the chosen list.
    pub fn render_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
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
            .child(
                ferrum_ui::title_row("sidebar-title", true, window)
                    .child(div().flex_1())
                    .child(self.render_title_controls(cx)),
            )
            .child(div().px_1().child(self.project_bar.clone()))
            .child(
                h_flex()
                    .flex_none()
                    .h_9()
                    .gap_1()
                    .px_2()
                    .text_size(gpui::px(13.))
                    .children(views.into_iter().map(|(view, label)| {
                        ferrum_ui::header_tab(SharedString::from(label), view == current, cx)
                            .child(label)
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
        let groups = groups::group_collections(list, self.state.project.as_deref());

        let header = section_header(
            "Collections",
            vec![
                icon_action(
                    "webhook-inspector",
                    IconName::Server,
                    "Webhook inspector",
                    cx,
                    |this, _| this.state.dialog = Some(crate::state::Dialog::Webhooks),
                ),
                icon_action(
                    "import-collection",
                    IconName::Download,
                    "Import a Postman, Insomnia or OpenAPI file",
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
        for group in &groups {
            if let Some(header) = self.render_group_header(group, cx) {
                rows.push(header);
            }
            for c in &group.collections {
                self.render_collection_node(c, &mut rows, window, cx);
            }
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
