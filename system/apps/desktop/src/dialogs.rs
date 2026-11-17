//! A database's schema, a new layer and a custom technology (dialogs), and
//! the Add tab: the palette of components for the right panel.

use ferrum_ui::prelude::*;
use ferrumsystem_core::catalog::{KINDS, TECHS};
use ferrumsystem_core::{Category, CustomTech, Node};

use crate::app::{Dialog, FerrumSystem};
use crate::icons;

/// The layer a new component of `category` goes to (when it exists).
pub fn default_layer(category: Category) -> &'static str {
    match category {
        Category::Client | Category::Mobile | Category::Frontend => "clients",
        Category::Backend | Category::Custom => "services",
        Category::Database => "data",
        Category::Messaging => "async",
        Category::Ai => "ai",
        Category::Infra => "platform",
        Category::Cloud => "edge",
    }
}

impl FerrumSystem {
    /// Opens the Add tab, into the selected component's layer.
    pub(crate) fn open_palette(&mut self, cx: &mut Context<Self>) {
        let layer = self
            .selected
            .as_ref()
            .and_then(|id| self.system.node(id))
            .map(|n| n.layer.clone())
            .or_else(|| {
                self.system
                    .layers
                    .iter()
                    .find(|l| l.id == self.palette.1)
                    .or(self.system.layers.get(2))
                    .map(|l| l.id.clone())
            })
            .unwrap_or_default();
        self.palette.1 = layer;
        self.set_panel_open(true, cx);
        self.tab = crate::app::Tab::Add;
        cx.notify();
    }

