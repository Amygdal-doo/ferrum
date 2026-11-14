//! The Component tab: the selected component and everything about it,
//! with its edits (name, layer, position, deployment, icon, connections).

use ferrum_ui::prelude::*;
use ferrumsystem_core::{DeployTarget, Kind, Node};

use crate::app::{Dialog, FerrumSystem};
use crate::panel::{chip, heading, para};

/// The Component tab's text fields for one component.
#[derive(Clone)]
pub struct NodeFields {
    pub id: String,
    pub name: Entity<TextInput>,
    pub description: Entity<TextInput>,
    pub above: Entity<TextInput>,
    pub below: Entity<TextInput>,
    pub body: Entity<TextInput>,
    pub item: Entity<TextInput>,
    _subscriptions: std::rc::Rc<Vec<Subscription>>,
}

const TARGETS: [DeployTarget; 6] = [
    DeployTarget::Pod,
    DeployTarget::Container,
    DeployTarget::Vm,
    DeployTarget::Serverless,
    DeployTarget::Managed,
    DeployTarget::Device,
];

fn small(id: impl Into<ElementId>, label: impl Into<SharedString>, on: bool) -> Button {
    Button::new(id, label)
        .size(ButtonSize::Compact)
        .style(if on {
            ButtonStyle::Filled
        } else {
            ButtonStyle::Subtle
        })
        .label_size(LabelSize::Small)
        .toggle_state(on)
}

