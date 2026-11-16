//! ER diagram tab: tables as cards, foreign keys as curves, drag to pan,
//! ⌘/Ctrl + scroll (or the buttons) to zoom. Layout comes from
//! `ferrumdata_application::er`; this file only scales and draws it.

use crate::app::FerrumData;
use crate::state::{DiagramView, SnapshotLoad};
use ferrum_ui::gpui::{
    Bounds, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathBuilder, Pixels,
    ScrollEvent, canvas, fill, point, size,
};
use ferrum_ui::prelude::*;
use ferrumdata_application::er::{ErLayout, ErNode, HEADER_HEIGHT, ROW_HEIGHT};
use ferrumdata_domain::{EditorTabId, ProfileId, TableInfo};
use std::sync::Arc;

/// Below this zoom only table names are drawn.
const DETAIL_ZOOM: f32 = 0.45;
const ZOOM_STEP: f32 = 1.25;

impl FerrumData {
    pub fn render_diagram_view(
        &mut self,
        tab_id: EditorTabId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let Some(profile_id) = self
            .state
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.profile_id)
        else {
            return div().into_any_element();
        };
        let Some(view) = self.diagram(tab_id).cloned() else {
            return div().into_any_element();
        };
        let message = |text: String| {
            div()
                .p_4()
                .child(Label::new(text).size(LabelSize::Small).color(Color::Muted))
                .into_any_element()
        };
        let summary = match &view.load {
            SnapshotLoad::Loaded(s) if s.omitted > 0 => {
                format!("{} tables ({} more not shown)", s.tables.len(), s.omitted)
            }
            SnapshotLoad::Loaded(s) => format!(
                "{} tables · {} foreign keys",
                s.tables.len(),
                view.layout.as_ref().map_or(0, |l| l.edges.len())
            ),
            _ => String::new(),
        };
        let body = match (&view.load, &view.layout) {
            (SnapshotLoad::Loading, _) => message("Reading the schema…".into()),
            (SnapshotLoad::Failed(e), _) => message(e.clone()),
            (SnapshotLoad::Loaded(_), Some(layout)) if layout.nodes.is_empty() => {
                message("This connection has no tables.".into())
            }
            (SnapshotLoad::Loaded(_), Some(layout)) => {
                self.render_diagram_canvas(tab_id, profile_id, &view, layout.clone(), cx)
            }
            (SnapshotLoad::Loaded(_), None) => message(String::new()),
        };
        let zoom = view.zoom;
        let button = |id: &'static str, label: &'static str| {
            Button::new(id, label)
                .style(ButtonStyle::Outlined)
                .size(ButtonSize::Compact)
        };

