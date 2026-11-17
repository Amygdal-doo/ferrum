//! The diagram: a dotted grid, one band per layer, component cards, the
//! connections between them (styles in the legend), zoom and pan.
//!
//! Click a card to select it (its sides get handles: drag one onto another
//! card, or click it and then the card, to connect them); double-click a
//! database for its schema; drag a card to move it, drag a layer by its
//! label to reorder layers; right-click a connection to restyle it.

use std::collections::HashSet;

use ferrum_ui::gpui::{
    Bounds, DragMoveEvent, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent, PathBuilder, Pixels,
    Point, ScrollEvent, canvas, point,
};
use ferrum_ui::prelude::*;
use ferrumsystem_core::{EdgeStyle, Kind, Side};

use crate::app::{DragLabel, DraggedHandle, DraggedLayer, DraggedNode, FerrumSystem};
use crate::icons::{self, rgb};
use crate::layout::{self, ITEM_H, MAX_ITEMS, NODE_H, NODE_W};

pub fn edge_color(style: EdgeStyle, cx: &App) -> Hsla {
    match style {
        EdgeStyle::Request => cx.theme().colors().text_muted,
        EdgeStyle::Rpc => rgb(0x38bdf8),
        EdgeStyle::Async => rgb(0xa855f7),
        EdgeStyle::Socket => rgb(0x22c55e),
        EdgeStyle::Cdc => rgb(0xeab308),
    }
}

/// From where a connection is being drawn (component, side).
#[derive(Clone)]
pub struct DraggedHandleState(pub String, pub Side);

struct EdgeDraw {
    points: Vec<Point<f32>>,
    color: Hsla,
    width: f32,
    dashed: bool,
}