impl FerrumSystem {
    /// The text fields for `node`, made when the selection changes. Every
    /// change is saved as it is typed; a new item on Enter.
    fn node_fields(
        &mut self,
        node: &Node,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> NodeFields {
        if let Some(f) = &self.fields
            && f.id == node.id
        {
            return f.clone();
        }
        let st = &node.style;
        let make = |text: &str,
                    placeholder: &'static str,
                    lines: usize,
                    window: &mut Window,
                    cx: &mut Context<Self>| {
            cx.new(|cx| {
                let input = TextInput::new(window, cx)
                    .placeholder(placeholder)
                    .with_text(text.to_owned());
                if lines > 1 {
                    input.multiline(lines)
                } else {
                    input
                }
            })
        };
        let name = make(&node.name, "Name", 1, window, cx);
        let description = make(
            &node.description,
            "What it does and why it is here",
            3,
            window,
            cx,
        );
        let above = make(
            st.above.as_deref().unwrap_or(""),
            "Above the title (default: the cloud service)",
            1,
            window,
            cx,
        );
        let below = make(
            st.below.as_deref().unwrap_or(""),
            "Below the title",
            1,
            window,
            cx,
        );
        let body = make(
            st.body.as_deref().unwrap_or(""),
            "Text inside the card",
            2,
            window,
            cx,
        );
        let item = make("", "Add an item inside, then Enter", 1, window, cx);
        let id = node.id.clone();
        let mut subs = Vec::new();
        let field = |f: fn(&mut Node, String)| {
            let id = id.clone();
            move |this: &mut Self,
                  input: Entity<TextInput>,
                  event: &TextInputEvent,
                  cx: &mut Context<Self>| {
                if *event == TextInputEvent::Changed {
                    let text = input.read(cx).text().to_owned();
                    this.edit_node(&id, cx, |n| f(n, text));
                }
            }
        };
        let opt = |t: String| Some(t).filter(|t| !t.trim().is_empty());
        subs.push(cx.subscribe(
            &name,
            field(|n, t| {
                if !t.trim().is_empty() {
                    n.name = t;
                }
            }),
        ));
        subs.push(cx.subscribe(&description, field(|n, t| n.description = t)));
        subs.push(cx.subscribe(
            &above,
            field(move |n, t| n.style.above = Some(t).filter(|t| !t.trim().is_empty())),
        ));
        subs.push(cx.subscribe(
            &below,
            field(move |n, t| n.style.below = Some(t).filter(|t| !t.trim().is_empty())),
        ));
        subs.push(cx.subscribe(
            &body,
            field(move |n, t| n.style.body = Some(t).filter(|t| !t.trim().is_empty())),
        ));
        let _ = opt;
        let item_id = id.clone();
        subs.push(cx.subscribe(
            &item,
            move |this: &mut Self, input, event: &TextInputEvent, cx| {
                if *event == TextInputEvent::Submit {
                    let text = input.read(cx).text().trim().to_owned();
                    if !text.is_empty() {
                        this.edit_node(&item_id, cx, |n| {
                            n.items.push(ferrumsystem_core::Item {
                                name: text,
                                detail: String::new(),
                            })
                        });
                        input.update(cx, |i, cx| i.set_text("", cx));
                    }
                }
            },
        ));
        let fields = NodeFields {
            id: node.id.clone(),
            name,
            description,
            above,
            below,
            body,
            item,
            _subscriptions: std::rc::Rc::new(subs),
        };
        self.fields = Some(fields.clone());
        fields
    }

    fn edit_node(&mut self, id: &str, cx: &mut Context<Self>, f: impl FnOnce(&mut Node)) {
        let id = id.to_owned();
        self.edit(cx, |s| {
            if let Some(n) = s.nodes.iter_mut().find(|n| n.id == id) {
                f(n);
            }
        });
    }

    /// Moves `id` one place left (`-1`) or right (`1`) in its layer.
    fn shift(&mut self, id: &str, by: i32, cx: &mut Context<Self>) {
        let id = id.to_owned();
        self.edit(cx, |s| {
            let Some(layer) = s.node(&id).map(|n| n.layer.clone()) else {
                return;
            };
            let order: Vec<String> = s.nodes_in(&layer).iter().map(|n| n.id.clone()).collect();
            let Some(i) = order.iter().position(|x| *x == id) else {
                return;
            };
            let j = i as i32 + by;
            if j < 0 || j as usize >= order.len() {
                return;
            }
            let mut order = order;
            order.swap(i, j as usize);
            for (slot, nid) in order.iter().enumerate() {
                if let Some(n) = s.nodes.iter_mut().find(|n| n.id == *nid) {
                    n.slot = Some(slot as f32);
                }
            }
        });
    }

    pub(crate) fn component_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(node) = self
            .selected
            .as_ref()
            .and_then(|id| self.system.node(id))
            .cloned()
        else {
            return para("Select a component on the diagram.", cx).into_any_element();
        };
        let colors = cx.theme().colors().clone();
        let kind = Kind::find(&node.kind);
        let service = kind.map(|k| k.service(self.system.provider)).unwrap_or("");
        let tech_name = node
            .tech
            .as_deref()
            .and_then(|t| crate::icons::tech_info(&self.system, t))
            .map(|(name, _, _)| name);
        let fields = self.node_fields(&node, window, cx);
        let field = fields.name.clone();
        let id = node.id.clone();
        let icon = crate::icons::icon(
            &self.system,
            self.icon_root(),
            node.tech.as_deref(),
            &node.kind,
            px(40.),
            cx,
        );

        let mut col = v_flex()
            .pt_3()
            .gap_1()
            .child(
                h_flex().gap_3().child(icon).child(
                    v_flex().flex_1().min_w_0().gap_0p5().child(field).child(
                        div()
                            .text_size(px(12.))
                            .text_color(colors.text_muted)
                            .child(format!(
                                "{}{}",
                                kind.map(|k| k.name).unwrap_or(&node.kind),
                                tech_name.map(|t| format!(" · {t}")).unwrap_or_default()
                            )),
                    ),
                ),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(colors.text_muted)
                    .child(format!("On {}: {service}", self.system.provider.label())),
            )
            .child(heading("Description", cx))
            .child(fields.description.clone());

        // Items inside.
        col = col.child(heading("Inside", cx));
        for (i, item) in node.items.iter().enumerate() {
            let nid = id.clone();
            col = col.child(
                h_flex()
                    .gap_2()
                    .child(div().size(px(5.)).rounded_full().bg(colors.text_muted))
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(12.))
                            .child(item.name.clone()),
                    )
                    .child(
                        IconButton::new(("remove-item", i), IconName::Close)
                            .icon_size(IconSize::XSmall)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.edit_node(&nid, cx, |n| {
                                    if i < n.items.len() {
                                        n.items.remove(i);
                                    }
                                })
                            })),
                    ),
            );
        }
        col = col.child(fields.item.clone());
        col = self.style_section(col, &node, &fields, cx);

        // Icon.
        let icon_for = node.tech.clone().unwrap_or_else(|| node.id.clone());
        col = col.child(heading("Icon", cx)).child(
            h_flex()
                .gap_2()
                .child(
                    small("choose-icon", "Choose icon…", false).on_click(cx.listener(
                        move |this, _, window, cx| this.choose_icon(icon_for.clone(), window, cx),
                    )),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(colors.text_muted)
                        .child("PNG, SVG or JPEG; saved in .architecture/icons/"),
                ),
        );

        // Database.
        if let Some(db) = node.database.clone() {
            col = col.child(heading("Data", cx)).child(
                small("open-schema", "Open schema", false).on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.dialog = Some(Dialog::Schema(db.clone()));
                        cx.notify();
                    },
                )),
            );
        }

        // Layer and position.
        let layers: Vec<_> = self
            .system
            .layers
            .iter()
            .map(|l| (l.id.clone(), l.name.clone()))
            .collect();
        col = col.child(heading("Layer", cx)).child(
            h_flex()
                .flex_wrap()
                .gap_1()
                .children(layers.into_iter().map(|(lid, name)| {
                    let on = lid == node.layer;
                    let nid = id.clone();
                    small(SharedString::from(format!("layer-{lid}")), name, on).on_click(
                        cx.listener(move |this, _, _, cx| {
                            let lid = lid.clone();
                            let next = this.system.nodes_in(&lid).len() as f32;
                            this.edit_node(&nid, cx, |n| {
                                n.layer = lid;
                                n.slot = Some(next);
                            });
                        }),
                    )
                }))
                .child(div().w_2())
                .child({
                    let nid = id.clone();
                    IconButton::new("move-left", IconName::ArrowLeft)
                        .tooltip(Tooltip::text("Move left"))
                        .on_click(cx.listener(move |this, _, _, cx| this.shift(&nid, -1, cx)))
                })
                .child({
                    let nid = id.clone();
                    IconButton::new("move-right", IconName::ArrowRight)
                        .tooltip(Tooltip::text("Move right"))
                        .on_click(cx.listener(move |this, _, _, cx| this.shift(&nid, 1, cx)))
                }),
        );

        // Deployment.
        let target = node.deploy.as_ref().map(|d| d.target);
        let replicas = node.deploy.as_ref().and_then(|d| d.replicas).unwrap_or(1);
        col = col
            .child(heading("Runs as", cx))
            .child(h_flex().flex_wrap().gap_1().children(TARGETS.map(|t| {
                let nid = id.clone();
                small(
                    SharedString::from(format!("target-{t:?}")),
                    t.label(),
                    target == Some(t),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.edit_node(&nid, cx, |n| {
                        let d = n.deploy.get_or_insert_with(Default::default);
                        d.target = t;
                        if matches!(
                            t,
                            DeployTarget::Pod | DeployTarget::Container | DeployTarget::Vm
                        ) {
                            d.replicas.get_or_insert(1);
                        } else {
                            d.replicas = None;
                        }
                    })
                }))
            })));
        if matches!(
            target,
            Some(DeployTarget::Pod | DeployTarget::Container | DeployTarget::Vm)
        ) {
            let (a, b) = (id.clone(), id.clone());
            col =
                col.child(
                    h_flex()
                        .pt_1()
                        .gap_2()
                        .child(div().text_size(px(12.)).child("Replicas"))
                        .child(IconButton::new("replicas-down", IconName::Dash).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.edit_node(&a, cx, |n| {
                                    if let Some(d) = n.deploy.as_mut() {
                                        d.replicas =
                                            Some(d.replicas.unwrap_or(1).saturating_sub(1).max(1));
                                    }
                                })
                            }),
                        ))
                        .child(div().text_size(px(12.)).child(replicas.to_string()))
                        .child(IconButton::new("replicas-up", IconName::Plus).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.edit_node(&b, cx, |n| {
                                    if let Some(d) = n.deploy.as_mut() {
                                        d.replicas = Some(d.replicas.unwrap_or(1) + 1);
                                    }
                                })
                            }),
                        )),
                );
        }

        // Connections.
        let edges: Vec<_> = self
            .system
            .edges
            .iter()
            .filter(|e| e.from == node.id || e.to == node.id)
            .cloned()
            .collect();
        col = col.child(heading("Connections", cx));
        if edges.is_empty() {
            col = col.child(para("None. Use Connect in the toolbar to add one.", cx));
        }
        for (i, e) in edges.into_iter().enumerate() {
            let out = e.from == node.id;
            let other = if out { &e.to } else { &e.from };
            let (from, to) = (e.from.clone(), e.to.clone());
            col = col.child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .w(px(10.))
                            .h(px(2.))
                            .bg(crate::canvas::edge_color(e.style, cx)),
                    )
                    .child(div().flex_1().truncate().text_size(px(12.)).child(format!(
                                "{} {}{}",
                                if out { "→" } else { "←" },
                                self.node_name(other),
                                e.label.as_ref().map(|l| format!(" ({l})")).unwrap_or_default()
                            )))
                    .child(
                        IconButton::new(("remove-edge", i), IconName::Close)
                            .icon_size(IconSize::XSmall)
                            .tooltip(Tooltip::text("Remove connection"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let style = this.edge_style;
                                this.edit(cx, |s| s.toggle_edge(&from, &to, style));
                            })),
                    ),
            );
        }

        // What it serves.
        let apis: Vec<_> = self
            .system
            .apis
            .iter()
            .filter(|a| a.service == node.id)
            .map(|a| format!("{} {}", a.method, a.path))
            .chain(
                self.system
                    .sockets
                    .iter()
                    .filter(|s| s.service == node.id)
                    .map(|s| s.channel.clone()),
            )
            .collect();
        if !apis.is_empty() {
            col = col.child(heading("Serves", cx)).child(
                v_flex()
                    .gap_1()
                    .children(apis.into_iter().map(|a| chip(a, cx))),
            );
        }

        col.child(div().h_3())
            .child(
                Button::new("delete-node", "Delete component")
                    .size(ButtonSize::Compact)
                    .style(ButtonStyle::Tinted(ferrum_ui::components::TintColor::Error))
                    .on_click(cx.listener(|this, _, _, cx| this.delete_selected(cx))),
            )
            .into_any_element()
    }
}