    pub(crate) fn open_layer_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| TextInput::new(window, cx).placeholder("Name, e.g. Edge AI"));
        self.dialog = Some(Dialog::Layer(input, 0x14b8a6));
        cx.notify();
    }

    /// Adds a component of `kind` built with `tech` to `layer` and selects it.
    fn add_node(
        &mut self,
        name: &str,
        kind: &str,
        tech: Option<String>,
        layer: String,
        cx: &mut Context<Self>,
    ) {
        let id = self.system.new_node_id(name);
        let slot = self.system.nodes_in(&layer).len() as f32;
        let node = Node {
            id: id.clone(),
            name: name.to_owned(),
            kind: kind.to_owned(),
            tech,
            layer,
            slot: Some(slot),
            ..Node::default()
        };
        self.edit(cx, |s| s.nodes.push(node));
        self.dialog = None;
        self.select(Some(id), cx);
    }

    pub(crate) fn render_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let close = cx.listener(|this, _, _, cx| {
            this.dialog = None;
            cx.notify();
        });
        Some(match self.dialog.as_ref()? {
            Dialog::Schema(id) => self
                .schema(id.clone(), cx)
                .on_dismiss(close)
                .dismiss_on_backdrop(true)
                .into_any_element(),
            Dialog::Layer(input, color) => {
                let (input, color) = (input.clone(), *color);
                self.layer_dialog(input, color, cx)
                    .on_dismiss(close)
                    .into_any_element()
            }
            Dialog::CustomTech(input) => {
                let input = input.clone();
                self.custom_tech(input, window, cx)
                    .on_dismiss(close)
                    .into_any_element()
            }
            Dialog::Export => self
                .export_dialog(cx)
                .on_dismiss(close)
                .dismiss_on_backdrop(true)
                .into_any_element(),
        })
    }

    fn schema(&self, id: String, cx: &App) -> Modal {
        let colors = cx.theme().colors();
        let Some(db) = self.system.database(&id) else {
            return Modal::new("Schema").child(crate::panel::para(
                "This database has no tables described.",
                cx,
            ));
        };
        let mono = ferrum_ui::theme::theme_settings(cx)
            .buffer_font
            .family
            .clone();
        Modal::new(db.name.clone()).width(px(860.)).child(
            div()
                .id("schema-tables")
                .max_h(px(560.))
                .overflow_y_scroll()
                .child(
                    h_flex()
                        .flex_wrap()
                        .items_start()
                        .gap_3()
                        .children(db.tables.iter().map(|t| {
                            v_flex()
                                .w(px(255.))
                                .rounded_md()
                                .border_1()
                                .border_color(colors.border)
                                .overflow_hidden()
                                .child(
                                    v_flex()
                                        .px_2()
                                        .py_1p5()
                                        .bg(colors.text.opacity(0.06))
                                        .child(
                                            h_flex()
                                                .gap_1p5()
                                                .child(
                                                    Icon::new(IconName::DatabaseZap)
                                                        .size(IconSize::XSmall),
                                                )
                                                .child(
                                                    div()
                                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                                        .child(t.name.clone()),
                                                ),
                                        )
                                        .when(!t.note.is_empty(), |c| {
                                            c.child(
                                                div()
                                                    .text_size(px(11.))
                                                    .text_color(colors.text_muted)
                                                    .child(t.note.clone()),
                                            )
                                        }),
                                )
                                .children(t.columns.iter().map(|c| {
                                    h_flex()
                                        .px_2()
                                        .py_0p5()
                                        .gap_1p5()
                                        .border_t_1()
                                        .border_color(colors.border_variant)
                                        .text_size(px(12.))
                                        .child(div().w(px(14.)).child(if c.primary {
                                            Icon::new(IconName::Lock)
                                                .size(IconSize::XSmall)
                                                .color(Color::Accent)
                                                .into_any_element()
                                        } else if c.references.is_some() {
                                            Icon::new(IconName::Link)
                                                .size(IconSize::XSmall)
                                                .color(Color::Muted)
                                                .into_any_element()
                                        } else {
                                            div().into_any_element()
                                        }))
                                        .child(
                                            div()
                                                .flex_1()
                                                .truncate()
                                                .font_family(mono.clone())
                                                .child(c.name.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_color(colors.text_muted)
                                                .font_family(mono.clone())
                                                .text_size(px(11.))
                                                .child(format!(
                                                    "{}{}",
                                                    c.ty,
                                                    if c.nullable { "?" } else { "" }
                                                )),
                                        )
                                }))
                                .children(
                                    t.columns
                                        .iter()
                                        .filter_map(|c| c.references.as_ref().map(|r| (c, r)))
                                        .map(|(c, r)| {
                                            div()
                                                .px_2()
                                                .py_0p5()
                                                .text_size(px(11.))
                                                .text_color(colors.text_accent)
                                                .child(format!("{} → {r}", c.name))
                                        }),
                                )
                        })),
                ),
        )
    }

    /// The Add tab: categories, the layer to add to, then what to add.
    pub(crate) fn add_tab(&self, cx: &mut Context<Self>) -> AnyElement {
        let (category, layer) = self.palette.clone();
        let colors = cx.theme().colors().clone();
        let tabs = h_flex()
            .flex_wrap()
            .gap_1()
            .children(Category::ALL.map(|c| {
                Button::new(SharedString::from(format!("cat-{}", c.label())), c.label())
                    .size(ButtonSize::Compact)
                    .label_size(LabelSize::Small)
                    .style(if c == category {
                        ButtonStyle::Filled
                    } else {
                        ButtonStyle::Outlined
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let next = default_layer(c);
                        this.palette.0 = c;
                        if this.system.layers.iter().any(|l| l.id == next) {
                            this.palette.1 = next.to_owned();
                        }
                        cx.notify();
                    }))
            }));
        let layer_row = h_flex()
            .flex_wrap()
            .gap_1()
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(colors.text_muted)
                    .child("Add to"),
            )
            .children(self.system.layers.iter().map(|l| {
                let id = l.id.clone();
                Button::new(SharedString::from(format!("to-{}", l.id)), l.name.clone())
                    .size(ButtonSize::Compact)
                    .label_size(LabelSize::Small)
                    .style(if l.id == layer {
                        ButtonStyle::Filled
                    } else {
                        ButtonStyle::Subtle
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.palette.1 = id.clone();
                        cx.notify();
                    }))
            }));

        // Technologies of the category, then the plain kinds.
        let mut items: Vec<(String, String, Option<String>)> = TECHS
            .iter()
            .filter(|t| t.category == category)
            .map(|t| (t.name.to_owned(), t.kind.to_owned(), Some(t.id.to_owned())))
            .collect();
        if category == Category::Custom {
            items.extend(
                self.system
                    .custom_techs
                    .iter()
                    .map(|t| (t.name.clone(), "service".to_owned(), Some(t.id.clone()))),
            );
        }
        items.extend(
            KINDS
                .iter()
                .filter(|k| k.category == category)
                .map(|k| (k.name.to_owned(), k.id.to_owned(), None)),
        );
        let root = self.icon_root().map(|p| p.to_path_buf());
        let grid = h_flex()
            .flex_wrap()
            .gap_1p5()
            .children(
                items
                    .into_iter()
                    .enumerate()
                    .map(|(i, (name, kind, tech))| {
                        let icon = icons::icon(
                            &self.system,
                            root.as_deref(),
                            tech.as_deref(),
                            &kind,
                            px(24.),
                            cx,
                        );
                        let layer = layer.clone();
                        let label = name.clone();
                        h_flex()
                            .id(("palette-item", i))
                            .w(px(186.))
                            .h(px(36.))
                            .px_2()
                            .gap_2()
                            .rounded_md()
                            .border_1()
                            .border_color(colors.border_variant)
                            .cursor_pointer()
                            .hover(|s| s.border_color(colors.text_accent))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.add_node(&name, &kind, tech.clone(), layer.clone(), cx)
                            }))
                            .child(icon)
                            .child(div().flex_1().truncate().text_size(px(12.)).child(label))
                    }),
            );
        let custom = (category == Category::Custom).then(|| {
            Button::new("new-custom-tech", "New technology with your own icon…")
                .size(ButtonSize::Compact)
                .style(ButtonStyle::Outlined)
                .on_click(cx.listener(|this, _, window, cx| {
                    let input = cx.new(|cx| {
                        TextInput::new(window, cx).placeholder("Name, e.g. Acme Billing")
                    });
                    this.dialog = Some(Dialog::CustomTech(input));
                    cx.notify();
                }))
        });
        v_flex()
            .pt_3()
            .gap_3()
            .child(tabs)
            .child(layer_row)
            .children(custom)
            .child(grid)
            .into_any_element()
    }

    fn custom_tech(
        &self,
        input: Entity<TextInput>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Modal {
        let can_save = self.icon_root().is_some();
        Modal::new("New technology")
            .width(px(440.))
            .child(
                v_flex()
                    .gap_2()
                    .child(input.clone())
                    .child(crate::panel::para(
                        if can_save {
                            "Next you choose its icon (PNG, SVG or JPEG). It is kept in .architecture/icons/."
                        } else {
                            "Open a project with an architecture first: custom icons are saved in its .architecture folder."
                        },
                        cx,
                    )),
            )
            .footer(
                Button::new("custom-tech-save", "Choose icon and add")
                    .style(ButtonStyle::Filled)
                    .disabled(!can_save)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let name = input.read(cx).text().trim().to_owned();
                        if name.is_empty() {
                            return;
                        }
                        let id = this.system.new_node_id(&format!("tech {name}"));
                        let tech = CustomTech {
                            id: id.clone(),
                            name: name.clone(),
                            category: Category::Custom,
                            color: 0x8b5cf6,
                        };
                        this.edit(cx, |s| s.custom_techs.push(tech));
                        let layer = if this.system.layers.iter().any(|l| l.id == "services") {
                            "services".to_owned()
                        } else {
                            this.system.layers.first().map(|l| l.id.clone()).unwrap_or_default()
                        };
                        this.add_node(&name, "service", Some(id.clone()), layer, cx);
                        this.choose_icon(id, window, cx);
                    })),
            )
    }

    fn layer_dialog(&self, input: Entity<TextInput>, color: u32, cx: &mut Context<Self>) -> Modal {
        const COLORS: [u32; 8] = [
            0x6366f1, 0x0ea5e9, 0x22c55e, 0xa855f7, 0xec4899, 0xeab308, 0x14b8a6, 0xef4444,
        ];
        let colors = cx.theme().colors().clone();
        Modal::new("New layer")
            .width(px(420.))
            .child(
                v_flex()
                    .gap_2()
                    .child(input.clone())
                    .child(h_flex().gap_1p5().children(COLORS.map(|c| {
                        div()
                            .id(SharedString::from(format!("layer-color-{c:06x}")))
                            .size(px(22.))
                            .rounded_full()
                            .bg(icons::rgb(c))
                            .border_2()
                            .border_color(if c == color {
                                colors.text
                            } else {
                                colors.border
                            })
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(Dialog::Layer(_, chosen)) = this.dialog.as_mut() {
                                    *chosen = c;
                                }
                                cx.notify();
                            }))
                    }))),
            )
            .footer(
                Button::new("layer-add", "Add layer")
                    .style(ButtonStyle::Filled)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let name = input.read(cx).text().trim().to_owned();
                        if name.is_empty() {
                            return;
                        }
                        this.dialog = None;
                        this.edit(cx, |s| {
                            s.add_layer(&name, color);
                        });
                    })),
            )
    }
}
