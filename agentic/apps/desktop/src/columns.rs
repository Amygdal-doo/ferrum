//! The columns: headers (drag to reorder, ⋯ menu, close), bodies and the
//! splitters between them.

use ferrum_ui::components::{IconButton, IconSize};
use ferrum_ui::gpui::{Axis, ElementId, relative};
use ferrum_ui::prelude::*;

use crate::app::FerrumAgentic;
use crate::layout::{Column, Side};

/// Prefix of the splitters' drag ids; the column on the left follows.
const SPLIT_PREFIX: &str = "agentic-split-";

pub(crate) fn id(prefix: &str, column: Column) -> ElementId {
    ElementId::Name(format!("{prefix}-{}", column.key()).into())
}

pub(crate) fn icon(column: Column) -> IconName {
    match column {
        Column::Chat => IconName::Sparkle,
        Column::Git => IconName::GitBranch,
        Column::Term => IconName::Terminal,
    }
}

/// A menu entry's handler running `f` on the app.
fn on_app(
    weak: WeakEntity<FerrumAgentic>,
    f: impl Fn(&mut FerrumAgentic, &mut Window, &mut Context<FerrumAgentic>) + 'static,
) -> impl Fn(&mut Window, &mut App) + 'static {
    move |window, cx| {
        weak.update(cx, |this, cx| f(this, window, cx)).ok();
    }
}

/// Drag payload of a column header; also its drag preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DraggedColumn(pub Column);

impl Render for DraggedColumn {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        h_flex()
            .gap_1()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(colors.border_focused)
            .bg(colors.elevated_surface_background)
            .child(Icon::new(icon(self.0)).size(IconSize::Small))
            .child(Label::new(self.0.title()).size(LabelSize::Small))
    }
}

impl FerrumAgentic {
    fn column_menu(&self, column: Column, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        let can_left = self.layout.neighbour(column, Side::Left).is_some();
        let can_right = self.layout.neighbour(column, Side::Right).is_some();
        PopoverMenu::new(id("menu", column))
            .anchor(gpui::Anchor::TopRight)
            .trigger(
                IconButton::new(id("menu-button", column), IconName::Ellipsis)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Column options")),
            )
            .menu(move |window, cx| {
                let weak = weak.clone();
                Some(ContextMenu::build(window, cx, move |menu, _, _| {
                    let menu = menu.header(column.title());
                    let menu = if can_left {
                        menu.icon_entry(
                            "Move left",
                            IconName::ArrowLeft,
                            on_app(weak.clone(), move |this, _, cx| {
                                this.move_column(column, Side::Left, cx);
                            }),
                        )
                    } else {
                        menu.disabled_entry("Move left")
                    };
                    let menu = if can_right {
                        menu.icon_entry(
                            "Move right",
                            IconName::ArrowRight,
                            on_app(weak.clone(), move |this, _, cx| {
                                this.move_column(column, Side::Right, cx);
                            }),
                        )
                    } else {
                        menu.disabled_entry("Move right")
                    };
                    menu.separator().icon_entry(
                        "Close column",
                        IconName::Close,
                        on_app(weak.clone(), move |this, window, cx| {
                            this.set_visible(column, false, window, cx);
                        }),
                    )
                }))
            })
    }

    fn header(&self, column: Column, cx: &mut Context<Self>) -> impl IntoElement {
        let status = cx.theme().status();
        let detail: Option<AnyElement> = match column {
            Column::Chat if self.pending_changes > 0 => Some(
                badge(
                    format!("{} to review", self.pending_changes),
                    status.modified,
                    cx,
                )
                .into_any_element(),
            ),
            Column::Term => self.term_title.clone().map(|title| {
                Label::new(title)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
                    .truncate()
                    .into_any_element()
            }),
            _ => None,
        };
        ferrum_ui::pane_header(cx)
            .id(id("header", column))
            .cursor_grab()
            .on_drag(DraggedColumn(column), |dragged, _, _, cx| {
                cx.new(|_| *dragged)
            })
            .child(
                Icon::new(icon(column))
                    .size(IconSize::Small)
                    .color(Color::Accent),
            )
            .child(
                Label::new(column.title())
                    .size(LabelSize::Small)
                    .weight(gpui::FontWeight::SEMIBOLD),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .children(detail),
            )
            .child(self.column_menu(column, cx))
            .child(
                IconButton::new(id("close", column), IconName::Close)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text(format!("Close {}", column.title())))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_visible(column, false, window, cx);
                    })),
            )
    }

    fn body(&self, column: Column, cx: &mut Context<Self>) -> AnyElement {
        let view = match column {
            Column::Chat => self.chat.clone().map(|v| v.into_any_element()),
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
                "Open a folder to use this column.",
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

    fn column(&self, column: Column, fraction: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let drop_bg = colors.drop_target_background;
        ferrum_ui::pane_frame(cx)
            .id(id("column", column))
            .debug_selector(|| format!("column-{}", column.key()))
            .h_full()
            .flex_basis(relative(fraction))
            .flex_shrink(1.)
            .overflow_hidden()
            .drag_over::<DraggedColumn>(move |style, _, _, _| style.bg(drop_bg))
            .on_drop(cx.listener(move |this, dragged: &DraggedColumn, _, cx| {
                this.drop_column(dragged.0, column, cx);
            }))
            .child(self.header(column, cx))
            .child(div().flex_1().min_h_0().child(self.body(column, cx)))
    }

    pub(crate) fn columns(&self, cx: &mut Context<Self>) -> AnyElement {
        let fractions = self.layout.fractions();
        if fractions.is_empty() {
            return empty_state(
                "All columns are closed",
                "Reopen Chat, Git or Term from the top bar.",
            )
            .into_any_element();
        }
        // Rounded cards on the window backdrop, with the gaps as splitters.
        let mut row = h_flex()
            .id("columns")
            .flex_1()
            .min_h_0()
            .w_full()
            .p(ferrum_ui::PANE_GAP);
        let count = fractions.len();
        for (i, (column, fraction)) in fractions.into_iter().enumerate() {
            row = row.child(self.column(column, fraction, cx));
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