const SWATCHES: [u32; 10] = [
    0x6366f1, 0x0ea5e9, 0x22c55e, 0xa855f7, 0xec4899, 0xeab308, 0x14b8a6, 0xef4444, 0x8b8b90,
    0xf1e9df,
];
const FILLS: [u32; 6] = [0x1f1d24, 0x16213a, 0x13261d, 0x2a1830, 0x2b2414, 0xf4f4f5];

impl FerrumSystem {
    /// Style: colour, fill, borders (each side), radius, texts, icon.
    fn style_section(
        &mut self,
        mut col: gpui::Div,
        node: &Node,
        fields: &NodeFields,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let colors = cx.theme().colors().clone();
        let st = node.style.clone();
        let id = node.id.clone();
        let swatch = |key: String,
                      c: Option<u32>,
                      on: bool,
                      cx: &mut Context<Self>,
                      set: Box<dyn Fn(&mut Node)>| {
            let nid = id.clone();
            let set = std::rc::Rc::new(set);
            div()
                .id(SharedString::from(key))
                .size(px(20.))
                .rounded_full()
                .bg(c
                    .map(crate::icons::rgb)
                    .unwrap_or(colors.text.opacity(0.08)))
                .border_2()
                .border_color(if on { colors.text } else { colors.border })
                .cursor_pointer()
                .when(c.is_none(), |d| {
                    d.child(div().text_size(px(9.)).flex().justify_center().child("—"))
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    let set = set.clone();
                    this.edit_node(&nid, cx, move |n| set(n))
                }))
        };
        col = col.child(heading("Style", cx)).child(
            div()
                .text_size(px(11.))
                .text_color(colors.text_muted)
                .child("Colour"),
        );
        let mut row = h_flex().flex_wrap().gap_1p5();
        row = row.child(swatch(
            "c-none".into(),
            None,
            st.color.is_none(),
            cx,
            Box::new(|n| n.style.color = None),
        ));
        for c in SWATCHES {
            row = row.child(swatch(
                format!("c-{c:06x}"),
                Some(c),
                st.color == Some(c),
                cx,
                Box::new(move |n| n.style.color = Some(c)),
            ));
        }
        col = col.child(row).child(
            div()
                .pt_1()
                .text_size(px(11.))
                .text_color(colors.text_muted)
                .child("Fill"),
        );
        let mut row = h_flex().flex_wrap().gap_1p5();
        row = row.child(swatch(
            "f-none".into(),
            None,
            st.fill.is_none(),
            cx,
            Box::new(|n| n.style.fill = None),
        ));
        for c in FILLS {
            row = row.child(swatch(
                format!("f-{c:06x}"),
                Some(c),
                st.fill == Some(c),
                cx,
                Box::new(move |n| n.style.fill = Some(c)),
            ));
        }
        col = col.child(row);

