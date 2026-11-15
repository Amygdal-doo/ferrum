//! Folders in the sidebar's terminal list (see `crate::folders`): a folder
//! row (chevron, name, count; right-click for New terminal here, Rename,
//! Set directory, Delete) with its terminals under it, then the terminals
//! in no folder. Terminal rows can be dragged onto a folder (or onto the
//! list's empty space to take them out), and their right-click menu has
//! "Move to folder".

use super::Prompt;
use crate::view::TermView;
use ferrum_ui::components::{
    Color, ContextMenu, Icon, IconName, IconSize, Label, LabelCommon, LabelSize, Tooltip, h_flex,
    right_click_menu,
};
use ferrum_ui::gpui::{
    self, AnyElement, Context, Render, SharedString, Window, div, prelude::*, px,
};
use ferrum_ui::theme::ActiveTheme;

/// A terminal row being dragged: its id (`w3` / `s2`) and title.
#[derive(Debug, Clone)]
pub(crate) struct DraggedTerminal {
    pub id: String,
    pub title: String,
}

impl Render for DraggedTerminal {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        h_flex()
            .px_2()
            .py_1()
            .gap_1()
            .rounded_md()
            .bg(colors.elevated_surface_background)
            .border_1()
            .border_color(colors.border)
            .shadow_md()
            .text_size(px(12.))
            .child(Icon::new(IconName::Terminal).size(IconSize::XSmall))
            .child(self.title.clone())
    }
}

/// What a folder menu entry does to the view.
type Act = Box<dyn Fn(&mut TermView, &mut Window, &mut Context<TermView>)>;

impl TermView {
    /// `row` made draggable into folders, with a "Move to folder" menu.
    pub(crate) fn folder_item(
        &self,
        id: String,
        title: String,
        row: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.app.folders_enabled() {
            return row;
        }
        let weak = cx.weak_entity();
        let folders: Vec<(u64, String)> = self
            .app
            .folders
            .folders
            .iter()
            .map(|f| (f.id, f.name.clone()))
            .collect();
        let current = self.app.folders.folder_of(&id);
        let drag = DraggedTerminal {
            id: id.clone(),
            title,
        };
        let row = div()
            .id(SharedString::from(format!("drag-{id}")))
            .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
            .child(row);
        right_click_menu(SharedString::from(format!("terminal-menu-{id}")))
            .trigger(move |_, _, _| row)
            .menu(move |window, cx| {
                let (weak, folders, id) = (weak.clone(), folders.clone(), id.clone());
                ContextMenu::build(window, cx, move |mut menu, _, _| {
                    let on = |f: Act| {
                        let weak = weak.clone();
                        move |window: &mut Window, cx: &mut gpui::App| {
                            if let Err(error) = weak.update(cx, |view, cx| {
                                f(view, window, cx);
                                view.after_change(cx);
                            }) {
                                tracing::debug!(%error, "terminal view closed before the menu ran");
                            }
                        }
                    };
                    menu = menu.header("Move to folder");
                    for (folder, name) in &folders {
                        let (folder, item) = (*folder, id.clone());
                        menu = if current == Some(folder) {
                            menu.disabled_entry(format!("{name} ✓"))
                        } else {
                            menu.icon_entry(
                                name.clone(),
                                IconName::Folder,
                                on(Box::new(move |view, _, _| {
                                    view.move_to_folder(&item, Some(folder))
                                })),
                            )
                        };
                    }
                    let item = id.clone();
                    menu = menu.icon_entry(
                        "New folder…",
                        IconName::Plus,
                        on(Box::new(move |view, window, cx| {
                            view.pending_folder_item = Some(item.clone());
                            view.begin_prompt(Prompt::NewFolder, window, cx);
                        })),
                    );
                    if current.is_some() {
                        let item = id.clone();
                        menu = menu.separator().entry(
                            "Remove from folder",
                            None,
                            on(Box::new(move |view, _, _| view.move_to_folder(&item, None))),
                        );
                    }
                    menu
                })
            })
            .into_any_element()
    }