        v_flex()
            .flex_1()
            .min_h_0()
            .child(
                h_flex()
                    .flex_none()
                    .gap_2()
                    .px_4()
                    .py_2()
                    .border_b_1()
                    .border_color(colors.border_variant)
                    .child(
                        Label::new(summary)
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(div().flex_1())
                    .child(
                        Label::new(
                            "Drag to pan · ⌘/Ctrl + scroll to zoom · double-click opens a table",
                        )
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                    )
                    .child(button("er-zoom-out", "−").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.zoom_diagram(tab_id, 1.0 / ZOOM_STEP, None, cx);
                        },
                    )))
                    .child(
                        div()
                            .w(px(44.))
                            .text_xs()
                            .text_center()
                            .child(format!("{:.0} %", zoom * 100.0)),
                    )
                    .child(button("er-zoom-in", "+").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.zoom_diagram(tab_id, ZOOM_STEP, None, cx);
                        },
                    )))
                    .child(
                        button("er-fit", "Fit").on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(view) = this.state.diagrams.get_mut(&tab_id) {
                                view.fit_pending = true;
                                cx.notify();
                            }
                        })),
                    )
                    .child(button("er-reload", "Reload").on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.load_diagram(tab_id, profile_id, cx);
                        },
                    ))),
            )
            .child(div().flex_1().min_h_0().relative().child(body))
            .into_any_element()
    }

    /// Zooms around `anchor` (canvas coordinates; the centre when `None`).
    fn zoom_diagram(
        &mut self,
        tab_id: EditorTabId,
        factor: f32,
        anchor: Option<(f32, f32)>,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.diagram_bounds.get();
        let center = (
            f32::from(bounds.size.width) / 2.0,
            f32::from(bounds.size.height) / 2.0,
        );
        if let Some(view) = self.state.diagrams.get_mut(&tab_id) {
            view.zoom_at(factor, anchor.unwrap_or(center));
            cx.notify();
        }
    }

    fn render_diagram_canvas(
        &mut self,
        tab_id: EditorTabId,
        profile_id: ProfileId,
        view: &DiagramView,
        layout: Arc<ErLayout>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let edge_color: Hsla = colors.text_muted.opacity(0.75);
        let (zoom, pan) = (view.zoom, view.pan);
        let bounds_cell = self.diagram_bounds.clone();
        let local = move |p: ferrum_ui::gpui::Point<Pixels>| {
            let origin = bounds_cell.get().origin;
            (f32::from(p.x - origin.x), f32::from(p.y - origin.y))
        };
        let viewport = self.diagram_bounds.get().size;
        let visible = |n: &ErNode| {
            let x = n.x * zoom + pan.0;
            let y = n.y * zoom + pan.1;
            x < f32::from(viewport.width)
                && y < f32::from(viewport.height)
                && x + n.width * zoom > 0.0
                && y + n.height * zoom > 0.0
        };
        let snapshot = match &view.load {
            SnapshotLoad::Loaded(snapshot) => Some(snapshot.clone()),
            _ => None,
        };
        let nodes: Vec<AnyElement> = layout
            .nodes
            .iter()
            .filter(|n| viewport.width <= px(0.) || visible(n))
            .map(|n| {
                let info = snapshot
                    .as_ref()
                    .and_then(|s| s.tables.get(n.table))
                    .map(|t| t.info.clone());
                self.render_diagram_node(profile_id, info, n, zoom, pan, cx)
            })
            .collect();

        let edges_layout = layout.clone();
        let store_bounds = self.diagram_bounds.clone();
        let fit = view.fit_pending;
        let this = cx.entity().downgrade();
        let edges = canvas(
            move |bounds, _, cx| {
                store_bounds.set(bounds);
                if fit {
                    // The canvas size is known only now: fit on the next frame.
                    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                    if let Err(e) = this.update(cx, |this, cx| {
                        if let Some(view) = this.state.diagrams.get_mut(&tab_id) {
                            view.fit(w, h);
                            view.fit_pending = false;
                            cx.notify();
                        }
                    }) {
                        tracing::debug!(error = %e, "diagram dropped");
                    }
                }
            },
            move |bounds, (), window, _| {
                paint_edges(&edges_layout, bounds, zoom, pan, edge_color, window);
            },
        )
        .absolute()
        .size_full();

        let down_local = local.clone();
        let move_local = local.clone();
        div()
            .id(SharedString::from(format!("diagram-{tab_id}")))
            .absolute()
            .inset_0()
            .overflow_hidden()
            .bg(colors.editor_background)
            .cursor_grab()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    if let Some(view) = this.state.diagrams.get_mut(&tab_id) {
                        view.drag = Some((down_local(e.position), view.pan));
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(move |this, e: &MouseMoveEvent, _, cx| {
                let Some(view) = this.state.diagrams.get_mut(&tab_id) else {
                    return;
                };
                match view.drag {
                    Some((start, from)) if e.pressed_button == Some(MouseButton::Left) => {
                        let now = move_local(e.position);
                        view.pan = (from.0 + now.0 - start.0, from.1 + now.1 - start.1);
                        cx.notify();
                    }
                    Some(_) => view.drag = None,
                    None => {}
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseUpEvent, _, _| {
                    if let Some(view) = this.state.diagrams.get_mut(&tab_id) {
                        view.drag = None;
                    }
                }),
            )
            .on_scroll(cx.listener(move |this, e: &ScrollEvent, _, cx| {
                let delta = e.delta.pixel_delta(px(20.));
                let (dx, dy) = (f32::from(delta.x), f32::from(delta.y));
                if e.modifiers.platform || e.modifiers.control {
                    let factor = (1.0 + dy / 300.0).clamp(0.5, 2.0);
                    this.zoom_diagram(tab_id, factor, Some(local(e.position)), cx);
                } else if let Some(view) = this.state.diagrams.get_mut(&tab_id) {
                    view.pan = (view.pan.0 + dx, view.pan.1 + dy);
                    cx.notify();
                }
            }))
            .child(edges)
            .children(nodes)
            .into_any_element()
    }

    fn render_diagram_node(
        &mut self,
        profile_id: ProfileId,
        info: Option<TableInfo>,
        node: &ErNode,
        zoom: f32,
        pan: (f32, f32),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let accent = cx.theme().status().info;
        let warning = cx.theme().status().warning;
        let font = px(12.0 * zoom);
        let detail = zoom >= DETAIL_ZOOM;
        let rows = node.rows.iter().map(|row| {
            let (mark, tone) = match (row.primary_key, row.foreign_key) {
                (true, true) => ("PK FK", Some(warning)),
                (true, false) => ("PK", Some(warning)),
                (false, true) => ("FK", Some(accent)),
                (false, false) => ("", None),
            };
            h_flex()
                .h(px(ROW_HEIGHT * zoom))
                .px(px(8.0 * zoom))
                .gap(px(6.0 * zoom))
                .child(
                    div()
                        .flex_none()
                        .w(px(34.0 * zoom))
                        .when_some(tone, |this, c| this.text_color(c))
                        .child(mark),
                )
                .child(div().flex_1().min_w_0().truncate().child(row.name.clone()))
                .child(
                    div()
                        .flex_none()
                        .max_w(px(150.0 * zoom))
                        .truncate()
                        .text_color(colors.text_muted)
                        .child(row.data_type.clone()),
                )
        });
        div()
            .id(SharedString::from(format!("er-node-{}", node.table)))
            .absolute()
            .left(px(node.x * zoom + pan.0))
            .top(px(node.y * zoom + pan.1))
            .w(px(node.width * zoom))
            .h(px(node.height * zoom))
            .overflow_hidden()
            .rounded(px(6.0 * zoom))
            .border_1()
            .border_color(colors.border)
            .bg(colors.elevated_surface_background)
            .font_buffer(cx)
            .text_size(font)
            .cursor_default()
            .on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, _, cx| {
                // Cards are not drag handles for the canvas.
                if e.click_count < 2 {
                    cx.stop_propagation();
                }
            })
            .when_some(info, |this, info| {
                this.on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                    if e.click_count() >= 2 {
                        this.open_table(profile_id, info.clone(), cx);
                    }
                }))
            })
            .child(
                div()
                    .h(px(HEADER_HEIGHT * zoom))
                    .px(px(8.0 * zoom))
                    .flex()
                    .items_center()
                    .bg(colors.element_selected)
                    .font_weight(ferrum_ui::gpui::FontWeight::SEMIBOLD)
                    .child(div().truncate().child(node.title.clone())),
            )
            .when(detail, |this| {
                this.children(rows).when(node.hidden > 0, |this| {
                    this.child(
                        div()
                            .h(px(ROW_HEIGHT * zoom))
                            .px(px(8.0 * zoom))
                            .text_color(colors.text_muted)
                            .child(format!("… {} more columns", node.hidden)),
                    )
                })
            })
            .into_any_element()
    }
}

