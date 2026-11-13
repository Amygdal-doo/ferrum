//! Collection tree: collection, folder and request rows.

use super::menu::{MenuAction, more_button, with_context_menu};
use crate::app::FerrumApi;
use crate::components::{MenuHandler, menu_item, request_badge};
use crate::state::Dialog;
use ferrum_ui::components::{
    ActiveTheme as _, Color, Icon, IconName, IconSize, Label, LabelCommon as _, LabelSize, Tooltip,
    VisibleOnHover as _, h_flex,
};
use ferrumapi_domain::{
    Collection, CollectionId, HttpMethod, Item, ItemId, RequestKind, RequestOrigin,
};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};
use uuid::Uuid;

const CONTAINER_ACTIONS: [MenuAction; 7] = [
    MenuAction::Run,
    MenuAction::AddRequest,
    MenuAction::AddFolder,
    MenuAction::Rename,
    MenuAction::Duplicate,
    MenuAction::Export,
    MenuAction::Delete,
];

struct RowSpec {
    uid: Uuid,
    cid: CollectionId,
    item: Option<ItemId>,
    depth: usize,
    name: String,
    folder: bool,
    expanded: bool,
    badge: Option<(RequestKind, HttpMethod)>,
    actions: Vec<MenuAction>,
    active: bool,
}

impl FerrumApi {
    pub fn render_collection_node(
        &mut self,
        collection: &Collection,
        rows: &mut Vec<AnyElement>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cid = collection.id;
        let expanded = self.state.expanded.contains(&cid.0);
        let actions = super::menu::collection_actions(
            &CONTAINER_ACTIONS,
            self.state.project.as_deref(),
            collection.project.as_deref(),
        );
        let row = self.render_tree_row(
            RowSpec {
                uid: cid.0,
                cid,
                item: None,
                depth: 0,
                name: collection.name.clone(),
                folder: true,
                expanded,
                badge: None,
                actions,
                active: false,
            },
            window,
            cx,
        );
        rows.push(row);
        if expanded {
            for item in &collection.items {
                self.render_item_node(cid, item, 1, rows, window, cx);
            }
            if collection.items.is_empty() {
                rows.push(
                    div()
                        .py_1()
                        .pl_8()
                        .child(
                            Label::new("Empty collection")
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        )
                        .into_any_element(),
                );
            }
        }
    }

    fn render_item_node(
        &mut self,
        cid: CollectionId,
        item: &Item,
        depth: usize,
        rows: &mut Vec<AnyElement>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = item.id();
        match item {
            Item::Folder(f) => {
                let expanded = self.state.expanded.contains(&id.0);
                let mut actions = CONTAINER_ACTIONS.to_vec();
                actions.retain(|a| *a != MenuAction::Export);
                let row = self.render_tree_row(
                    RowSpec {
                        uid: id.0,
                        cid,
                        item: Some(id),
                        depth,
                        name: f.name.clone(),
                        folder: true,
                        expanded,
                        badge: None,
                        actions,
                        active: false,
                    },
                    window,
                    cx,
                );
                rows.push(row);
                if expanded {
                    for child in &f.items {
                        self.render_item_node(cid, child, depth + 1, rows, window, cx);
                    }
                }
            }
            Item::Request(r) => {
                let origin = RequestOrigin {
                    collection: cid,
                    item: id,
                };
                let active = self.state.active_tab().and_then(|t| t.origin) == Some(origin);
                let row = self.render_tree_row(
                    RowSpec {
                        uid: id.0,
                        cid,
                        item: Some(id),
                        depth,
                        name: r.name.clone(),
                        folder: false,
                        expanded: false,
                        badge: Some((r.request.kind, r.request.method)),
                        actions: vec![
                            MenuAction::Open,
                            MenuAction::Rename,
                            MenuAction::Duplicate,
                            MenuAction::Delete,
                        ],
                        active,
                    },
                    window,
                    cx,
                );
                rows.push(row);
            }
        }
    }