impl FerrumSystem {
    pub(crate) fn render_canvas(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let accent = colors.text_accent;
        let layout = layout::compute(&self.system);
        let (z, pan) = (self.zoom, self.pan);
        let at = |x: f32, y: f32| point(pan.x + x * z, pan.y + y * z);

        // What to emphasise: the walkthrough step, else the selection.
        let step = self.walk_step().map(|(a, b)| (a.to_owned(), b.to_owned()));
        let mut lit: HashSet<String> = HashSet::new();
        if let Some((a, b)) = &step {
            lit.insert(a.clone());
            lit.insert(b.clone());
        } else if let Some(sel) = &self.selected {
            lit.insert(sel.clone());
        }

        // Connections, with a lane per source so parallel routes separate.
        let mut edges = Vec::new();
        let mut labels = Vec::new();
        let mut routes = Vec::new();
        for (index, route) in layout::routes(&self.system, &layout) {
            let edge = &self.system.edges[index];
            let on_step = step
                .as_ref()
                .is_some_and(|(x, y)| *x == edge.from && *y == edge.to);
            let on_selection = step.is_none()
                && self
                    .selected
                    .as_ref()
                    .is_some_and(|s| *s == edge.from || *s == edge.to);
            let menu_open = self.edge_menu.is_some_and(|(i, _)| i == index);
            let base = edge
                .color
                .map(rgb)
                .unwrap_or_else(|| edge_color(edge.style, cx));
            let base_w = edge.width.unwrap_or(1.4);
            let (color, width) = if on_step || on_selection || menu_open {
                (
                    if edge.color.is_some() { base } else { accent },
                    base_w + 1.,
                )
            } else if step.is_some() || self.selected.is_some() {
                (base.opacity(0.35), base_w * 0.9)
            } else {
                (base, base_w)
            };
            if let Some(text) = &edge.label
                && let Some((lx, ly)) = layout::label_point(&route)
            {
                labels.push((at(lx, ly), text.clone(), on_step || on_selection));
            }
            edges.push(EdgeDraw {
                points: route.iter().map(|(x, y)| at(*x, *y)).collect(),
                color,
                width: width * z.max(0.6),
                dashed: edge.style.dashed(),
            });
            routes.push((index, route));
        }
        self.routes = routes;
        // A walkthrough step between components with no connection: a
        // direct line so the step still shows.
        if let Some((a, b)) = &step
            && !self.system.edges.iter().any(|e| e.from == *a && e.to == *b)
            && let (Some(ra), Some(rb)) = (layout.nodes.get(a), layout.nodes.get(b))
        {
            edges.push(EdgeDraw {
                points: layout::route(ra, rb, 0)
                    .iter()
                    .map(|(x, y)| at(*x, *y))
                    .collect(),
                color: accent,
                width: 2.4 * z.max(0.6),
                dashed: true,
            });
        }
        // The connection being drawn from a handle, to the pointer.
        if let (Some(DraggedHandleState(from, side)), Some(p)) = (&self.drawing, self.drag_pos)
            && let Some(r) = layout.nodes.get(from)
        {
            let (sx, sy) = r.side(*side);
            let (px_, py_) = self.to_world(p);
            edges.push(EdgeDraw {
                points: vec![at(sx, sy), at(px_, py_)],
                color: accent,
                width: 2. * z.max(0.6),
                dashed: true,
            });
        }

        let grid = colors.text.opacity(0.07);
        let bounds_cell = self.canvas_bounds.clone();
        let paint_layer = canvas(
            move |bounds: Bounds<Pixels>, _, _| {
                bounds_cell.set((
                    f32::from(bounds.origin.x),
                    f32::from(bounds.origin.y),
                    f32::from(bounds.size.width),
                    f32::from(bounds.size.height),
                ));
                bounds
            },
            move |_, bounds, window, _| {
                paint_grid(bounds, pan, z, grid, window);
                for edge in &edges {
                    paint_edge(bounds.origin, edge, window);
                }
            },
        )
        .absolute()
        .size_full();

        let node_rects: Vec<layout::Rect> = layout.nodes.values().copied().collect();
        let mut root = div()
            .id("canvas")
            .relative()
            .size_full()
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    this.edge_menu = None;
                    let p = to_f32(ev.position);
                    let (x, y) = this.to_world(p);
                    // On a card or its handles: the card handles it.
                    let pad = 12. / this.zoom;
                    if node_rects.iter().any(|r| {
                        x >= r.x - pad
                            && x <= r.right() + pad
                            && y >= r.y - pad
                            && y <= r.bottom() + pad
                    }) {
                        return;
                    }
                    this.drag_from = Some(point(p.x - this.pan.x, p.y - this.pan.y));
                    if this.connecting.is_none() && this.walk.is_none() {
                        this.select(None, cx);
                    }
                    cx.notify();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                    let p = to_f32(ev.position);
                    let (ox, oy, _, _) = this.canvas_bounds.get();
                    this.edge_menu = this.edge_at(p).map(|i| (i, point(p.x - ox, p.y - oy)));
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                if ev.pressed_button != Some(MouseButton::Left) {
                    this.drag_from = None;
                    return;
                }
                if let Some(from) = this.drag_from {
                    let p = to_f32(ev.position);
                    this.pan = point(p.x - from.x, p.y - from.y);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.drag_from = None;
                    if this.drawing.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .on_drag_move(cx.listener(|this, ev: &DragMoveEvent<DraggedNode>, _, _| {
                this.drag_pos = Some(to_f32(ev.event.position));
            }))
            .on_drag_move(
                cx.listener(|this, ev: &DragMoveEvent<DraggedHandle>, _, cx| {
                    let d = ev.drag(cx).clone();
                    this.drawing = Some(DraggedHandleState(d.0, d.1));
                    this.drag_pos = Some(to_f32(ev.event.position));
                    cx.notify();
                }),
            )
            .on_scroll(cx.listener(|this, ev: &ScrollEvent, _, cx| {
                let d = ev.delta.pixel_delta(px(16.));
                if ev.modifiers.platform || ev.modifiers.control {
                    let factor = 1. - f32::from(d.y) / 400.;
                    this.zoom_by(factor.clamp(0.8, 1.25), cx);
                } else {
                    this.pan.x += f32::from(d.x);
                    this.pan.y += f32::from(d.y);
                    cx.notify();
                }
            }))
            .child(paint_layer);

        // Bands: drop targets for cards and other bands; drag by the label.
        for (i, rect) in &layout.bands {
            let i = *i;
            let layer = &self.system.layers[i];
            let color = rgb(layer.color);
            let o = at(rect.x, rect.y);
            let layer_id = layer.id.clone();
            root =
                root.child(
                    div()
                        .id(SharedString::from(format!("band-{}", layer.id)))
                        .absolute()
                        .left(px(o.x))
                        .top(px(o.y))
                        .w(px(rect.w * z))
                        .h(px(rect.h * z))
                        .rounded(px(10. * z))
                        .border_1()
                        .border_color(color.opacity(0.22))
                        .bg(color.opacity(0.035))
                        .drag_over::<DraggedNode>(move |s, _, _, _| {
                            s.bg(color.opacity(0.1)).border_color(color)
                        })
                        .drag_over::<DraggedLayer>(move |s, _, _, _| s.border_color(color))
                        .on_drop(cx.listener(move |this, d: &DraggedNode, _, cx| {
                            this.drop_node(&d.0, &layer_id, cx)
                        }))
                        .on_drop(cx.listener(move |this, d: &DraggedLayer, _, cx| {
                            this.drop_layer(d.0, i, cx)
                        }))
                        .child(
                            h_flex()
                                .id(SharedString::from(format!("band-label-{}", layer.id)))
                                .px(px(10. * z))
                                .pt(px(6. * z))
                                .gap(px(6. * z))
                                .cursor_grab()
                                .on_drag(DraggedLayer(i), {
                                    let name = SharedString::from(layer.name.clone());
                                    move |_, _, _, cx| cx.new(|_| DragLabel(name.clone()))
                                })
                                .child(div().size(px(7. * z)).rounded(px(2. * z)).bg(color))
                                .child(
                                    div()
                                        .text_size(px(11. * z))
                                        .text_color(colors.text_muted)
                                        .child(layer.name.clone()),
                                ),
                        ),
                );
        }

        // Connection labels, under the cards.
        for (p, text, strong) in labels {
            root = root.child(
                div()
                    .absolute()
                    .left(px(p.x - 60. * z))
                    .top(px(p.y - 8. * z))
                    .w(px(120. * z))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .px(px(4. * z))
                            .rounded(px(3. * z))
                            .bg(colors.panel_background)
                            .text_size(px(9.5 * z))
                            .text_color(if strong { accent } else { colors.text_muted })
                            .child(text),
                    ),
            );
        }

        // Cards.
        let provider = self.system.provider;
        let root_dir = self.icon_root().map(|p| p.to_path_buf());
        let connecting_from = self.connecting.clone().flatten();
        for node in &self.system.nodes {
            let Some(rect) = layout.nodes.get(&node.id) else {
                continue;
            };
            let o = at(rect.x, rect.y);
            let st = &node.style;
            let layer_color = st.color.map(rgb).unwrap_or_else(|| {
                self.system
                    .layers
                    .iter()
                    .find(|l| l.id == node.layer)
                    .map(|l| rgb(l.color))
                    .unwrap_or(accent)
            });
            let service = st.above.clone().unwrap_or_else(|| {
                Kind::find(&node.kind)
                    .map(|k| k.service(provider).to_owned())
                    .unwrap_or_else(|| node.kind.clone())
            });
            let fill = st
                .fill
                .map(rgb)
                .unwrap_or(colors.elevated_surface_background);
            let widths = if st.border.unwrap_or(true) {
                st.border_widths.unwrap_or([1.; 4])
            } else {
                [0.; 4]
            };
            let radius = st.radius.unwrap_or(8.);
            let show_icon = st.icon.unwrap_or(true);
            let below = st.below.clone().filter(|t| !t.is_empty());
            let body = st.body.clone().filter(|t| !t.is_empty());
            let custom_border = st.color.is_some();
            let selected = self.selected.as_deref() == Some(&node.id);
            let highlighted =
                lit.contains(&node.id) || connecting_from.as_deref() == Some(&node.id);
            let dimmed = (step.is_some() || self.selected.is_some()) && !highlighted && !selected;
            let replicas = node
                .deploy
                .as_ref()
                .filter(|d| d.target == ferrumsystem_core::DeployTarget::Pod)
                .and_then(|d| d.replicas);
            let (id, drop_id) = (node.id.clone(), node.id.clone());
            let icon = icons::icon(
                &self.system,
                root_dir.as_deref(),
                node.tech.as_deref(),
                &node.kind,
                px(26. * z),
                cx,
            );
            let more = node.items.len().saturating_sub(MAX_ITEMS);
            let items = node.items.iter().take(MAX_ITEMS).map(|item| {
                h_flex()
                    .h(px(ITEM_H * z))
                    .gap(px(5. * z))
                    .child(
                        div()
                            .size(px(4. * z))
                            .rounded_full()
                            .bg(layer_color.opacity(0.8)),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(10.5 * z))
                            .text_color(colors.text_muted)
                            .child(item.name.clone()),
                    )
            });
            let header = h_flex()
                .h(px(NODE_H * z))
                .px(px(8. * z))
                .gap(px(8. * z))
                .when(show_icon, |r| r.child(icon))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .truncate()
                                .text_size(px(8.5 * z))
                                .text_color(colors.text_muted)
                                .child(service),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(px(12. * z))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(colors.text)
                                .child(node.name.clone()),
                        )
                        .when_some(below, |c, t| {
                            c.child(
                                div()
                                    .truncate()
                                    .text_size(px(10. * z))
                                    .text_color(colors.text_muted)
                                    .child(t),
                            )
                        }),
                )
                .when_some(replicas, |row, n| {
                    row.child(
                        div()
                            .flex_none()
                            .px(px(4. * z))
                            .rounded(px(4. * z))
                            .bg(colors.text.opacity(0.08))
                            .text_size(px(9. * z))
                            .text_color(colors.text_muted)
                            .child(format!("×{n}")),
                    )
                })
                .when(node.database.is_some(), |row| {
                    row.child(
                        Icon::new(IconName::Info)
                            .size(IconSize::XSmall)
                            .color(Color::Muted),
                    )
                });
            let name = SharedString::from(node.name.clone());
            root = root.child(
                h_flex()
                    .id(SharedString::from(format!("node-{}", node.id)))
                    .absolute()
                    .left(px(o.x))
                    .top(px(o.y))
                    .w(px(NODE_W * z))
                    .h(px(rect.h * z))
                    .items_start()
                    .rounded(px(radius * z))
                    .overflow_hidden()
                    .bg(fill)
                    .map(|mut card| {
                        let b = &mut card.style().border_widths;
                        b.top = Some(px(widths[0]).into());
                        b.right = Some(px(widths[1]).into());
                        b.bottom = Some(px(widths[2]).into());
                        b.left = Some(px(widths[3]).into());
                        card
                    })
                    .border_color(if selected || highlighted {
                        accent
                    } else if custom_border {
                        layer_color.opacity(0.7)
                    } else {
                        colors.border
                    })
                    .when(selected, |c| c.shadow_md())
                    .when(dimmed, |c| c.opacity(0.55))
                    .cursor_pointer()
                    .drag_over::<DraggedHandle>(move |s, _, _, _| s.border_color(accent))
                    .on_drop(cx.listener(move |this, d: &DraggedHandle, _, cx| {
                        this.drawing = None;
                        this.drop_handle(&d.0, d.1, &drop_id, cx)
                    }))
                    .on_drag(DraggedNode(node.id.clone()), move |_, _, _, cx| {
                        cx.new(|_| DragLabel(name.clone()))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                            this.click_node(&id, ev.click_count, cx);
                        }),
                    )
                    .child(div().w(px(3. * z)).h_full().bg(layer_color))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(header)
                            .when_some(body, |c, t| {
                                c.child(
                                    div()
                                        .px(px(10. * z))
                                        .pb(px(6. * z))
                                        .text_size(px(10.5 * z))
                                        .line_height(px(14. * z))
                                        .text_color(colors.text)
                                        .child(t),
                                )
                            })
                            .when(!node.items.is_empty(), |c| {
                                c.child(
                                    v_flex()
                                        .px(px(10. * z))
                                        .pb(px(6. * z))
                                        .children(items)
                                        .when(more > 0, |c| {
                                            c.child(
                                                div()
                                                    .h(px(ITEM_H * z))
                                                    .text_size(px(10. * z))
                                                    .text_color(colors.text_muted)
                                                    .child(format!("+{more} more")),
                                            )
                                        }),
                                )
                            }),
                    ),
            );
            if selected {
                root = root.child(self.node_toolbar(&node.id, at(rect.x, rect.y), cx));
                for side in Side::ALL {
                    let (hx, hy) = rect.side(side);
                    let c = at(hx, hy);
                    let size = 11.;
                    let from = node.id.clone();
                    let click_from = node.id.clone();
                    root = root.child(
                        div()
                            .id(SharedString::from(format!(
                                "handle-{}-{}",
                                node.id,
                                side.label()
                            )))
                            .absolute()
                            .left(px(c.x - size / 2.))
                            .top(px(c.y - size / 2.))
                            .size(px(size))
                            .rounded_full()
                            .bg(accent)
                            .border_2()
                            .border_color(colors.elevated_surface_background)
                            .cursor_crosshair()
                            .hover(|s| s.size(px(size + 3.)))
                            .tooltip(Tooltip::text(
                                "Drag to a component, or click and then click one",
                            ))
                            .on_drag(DraggedHandle(from, side), |_, _, _, cx| {
                                cx.new(|_| DragLabel("Connect…".into()))
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.connecting = Some(Some(click_from.clone()));
                                this.connect_side = Some(side);
                                cx.notify();
                            })),
                    );
                }
            }
        }

        root.child(self.render_legend(cx))
            .child(self.render_zoom_controls(cx))
            .children(self.render_connect_hint(cx))
            .children(self.render_edge_menu(cx))
            .into_any_element()
    }

    /// The small bar above the selected card.
    fn node_toolbar(&self, id: &str, o: Point<f32>, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let (a, b, c) = (id.to_owned(), id.to_owned(), id.to_owned());
        h_flex()
            .id("node-toolbar")
            .absolute()
            .left(px(o.x))
            .top(px(o.y - 34.))
            .p_0p5()
            .gap_0p5()
            .rounded_md()
            .border_1()
            .border_color(colors.border)
            .bg(colors.elevated_surface_background)
            .shadow_md()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                IconButton::new("node-settings", IconName::Settings)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Settings: style, text, items, deployment"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.set_panel_open(true, cx);
                        this.tab = crate::app::Tab::Component;
                        cx.notify();
                    })),
            )
            .child(
                IconButton::new("node-connect", IconName::Link)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Connect to another component"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.connecting = Some(Some(a.clone()));
                        this.connect_side = None;
                        cx.notify();
                    })),
            )
            .child(
                IconButton::new("node-duplicate", IconName::Copy)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Duplicate"))
                    .on_click(cx.listener(move |this, _, _, cx| this.duplicate(&b, cx))),
            )
            .child(
                IconButton::new("node-delete", IconName::Trash)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Delete (⌫)"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected = Some(c.clone());
                        this.delete_selected(cx);
                    })),
            )
    }

    /// Right-click menu of a connection: sides, kind, colour, width.
    fn render_edge_menu(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (index, at) = self.edge_menu?;
        let edge = self.system.edges.get(index)?.clone();
        let colors = cx.theme().colors().clone();
        let label = |t: &'static str| {
            div()
                .pt_1p5()
                .text_size(px(11.))
                .text_color(colors.text_muted)
                .child(t)
        };
        let chip = |id: SharedString, text: SharedString, on: bool| {
            Button::new(id, text)
                .size(ButtonSize::Compact)
                .label_size(LabelSize::Small)
                .style(if on {
                    ButtonStyle::Filled
                } else {
                    ButtonStyle::Subtle
                })
        };
        let sides = |from: bool, cx: &mut Context<Self>| {
            let current = if from { edge.from_side } else { edge.to_side };
            let mut row = h_flex().flex_wrap().gap_0p5();
            let opts: [Option<Side>; 5] = [
                None,
                Some(Side::Top),
                Some(Side::Right),
                Some(Side::Bottom),
                Some(Side::Left),
            ];
            for opt in opts {
                let text = opt.map(|s| s.label()).unwrap_or("Auto");
                row = row.child(
                    chip(
                        format!("side-{from}-{text}").into(),
                        text.into(),
                        current == opt,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_edge(index, cx, |e| {
                            if from {
                                e.from_side = opt;
                            } else {
                                e.to_side = opt;
                            }
                        })
                    })),
                );
            }
            row
        };
        let from_row = sides(true, cx);
        let to_row = sides(false, cx);
        let kinds = h_flex()
            .flex_wrap()
            .gap_0p5()
            .children(EdgeStyle::ALL.map(|k| {
                chip(
                    format!("kind-{k:?}").into(),
                    k.label().into(),
                    edge.style == k,
                )
                .on_click(
                    cx.listener(move |this, _, _, cx| this.edit_edge(index, cx, |e| e.style = k)),
                )
            }));
        const SWATCHES: [u32; 8] = [
            0x8b8b90, 0x38bdf8, 0xa855f7, 0x22c55e, 0xeab308, 0xef4444, 0xec4899, 0x14b8a6,
        ];
        let swatches =
            h_flex()
                .gap_1()
                .child(
                    chip(
                        "color-default".into(),
                        "Default".into(),
                        edge.color.is_none(),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_edge(index, cx, |e| e.color = None)
                    })),
                )
                .children(SWATCHES.map(|c| {
                    div()
                        .id(SharedString::from(format!("color-{c:06x}")))
                        .size(px(18.))
                        .rounded_full()
                        .bg(rgb(c))
                        .border_2()
                        .border_color(if edge.color == Some(c) {
                            colors.text
                        } else {
                            colors.border
                        })
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit_edge(index, cx, |e| e.color = Some(c))
                        }))
                }));
        let widths = h_flex().gap_0p5().children([1., 2., 3., 4.].map(|w: f32| {
            let on = (edge.width.unwrap_or(1.4) - w).abs() < 0.3;
            chip(format!("width-{w}").into(), format!("{w:.0} px").into(), on).on_click(
                cx.listener(move |this, _, _, cx| this.edit_edge(index, cx, |e| e.width = Some(w))),
            )
        }));
        Some(
            v_flex()
                .id("edge-menu")
                .absolute()
                .left(px(at.x))
                .top(px(at.y))
                .w(px(300.))
                .p_2()
                .gap_0p5()
                .rounded_lg()
                .border_1()
                .border_color(colors.border)
                .bg(colors.elevated_surface_background)
                .shadow_lg()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_size(px(12.))
                        .child(format!(
                            "{} → {}",
                            self.node_name(&edge.from),
                            self.node_name(&edge.to)
                        )),
                )
                .child(label("Leaves from"))
                .child(from_row)
                .child(label("Arrives at"))
                .child(to_row)
                .child(label("Kind"))
                .child(kinds)
                .child(label("Colour"))
                .child(swatches)
                .child(label("Thickness"))
                .child(widths)
                .child(
                    h_flex()
                        .pt_2()
                        .gap_1()
                        .child(
                            Button::new("edge-reverse", "Reverse")
                                .size(ButtonSize::Compact)
                                .style(ButtonStyle::Outlined)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.edit_edge(index, cx, |e| {
                                        std::mem::swap(&mut e.from, &mut e.to);
                                        std::mem::swap(&mut e.from_side, &mut e.to_side);
                                    })
                                })),
                        )
                        .child(div().flex_1())
                        .child(
                            Button::new("edge-delete", "Delete")
                                .size(ButtonSize::Compact)
                                .style(ButtonStyle::Tinted(ferrum_ui::components::TintColor::Error))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.edge_menu = None;
                                    this.edit(cx, |s| {
                                        if index < s.edges.len() {
                                            s.edges.remove(index);
                                        }
                                    });
                                })),
                        ),
                ),
        )
    }

    fn render_legend(&self, cx: &App) -> impl IntoElement {
        let colors = cx.theme().colors();
        v_flex()
            .absolute()
            .bottom_3()
            .right_3()
            .p_2()
            .gap_1()
            .rounded_md()
            .border_1()
            .border_color(colors.border)
            .bg(colors.elevated_surface_background.opacity(0.92))
            .children(EdgeStyle::ALL.map(|style| {
                let color = edge_color(style, cx);
                h_flex()
                    .gap_2()
                    .child(line_sample(color, style.dashed()))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(colors.text_muted)
                            .child(style.label()),
                    )
            }))
    }

    fn render_zoom_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        v_flex()
            .absolute()
            .bottom_3()
            .left_3()
            .p_0p5()
            .rounded_md()
            .border_1()
            .border_color(colors.border)
            .bg(colors.elevated_surface_background)
            .child(
                IconButton::new("zoom-in", IconName::Plus)
                    .tooltip(Tooltip::text("Zoom in (⌘=)"))
                    .on_click(cx.listener(|this, _, _, cx| this.zoom_by(1.2, cx))),
            )
            .child(
                IconButton::new("zoom-out", IconName::Dash)
                    .tooltip(Tooltip::text("Zoom out (⌘-)"))
                    .on_click(cx.listener(|this, _, _, cx| this.zoom_by(1. / 1.2, cx))),
            )
            .child(
                IconButton::new("zoom-fit", IconName::ArrowCircle)
                    .tooltip(Tooltip::text("Fit (⌘0)"))
                    .on_click(cx.listener(|this, _, _, cx| this.fit(cx))),
            )
    }

    fn render_connect_hint(&self, cx: &App) -> Option<impl IntoElement> {
        let from = self.connecting.as_ref()?;
        let text = match from {
            None => "Connect: click the component the connection starts at".to_owned(),
            Some(id) => format!(
                "Connect from {}: click where it goes (click an existing one to remove it)",
                self.system.node(id).map(|n| n.name.as_str()).unwrap_or(id)
            ),
        };
        let colors = cx.theme().colors();
        Some(
            div()
                .absolute()
                .top_3()
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .bg(colors.elevated_surface_background)
                        .border_1()
                        .border_color(colors.text_accent)
                        .text_size(px(12.))
                        .child(text),
                ),
        )
    }
}

