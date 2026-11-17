//! The cards: the Files pane, the chat (or the page open in its place),
//! then the Git, Term and Editor panes, with the gaps between them as
//! splitters. Panes have no header of ours: the title row's toggles show
//! and hide them (the Editor's own header moves and closes it). The chat
//! takes files dropped on it (from the Files pane or the Finder).

use ferrum_ui::components::Clickable;
use ferrum_ui::gpui::{Axis, ElementId, ExternalPaths, MouseButton, relative};
use ferrum_ui::prelude::*;

use crate::app::FerrumEngineer;
use crate::files::DraggedPaths;
use crate::layout::Column;
use crate::nav::Page;

/// Prefix of the splitters' drag ids; the pane on the left follows.
const SPLIT_PREFIX: &str = "vibe-split-";
/// Drag id of the splitter right of the Files pane.
const FILES_SPLITTER: &str = "vibe-split-files";

pub(crate) fn id(prefix: &str, column: Column) -> ElementId {
    ElementId::Name(format!("{prefix}-{}", column.key()).into())
}

impl FerrumEngineer {
    fn body(&mut self, column: Column, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if column == Column::Chat && self.page() != Page::Chat {
            return v_flex()
                .size_full()
                .child(self.page_header(window, cx))
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .child(self.render_page(self.page(), window, cx)),
                )
                .into_any_element();
        }
        let view = match column {
            Column::Chat => self.chat().map(|v| v.into_any_element()),
            Column::Git => self.git.clone().map(|v| v.into_any_element()),
            Column::Term => self.term.clone().map(|v| v.into_any_element()),
            Column::Editor => {
                let editor = self.ensure_editor(window, cx);
                let can_move = (
                    self.layout.can_move(Column::Editor, -1),
                    self.layout.can_move(Column::Editor, 1),
                );
                editor.update(cx, |editor, _| editor.can_move = can_move);
                Some(editor.into_any_element())
            }
        };
        if let Some(view) = view {
            return view;
        }
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .child(empty_state(
                "No project open",
                "Open a folder to work on it with an agent.",
            ))
            .child(
                Button::new(id("open-folder", column), "Open Folder…")
                    .style(ButtonStyle::Outlined)
                    .size(ButtonSize::Compact)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.project_bar
                            .update(cx, |bar, cx| bar.open_folder_dialog(window, cx));
                    })),
            )
            .into_any_element()
    }

    /// Whether the Files pane is drawn (shown, with a project open).
    pub(crate) fn files_shown(&self) -> bool {
        self.layout.files.visible && self.files.is_some()
    }

    /// Whether the chat's (or a page's) header holds the window controls:
    /// the sidebar is folded and no Files pane is left of it.
    pub(crate) fn controls_in_chat(&self) -> bool {
        !self.ui.sidebar_open && !self.files_shown()
    }

    /// The Files pane: a header row (with the window controls while the
    /// sidebar is folded) over the tree.
    fn files_card(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let files = self.files.clone().filter(|_| self.layout.files.visible)?;
        let lights = !self.ui.sidebar_open;
        let active = files.focus_handle(cx).contains_focused(window, cx);
        let header = ferrum_ui::title_row("files-header", lights, window)
            .gap_1()
            .when(!lights, |row| row.pl_3())
            .when(lights, |row| row.child(self.window_controls(cx)))
            .child(
                Label::new("Files")
                    .size(LabelSize::Small)
                    .weight(gpui::FontWeight::SEMIBOLD),
            )
            .child(div().flex_1())
            .child(
                ferrum_ui::title_button("files-refresh", IconName::RotateCw, "Refresh").on_click({
                    let files = files.clone();
                    move |_, _, cx| files.update(cx, |files, cx| files.refresh(cx))
                }),
            )
            .child(
                ferrum_ui::title_button("files-collapse", IconName::ChevronUp, "Collapse All")
                    .on_click({
                        let files = files.clone();
                        move |_, _, cx| files.update(cx, |files, cx| files.collapse_all(cx))
                    }),
            );
        Some(
            ferrum_ui::pane_card(active, cx)
                .id("files-pane")
                .debug_selector(|| "files-pane".into())
                .h_full()
                .flex_none()
                .w(px(self.layout.files.width))
                .overflow_hidden()
                .child(
                    v_flex()
                        .size_full()
                        .child(header)
                        .child(div().flex_1().min_h_0().child(files)),
                )
                .into_any_element(),
        )
    }

    fn card(
        &mut self,
        column: Column,
        fraction: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = column == self.active_column;
        ferrum_ui::pane_card(active, cx)
            .id(id("column", column))
            .debug_selector(|| format!("column-{}", column.key()))
            .h_full()
            .flex_basis(relative(fraction))
            .flex_shrink(1.)
            .overflow_hidden()
            // A click makes the pane the active one, also where its view
            // takes no focus.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    if this.active_column != column {
                        this.active_column = column;
                        cx.notify();
                    }
                }),
            )
            .when(
                column == Column::Chat && self.page() == Page::Chat,
                |card| {
                    let accent = cx.theme().colors().border_focused;
                    card.drag_over::<DraggedPaths>(move |style, _, _, _| style.border_color(accent))
                        .drag_over::<ExternalPaths>(move |style, _, _, _| {
                            style.border_color(accent)
                        })
                        .on_drop(cx.listener(|this, dragged: &DraggedPaths, window, cx| {
                            this.add_context_paths(&dragged.0, window, cx);
                        }))
                        .on_drop(cx.listener(|this, dropped: &ExternalPaths, window, cx| {
                            this.add_context_paths(dropped.paths(), window, cx);
                        }))
                },
            )
            .child(
                div()
                    .size_full()
                    .min_h_0()
                    .child(self.body(column, window, cx)),
            )
    }

    pub(crate) fn columns(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let fractions = self.layout.fractions();
        // Rounded cards on the window backdrop, with the gaps as splitters.
        let mut row = h_flex()
            .id("columns")
            .flex_1()
            .min_h_0()
            .w_full()
            .pr(ferrum_ui::PANE_GAP)
            .pb(ferrum_ui::PANE_GAP)
            .when(!self.ui.sidebar_open, |row| row.pl(ferrum_ui::PANE_GAP));
        if let Some(files) = self.files_card(window, cx) {
            row = row.child(files).child(
                div()
                    .h_full()
                    .flex_none()
                    .debug_selector(|| "splitter-files".into())
                    .child(ferrum_ui::gap_splitter(
                        FILES_SPLITTER,
                        Axis::Horizontal,
                        cx,
                    )),
            );
        }
        let count = fractions.len();
        for (i, (column, fraction)) in fractions.into_iter().enumerate() {
            row = row.child(self.card(column, fraction, window, cx));
            if i + 1 < count {
                row = row.child(
                    div()
                        .h_full()
                        .flex_none()
                        .debug_selector(move || format!("splitter-{}", column.key()))
                        .child(ferrum_ui::gap_splitter(
                            format!("{SPLIT_PREFIX}{}", column.key()),
                            Axis::Horizontal,
                            cx,
                        )),
                );
            }
        }
        row.on_drag_move(
            cx.listener(|this, event: &DragMoveEvent<DraggedSplitter>, _, cx| {
                if event.drag(cx).id.as_ref() == FILES_SPLITTER {
                    // The pane starts after the row's left padding (shown
                    // while the sidebar is folded); the splitter is the gap.
                    let pad = if this.ui.sidebar_open {
                        0.
                    } else {
                        f32::from(ferrum_ui::PANE_GAP)
                    };
                    let x = f32::from(event.event.position.x - event.bounds.left());
                    this.resize_files(x - pad - f32::from(ferrum_ui::PANE_GAP) / 2., cx);
                    return;
                }
                let Some(column) = event
                    .drag(cx)
                    .id
                    .strip_prefix(SPLIT_PREFIX)
                    .and_then(Column::from_key)
                else {
                    return;
                };
                // The columns share what the Files pane leaves.
                let files = if this.files_shown() {
                    this.layout.files.width + f32::from(ferrum_ui::PANE_GAP)
                } else {
                    0.
                };
                let x = f32::from(event.event.position.x - event.bounds.left()) - files;
                this.resize(column, x, f32::from(event.bounds.size.width) - files, cx);
            }),
        )
        .into_any_element()
    }
}