    /// Folders toggle; requests open in a tab.
    fn open_tree_row(&mut self, uid: Uuid, cid: CollectionId, item: Option<ItemId>, folder: bool) {
        if folder {
            if !self.state.expanded.remove(&uid) {
                self.state.expanded.insert(uid);
            }
        } else if let Some(id) = item
            && let Some(saved) = self
                .state
                .collection(cid)
                .and_then(|c| c.find_request(id).cloned())
        {
            let origin = RequestOrigin {
                collection: cid,
                item: id,
            };
            self.open_request(saved.name, saved.request, Some(origin));
        }
    }

    fn render_tree_row(
        &mut self,
        spec: RowSpec,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let RowSpec {
            uid,
            cid,
            item,
            depth,
            name,
            folder,
            expanded,
            badge,
            actions,
            active,
        } = spec;
        let renaming = self.state.renaming == Some(uid);
        let entries: Vec<(SharedString, MenuHandler)> = actions
            .into_iter()
            .map(|action| {
                let name = name.clone();
                menu_item(
                    action.label(),
                    move |this: &mut FerrumApi, _, cx| match action {
                        MenuAction::Open => this.open_tree_row(uid, cid, item, folder),
                        MenuAction::Run => this.start_run(cid, item, cx),
                        MenuAction::MockServer => this.open_mock_server(cid, cx),
                        MenuAction::AddRequest => {
                            this.state.dialog = Some(Dialog::NewRequest {
                                target: (cid, item),
                            });
                        }
                        MenuAction::AddFolder => this.add_folder(cid, item, cx),
                        MenuAction::Settings => this.open_collection_settings(cid),
                        MenuAction::Rename => this.state.renaming = Some(uid),
                        MenuAction::Duplicate => this.duplicate_item(cid, item, cx),
                        MenuAction::Export => {
                            if let Some(c) = this.state.collection(cid).cloned() {
                                this.export_collection(c, cx);
                            }
                        }
                        MenuAction::MoveToProject => {
                            let project = this.state.project.clone();
                            this.move_collection(cid, project, cx);
                        }
                        MenuAction::MakeShared => this.move_collection(cid, None, cx),
                        MenuAction::Delete => {
                            this.state.dialog = Some(Dialog::DeleteItem {
                                collection: cid,
                                item,
                                name: name.clone(),
                            });
                        }
                    },
                )
            })
            .collect();

        let group: SharedString = format!("tree-row-{uid}").into();
        let lead = if folder {
            Icon::new(if expanded {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            })
            .size(IconSize::XSmall)
            .color(Color::Muted)
            .into_any_element()
        } else if let Some((kind, m)) = badge {
            div()
                .w(px(40.))
                .flex()
                .justify_end()
                .child(request_badge(kind, m, cx))
                .into_any_element()
        } else {
            div().into_any_element()
        };
        let body = if renaming {
            let input = self.rename_field(
                uid,
                &name,
                move |this, new_name, cx| this.rename_item(cid, item, new_name, cx),
                window,
                cx,
            );
            div().flex_1().min_w_0().child(input).into_any_element()
        } else {
            h_flex()
                .flex_1()
                .min_w_0()
                .gap_1()
                .child(
                    div()
                        .id(SharedString::from(format!("tree-name-{uid}")))
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .when(depth == 0, |this| {
                            this.font_weight(gpui::FontWeight::MEDIUM)
                        })
                        .tooltip(Tooltip::text(name.clone()))
                        .child(name.clone()),
                )
                .child(div().visible_on_hover(group.clone()).child(more_button(
                    format!("tree-more-{uid}"),
                    entries.clone(),
                    cx,
                )))
                .into_any_element()
        };

        let row = h_flex()
            .id(SharedString::from(format!("tree-row-{uid}")))
            .group(group)
            .h_7()
            .gap_1p5()
            .pr_1()
            .pl(px((8 + depth * 14) as f32))
            .rounded_md()
            .cursor_default()
            .map(|this| {
                if active {
                    this.bg(colors.element_selected)
                } else {
                    this.hover(|s| s.bg(colors.element_hover))
                }
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.state.renaming == Some(uid) {
                    return;
                }
                this.open_tree_row(uid, cid, item, folder);
                cx.notify();
            }))
            .child(lead)
            .child(body)
            .into_any_element();
        with_context_menu(format!("tree-menu-{uid}"), row, entries, cx)
    }
}