fn to_f32(p: Point<Pixels>) -> Point<f32> {
    point(f32::from(p.x), f32::from(p.y))
}

/// A 26-pixel line for the legend, dashed for async kinds.
fn line_sample(color: Hsla, dashed: bool) -> impl IntoElement {
    let segments = if dashed { 4 } else { 1 };
    h_flex()
        .w(px(26.))
        .gap(px(3.))
        .children((0..segments).map(move |_| div().flex_1().h(px(2.)).bg(color)))
}

fn paint_grid(
    bounds: Bounds<Pixels>,
    pan: Point<f32>,
    zoom: f32,
    color: Hsla,
    window: &mut Window,
) {
    let step = 24. * zoom;
    if step < 6. {
        return;
    }
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let dot = px((1.4 * zoom).max(1.));
    let mut x = pan.x.rem_euclid(step);
    while x < w {
        let mut y = pan.y.rem_euclid(step);
        while y < h {
            window.paint_quad(gpui::fill(
                Bounds::new(bounds.origin + point(px(x), px(y)), gpui::size(dot, dot)),
                color,
            ));
            y += step;
        }
        x += step;
    }
}

fn paint_edge(origin: Point<Pixels>, edge: &EdgeDraw, window: &mut Window) {
    let p = |q: Point<f32>| origin + point(px(q.x), px(q.y));
    let mut builder = PathBuilder::stroke(px(edge.width));
    if edge.dashed {
        builder = builder.dash_array(&[px(5.), px(4.)]);
    }
    let Some(first) = edge.points.first() else {
        return;
    };
    builder.move_to(p(*first));
    for q in &edge.points[1..] {
        builder.line_to(p(*q));
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, edge.color);
    }
    // Arrowhead at the end, pointing along the last segment.
    let n = edge.points.len();
    if n < 2 {
        return;
    }
    let (a, b) = (edge.points[n - 2], edge.points[n - 1]);
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let (ux, uy) = (dx / len, dy / len);
    let s = 3. + edge.width * 1.6;
    let back = point(b.x - ux * s * 1.6, b.y - uy * s * 1.6);
    let mut head = PathBuilder::fill();
    head.move_to(p(b));
    head.line_to(p(point(back.x - uy * s, back.y + ux * s)));
    head.line_to(p(point(back.x + uy * s, back.y - ux * s)));
    head.close();
    if let Ok(path) = head.build() {
        window.paint_path(path, edge.color);
    }
}
