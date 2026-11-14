//! The cards: the chat (or the page open in its place), then the Git and
//! Term panes, with the gaps between them as splitters. Panes have no
//! header: the title row's toggles show and hide them.

use ferrum_ui::gpui::{Axis, ElementId, MouseButton, relative};
use ferrum_ui::prelude::*;

use crate::app::FerrumVibe;
use crate::layout::Column;
use crate::nav::Page;

/// Prefix of the splitters' drag ids; the pane on the left follows.
const SPLIT_PREFIX: &str = "vibe-split-";

pub(crate) fn id(prefix: &str, column: Column) -> ElementId {
    ElementId::Name(format!("{prefix}-{}", column.key()).into())
}

impl FerrumVibe {
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
                let Some(column) = event
                    .drag(cx)
                    .id
                    .strip_prefix(SPLIT_PREFIX)
                    .and_then(Column::from_key)
                else {
                    return;
                };
                let x = f32::from(event.event.position.x - event.bounds.left());
                this.resize(column, x, f32::from(event.bounds.size.width), cx);
            }),
        )
        .into_any_element()
    }
}
