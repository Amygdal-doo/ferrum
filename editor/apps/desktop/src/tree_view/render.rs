//! Rendering of the tree view: header, entry list and context menu.

use ferrum_ui::prelude::*;
use gpui::{MouseButton, MouseDownEvent, anchored, deferred, point};
use ui::{Icon, IconButton, IconName, IconSize};

use super::{ContextMenuState, TreeEvent, TreeView};
use crate::actions::RunCommand;

const REVEAL_LABEL: &str = if cfg!(target_os = "macos") {
    "Reveal in Finder"
} else {
    "Reveal in File Manager"
};

impl TreeView {
    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let name = self.root().file_name().map_or_else(
            || self.root().display().to_string(),
            |n| n.to_string_lossy().to_string(),
        );
        h_flex()
            .flex_none()
            .h(px(30.))
            .pl_3()
            .pr_1()
            .justify_between()
            .child(
                Label::new(name.to_uppercase())
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
                    .truncate(),
            )
            .child(
                h_flex()
                    .gap_0p5()
                    .child(
                        IconButton::new("tree-new-file", IconName::Plus)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("New File"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.emit(TreeEvent::NewFile {
                                    dir: this.target_dir(),
                                });
                            })),
                    )
                    .child(
                        IconButton::new("tree-new-folder", IconName::Folder)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("New Folder"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.emit(TreeEvent::NewFolder {
                                    dir: this.target_dir(),
                                });
                            })),
                    )
                    .child(
                        IconButton::new("tree-collapse-all", IconName::ChevronDown)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Collapse All"))
                            .on_click(cx.listener(|this, _, _, cx| this.collapse_all(cx))),
                    ),
            )
    }

    fn render_rows(
        &self,
        range: std::ops::Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let accent =
            crate::theme_apply::ui(&cx.global::<crate::settings::Settings>().theme, "accent");
        range
            .filter_map(|index| self.entries.get(index).cloned().map(|e| (index, e)))
            .map(|(index, entry)| {
                let selected = self.selected.as_ref() == Some(&entry.path);
                let active = self.active_file.as_ref() == Some(&entry.path);
                let icon = crate::file_icons::icon_element(
                    &entry.path,
                    crate::file_icons::file_kind(entry.is_dir, entry.expanded),
                    cx,
                );
                let chevron: AnyElement = if entry.is_dir {
                    Icon::new(if entry.expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .size(IconSize::XSmall)
                    .color(Color::Muted)
                    .into_any_element()
                } else {
                    div().w(px(12.)).into_any_element()
                };
                let click_path = entry.path.clone();
                let menu_path = entry.path.clone();
                ListItem::new(("tree-entry", index))
                    .toggle_state(selected)
                    .indent_level(entry.depth)
                    .indent_step_size(px(12.))
                    .child(
                        h_flex()
                            .gap_1()
                            .child(chevron)
                            .when_some(icon, |this, icon| this.child(icon))
                            .child(
                                div().when(active, |this| this.text_color(accent)).child(
                                    Label::new(entry.name.clone())
                                        .size(LabelSize::Small)
                                        .when(active, |l| l.color(Color::Accent)),
                                ),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.focus(window, cx);
                        this.activate(click_path.clone(), cx);
                    }))
                    .on_secondary_mouse_down(cx.listener(
                        move |this, event: &MouseDownEvent, _, cx| {
                            this.selected = Some(menu_path.clone());
                            this.menu = Some(ContextMenuState {
                                path: menu_path.clone(),
                                position: event.position,
                            });
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ))
                    .into_any_element()
            })
            .collect()
    }

    fn render_menu(&self, menu: &ContextMenuState, cx: &mut Context<Self>) -> impl IntoElement {
        let (border, surface) = {
            let colors = cx.theme().colors();
            (colors.border, colors.elevated_surface_background)
        };
        let path = menu.path.clone();
        let dir = if path.is_dir() {
            path.clone()
        } else {
            path.parent()
                .map_or_else(|| self.root().to_path_buf(), |p| p.to_path_buf())
        };
        let entry =
            |id: &'static str, label: &'static str, event: TreeEvent, cx: &mut Context<Self>| {
                ListItem::new(id)
                    .child(Label::new(label).size(LabelSize::Small))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.menu = None;
                        cx.emit(event.clone());
                        cx.notify();
                    }))
            };
        let items = [
            entry(
                "tree-menu-new-file",
                "New File",
                TreeEvent::NewFile { dir: dir.clone() },
                cx,
            ),
            entry(
                "tree-menu-new-folder",
                "New Folder",
                TreeEvent::NewFolder { dir },
                cx,
            ),
            entry(
                "tree-menu-rename",
                "Rename",
                TreeEvent::Rename(path.clone()),
                cx,
            ),
            entry(
                "tree-menu-delete",
                "Move to Trash",
                TreeEvent::Delete(path.clone()),
                cx,
            ),
            entry(
                "tree-menu-reveal",
                REVEAL_LABEL,
                TreeEvent::RevealInFinder(path),
                cx,
            ),
        ];
        deferred(
            anchored()
                .position(point(menu.position.x, menu.position.y))
                .snap_to_window()
                .child(
                    v_flex()
                        .id("tree-context-menu")
                        .occlude()
                        .min_w(px(180.))
                        .p_1()
                        .rounded_md()
                        .border_1()
                        .border_color(border)
                        .bg(surface)
                        .shadow_lg()
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                            this.menu = None;
                            cx.notify();
                        }))
                        .children(items),
                ),
        )
        .with_priority(1)
    }
}

impl Render for TreeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = cx.theme().colors().panel_background;
        let count = self.entries.len();
        let list = uniform_list(
            "tree-entries",
            count,
            cx.processor(|this, range: std::ops::Range<usize>, _window, cx| {
                this.render_rows(range, cx)
            }),
        )
        .track_scroll(&self.scroll)
        .flex_1()
        .min_h_0()
        .px_1();
        let menu = self.menu.clone();
        v_flex()
            .key_context("FerrumTreeView")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, action: &RunCommand, window, cx| {
                if !this.run_command(&action.name, window, cx) {
                    cx.propagate();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.focus(window, cx);
                }),
            )
            .size_full()
            .bg(panel)
            .child(self.render_header(cx))
            .when(count == 0, |this| {
                this.child(empty_state("Empty folder", ""))
            })
            .child(list)
            .when_some(menu, |this, menu| this.child(self.render_menu(&menu, cx)))
    }
}