        // Border: on/off, then each side's width.
        let border_on = st.border.unwrap_or(true);
        let widths = st.border_widths.unwrap_or([1.; 4]);
        let nid = id.clone();
        col = col.child(
            h_flex()
                .pt_1()
                .gap_2()
                .child(div().text_size(px(12.)).child("Border"))
                .child(
                    small(
                        "border-toggle",
                        if border_on { "On" } else { "Off" },
                        border_on,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_node(&nid, cx, |n| {
                            n.style.border = Some(!n.style.border.unwrap_or(true))
                        })
                    })),
                ),
        );
        if border_on {
            for (i, side) in ["Top", "Right", "Bottom", "Left"].into_iter().enumerate() {
                let (a, b) = (id.clone(), id.clone());
                let step = move |n: &mut Node, by: f32| {
                    let mut w = n.style.border_widths.unwrap_or([1.; 4]);
                    w[i] = (w[i] + by).clamp(0., 8.);
                    n.style.border_widths = Some(w);
                };
                col = col.child(
                    h_flex()
                        .gap_2()
                        .child(
                            div()
                                .w(px(60.))
                                .text_size(px(12.))
                                .text_color(colors.text_muted)
                                .child(side),
                        )
                        .child(
                            IconButton::new(("bw-down", i), IconName::Dash)
                                .icon_size(IconSize::XSmall)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.edit_node(&a, cx, |n| step(n, -1.))
                                })),
                        )
                        .child(
                            div()
                                .w(px(30.))
                                .text_size(px(12.))
                                .child(format!("{} px", widths[i])),
                        )
                        .child(
                            IconButton::new(("bw-up", i), IconName::Plus)
                                .icon_size(IconSize::XSmall)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.edit_node(&b, cx, |n| step(n, 1.))
                                })),
                        ),
                );
            }
        }
        let radius = st.radius.unwrap_or(8.);
        let (a, b) = (id.clone(), id.clone());
        col = col.child(
            h_flex()
                .gap_2()
                .child(
                    div()
                        .w(px(60.))
                        .text_size(px(12.))
                        .text_color(colors.text_muted)
                        .child("Radius"),
                )
                .child(
                    IconButton::new("radius-down", IconName::Dash)
                        .icon_size(IconSize::XSmall)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit_node(&a, cx, |n| {
                                n.style.radius = Some((n.style.radius.unwrap_or(8.) - 2.).max(0.))
                            })
                        })),
                )
                .child(
                    div()
                        .w(px(30.))
                        .text_size(px(12.))
                        .child(format!("{radius} px")),
                )
                .child(
                    IconButton::new("radius-up", IconName::Plus)
                        .icon_size(IconSize::XSmall)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit_node(&b, cx, |n| {
                                n.style.radius = Some((n.style.radius.unwrap_or(8.) + 2.).min(28.))
                            })
                        })),
                ),
        );
        let show_icon = st.icon.unwrap_or(true);
        let nid = id.clone();
        col = col
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .w(px(60.))
                            .text_size(px(12.))
                            .text_color(colors.text_muted)
                            .child("Icon"),
                    )
                    .child(
                        small(
                            "icon-toggle",
                            if show_icon { "Shown" } else { "Hidden" },
                            show_icon,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit_node(&nid, cx, |n| {
                                n.style.icon = Some(!n.style.icon.unwrap_or(true))
                            })
                        })),
                    ),
            )
            .child(
                div()
                    .pt_1()
                    .text_size(px(11.))
                    .text_color(colors.text_muted)
                    .child("Text above the title"),
            )
            .child(fields.above.clone())
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(colors.text_muted)
                    .child("Text below the title"),
            )
            .child(fields.below.clone())
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(colors.text_muted)
                    .child("Text inside"),
            )
            .child(fields.body.clone());
        let nid = id.clone();
        col.child(
            small("style-reset", "Reset style", false).on_click(cx.listener(
                move |this, _, _, cx| {
                    this.fields = None;
                    this.edit_node(&nid, cx, |n| n.style = Default::default())
                },
            )),
        )
    }
}