/// Foreign keys as cubic curves from the referencing column to the
/// referenced one, leaving and entering each card on the facing side.
fn paint_edges(
    layout: &ErLayout,
    bounds: Bounds<Pixels>,
    zoom: f32,
    pan: (f32, f32),
    color: Hsla,
    window: &mut Window,
) {
    let at = |x: f32, y: f32| {
        point(
            bounds.origin.x + px(x * zoom + pan.0),
            bounds.origin.y + px(y * zoom + pan.1),
        )
    };
    let bend = 40.0;
    for edge in &layout.edges {
        let (Some(from), Some(to)) = (layout.nodes.get(edge.from), layout.nodes.get(edge.to))
        else {
            continue;
        };
        let y1 = from.row_center(edge.from_row);
        let y2 = to.row_center(edge.to_row);
        // Leave towards the referenced card; a self reference loops on the right.
        let (x1, d1, x2, d2) = if edge.from == edge.to || to.x >= from.x + from.width {
            if edge.from == edge.to {
                (from.x + from.width, 1.0, to.x + to.width, 1.0)
            } else {
                (from.x + from.width, 1.0, to.x, -1.0)
            }
        } else if to.x + to.width <= from.x {
            (from.x, -1.0, to.x + to.width, 1.0)
        } else {
            (from.x + from.width, 1.0, to.x + to.width, 1.0)
        };
        let mut path = PathBuilder::stroke(px(1.25));
        path.move_to(at(x1, y1));
        path.cubic_bezier_to(at(x2, y2), at(x1 + d1 * bend, y1), at(x2 + d2 * bend, y2));
        match path.build() {
            Ok(path) => window.paint_path(path, color),
            Err(e) => tracing::debug!(error = ?e, "edge path"),
        }
        // A small block marks the referenced (one) end.
        let end = at(x2, y2);
        let side = px(5.0 * zoom.max(0.6));
        window.paint_quad(fill(
            Bounds::new(
                point(end.x - side / 2.0, end.y - side / 2.0),
                size(side, side),
            ),
            color,
        ));
    }
}