    pub(crate) fn move_to_folder(&mut self, item: &str, folder: Option<u64>) {
        self.app.folders.move_item(item, folder);
        self.app.folders.save();
    }

    /// A folder row and, unless collapsed, `rows` (its terminals) under it.
    pub(crate) fn render_folder(
        &self,
        folder: &crate::folders::Folder,
        rows: Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let (id, collapsed) = (folder.id, folder.collapsed);
        let weak = cx.weak_entity();
        let has_path = folder.path.is_some();
        let count = rows.len();
        let header = h_flex()
            .id(SharedString::from(format!("folder-{id}")))
            .mx_1p5()
            .px_1()
            .py(px(3.))
            .gap_1()
            .rounded(px(8.))
            .cursor_pointer()
            .text_color(colors.text_muted)
            .hover(|s| s.bg(ferrum_ui::hover_fill(cx)))
            .drag_over::<DraggedTerminal>(|s, _, _, cx| {
                s.bg(cx.theme().colors().drop_target_background)
            })
            .on_drop(cx.listener(move |view, drag: &DraggedTerminal, _, cx| {
                cx.stop_propagation();
                view.move_to_folder(&drag.id, Some(id));
                view.after_change(cx);
            }))
            .when_some(folder.path.clone(), |row, path| {
                row.tooltip(Tooltip::text(format!("New terminals here start in {path}")))
            })
            .on_click(cx.listener(move |view, _, _, cx| {
                view.app.folders.toggle(id);
                view.app.folders.save();
                view.after_change(cx);
            }))
            .child(
                Icon::new(if collapsed {
                    IconName::ChevronRight
                } else {
                    IconName::ChevronDown
                })
                .size(IconSize::XSmall)
                .color(Color::Muted),
            )
            .child(
                Icon::new(if collapsed {
                    IconName::Folder
                } else {
                    IconName::FolderOpen
                })
                .size(IconSize::Small)
                .color(Color::Muted),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(colors.text)
                    .child(folder.name.clone()),
            )
            .child(
                Label::new(count.to_string())
                    .size(LabelSize::XSmall)
                    .color(Color::Placeholder),
            );
        let header = right_click_menu(SharedString::from(format!("folder-menu-{id}")))
            .trigger(move |_, _, _| header)
            .menu(move |window, cx| {
                let weak = weak.clone();
                ContextMenu::build(window, cx, move |menu, _, _| {
                    let on = |f: Act| {
                        let weak = weak.clone();
                        move |window: &mut Window, cx: &mut gpui::App| {
                            if let Err(error) = weak.update(cx, |view, cx| {
                                f(view, window, cx);
                                view.after_change(cx);
                            }) {
                                tracing::debug!(%error, "terminal view closed before the menu ran");
                            }
                        }
                    };
                    menu.icon_entry(
                        "New terminal in this folder",
                        IconName::Terminal,
                        on(Box::new(move |view, _, _| {
                            view.app.new_terminal_in_folder(id)
                        })),
                    )
                    .separator()
                    .icon_entry(
                        "Rename…",
                        IconName::Pencil,
                        on(Box::new(move |view, window, cx| {
                            view.begin_prompt(Prompt::RenameFolder(id), window, cx)
                        })),
                    )
                    .icon_entry(
                        if has_path {
                            "Change directory…"
                        } else {
                            "Set directory…"
                        },
                        IconName::FolderSearch,
                        on(Box::new(move |view, window, cx| {
                            view.begin_prompt(Prompt::FolderPath(id), window, cx)
                        })),
                    )
                    .separator()
                    .icon_entry(
                        "Delete folder",
                        IconName::Trash,
                        on(Box::new(move |view, _, _| {
                            // Its terminals go back to the top level and keep running.
                            view.app.folders.delete(id);
                            view.app.folders.save();
                        })),
                    )
                })
            });
        let mut column = gpui::div().flex().flex_col().gap_0p5().child(header);
        if !collapsed {
            for row in rows {
                column = column.child(div().pl_3().child(row));
            }
        }
        column
    }
}
