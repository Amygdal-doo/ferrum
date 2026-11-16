//! The window: sidebar, the diagram card (header, the toolbar with the
//! cloud switch, then the canvas — or, with nothing to show, the empty
//! view) and the panel card, over the backdrop. The sidebar (⌘B) and the
//! panel (⌥⌘B) each fold away; the canvas takes their room.

use ferrum_ui::prelude::*;
use ferrum_ui::settings::widgets::Segmented;
use ferrumsystem_core::{EdgeStyle, Provider};

use crate::app::{
    DeleteSelected, Escape, FerrumSystem, KEY_CONTEXT, NewDiagram, OpenDemo, OpenProject,
    TogglePanel, ToggleSidebar, ZoomFit, ZoomIn, ZoomOut,
};

const PANEL_W: f32 = 430.;
const HEADER_H: f32 = 52.;
const TOOLBAR_H: f32 = 44.;

impl FerrumSystem {
    fn render_header(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let empty = self.is_empty();
        let (title, tagline): (SharedString, SharedString) = if empty {
            ("FerrumSystem".into(), "No diagram open".into())
        } else {
            (
                self.system.name.clone().into(),
                self.system.tagline.clone().into(),
            )
        };
        ferrum_ui::title_row("diagram-header", !self.sidebar_open(), window)
            .h(px(HEADER_H))
            .gap_2()
            .border_b_1()
            .border_color(colors.border_variant)
            .when(!self.sidebar_open(), |row| {
                row.child(self.title_controls(cx))
            })
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .px_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .truncate()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .when(self.is_demo(), |r| {
                                r.child(
                                    div()
                                        .px_1p5()
                                        .rounded(px(4.))
                                        .bg(colors.text.opacity(0.08))
                                        .text_size(px(11.))
                                        .text_color(colors.text_muted)
                                        .child("Demo"),
                                )
                            }),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.))
                            .text_color(colors.text_muted)
                            .child(tagline),
                    ),
            )
            .when(!empty, |row| row.child(self.header_buttons(cx)))
            // The Ferrum account (Sign in / avatar) at the top right.
            .children(ferrum_account_ui::account_button(cx))
    }

    /// The header's right end: the tools toggle and the ◨ panel toggle.
    fn header_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self.panel_open();
        h_flex()
            .flex_none()
            .gap_0p5()
            .child(
                ferrum_ui::title_button(
                    "toggle-toolbar",
                    IconName::Settings,
                    if self.show_toolbar {
                        "Hide tools"
                    } else {
                        "Show tools"
                    },
                )
                .toggle_state(self.show_toolbar)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show_toolbar = !this.show_toolbar;
                    cx.notify();
                })),
            )
            .child(
                ferrum_ui::panel_toggle("toggle-panel", open)
                    .tooltip(Tooltip::text(if open {
                        "Hide panel: overview, APIs, data… (⌥⌘B)"
                    } else {
                        "Show panel: overview, APIs, data… (⌥⌘B)"
                    }))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_panel(cx))),
            )
    }

    /// Instead of the canvas when nothing is shown: start a diagram, open a
    /// project, or look at the demo.
    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let text = if self.project.is_some() {
            "This project has no diagram yet. Start one: it is kept in .architecture/ inside the project."
        } else {
            "Open a project folder to see its diagram, or start a new one."
        };
        v_flex()
            .id("empty-state")
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .p_6()
            .child(
                Icon::new(IconName::Blocks)
                    .size(IconSize::Medium)
                    .color(Color::Muted),
            )
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("No diagram open"),
            )
            .child(
                div()
                    .max_w(px(360.))
                    .text_center()
                    .text_size(px(13.))
                    .text_color(colors.text_muted)
                    .child(text),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("empty-new-diagram", "New diagram")
                            .style(ButtonStyle::Filled)
                            .start_icon(Icon::new(IconName::Plus).size(IconSize::Small))
                            .tooltip(Tooltip::text("New diagram (⌘N)"))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.new_diagram(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("empty-open", "Open…")
                            .style(ButtonStyle::Outlined)
                            .tooltip(Tooltip::text("Open a project folder (⌘O)"))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.open_folder(window, cx)),
                            ),
                    ),
            )
            .child(
                Button::new("empty-open-demo", "Open the demo")
                    .style(ButtonStyle::Subtle)
                    .size(ButtonSize::Compact)
                    .label_size(LabelSize::Small)
                    .on_click(cx.listener(|this, _, _, cx| this.show_demo(0, cx))),
            )
    }

    /// Under the header: the cloud whose service names are shown, then the
    /// editing tools.
    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let provider = self.system.provider;
        let weak = cx.entity().downgrade();
        let mut cloud = Segmented::new(
            "provider",
            Provider::ALL
                .iter()
                .position(|p| *p == provider)
                .unwrap_or(0),
        );
        for p in Provider::ALL {
            cloud = cloud.item(p.label());
        }
        let cloud = cloud.on_select(move |i, _, cx| {
            weak.update(cx, |this, cx| this.set_provider(Provider::ALL[i], cx))
                .ok();
        });
        let connecting = self.connecting.is_some();
        let style = self.edge_style;
        h_flex()
            .flex_none()
            .h(px(TOOLBAR_H))
            .px_3()
            .gap_2()
            .border_b_1()
            .border_color(colors.border_variant)
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(colors.text_muted)
                    .child("Cloud"),
            )
            .child(cloud)
            .child(div().flex_1())
            .when(connecting, |row| {
                row.child(
                    Button::new("edge-style", style.label())
                        .size(ButtonSize::Compact)
                        .style(ButtonStyle::Outlined)
                        .label_size(LabelSize::Small)
                        .tooltip(Tooltip::text(
                            "Kind of the next connection: click to change",
                        ))
                        .on_click(cx.listener(|this, _, _, cx| {
                            let i = EdgeStyle::ALL
                                .iter()
                                .position(|s| *s == this.edge_style)
                                .unwrap_or(0);
                            this.edge_style = EdgeStyle::ALL[(i + 1) % EdgeStyle::ALL.len()];
                            cx.notify();
                        })),
                )
            })
            .child(
                Button::new("connect", "Connect")
                    .size(ButtonSize::Compact)
                    .style(ButtonStyle::Subtle)
                    .toggle_state(connecting)
                    .start_icon(Icon::new(IconName::Link).size(IconSize::Small))
                    .tooltip(Tooltip::text("Click two components to connect them"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.connecting = if this.connecting.is_some() {
                            None
                        } else {
                            Some(None)
                        };
                        cx.notify();
                    })),
            )
            .child(
                Button::new("add-node", "Add")
                    .size(ButtonSize::Compact)
                    .style(ButtonStyle::Filled)
                    .start_icon(Icon::new(IconName::Plus).size(IconSize::Small))
                    .tooltip(Tooltip::text(
                        "Add a component: clouds, languages, databases, AI…",
                    ))
                    .on_click(cx.listener(|this, _, _, cx| this.open_palette(cx))),
            )
    }
}

impl Focusable for FerrumSystem {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for FerrumSystem {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        if self.fit_pending {
            // The canvas's size before it is painted: the window minus the
            // sidebar, the panel, the header rows and the gaps.
            self.fit_pending = false;
            let view = window.viewport_size();
            let side = if self.sidebar_open() { 250. } else { 0. };
            let panel = if self.panel_open() { PANEL_W } else { 0. };
            let w = f32::from(view.width) - side - panel - 30.;
            let h = f32::from(view.height) - HEADER_H - TOOLBAR_H - 20.;
            self.fit_to(w, h);
        }
        let diagram = ferrum_ui::pane_card(true, cx)
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_header(window, cx));
        let diagram = if self.is_empty() {
            diagram.child(div().flex_1().min_h_0().child(self.render_empty(cx)))
        } else {
            diagram
                .when(self.show_toolbar, |c| c.child(self.render_toolbar(cx)))
                .child(div().flex_1().min_h_0().child(self.render_canvas(cx)))
        };
        let panel = (self.panel_open() && !self.is_empty()).then(|| {
            ferrum_ui::pane_card(false, cx)
                .flex_none()
                .w(px(PANEL_W))
                .h_full()
                .overflow_hidden()
                .child(self.render_panel(window, cx))
        });
        let dialog = self.render_dialog(window, cx);
        let notice = self.notice.as_ref().map(|n| {
            ferrum_ui::render_notice(
                n,
                cx.listener(|this, _, _, cx| {
                    this.notice = None;
                    cx.notify();
                }),
                cx,
            )
        });
        ferrum_ui::backdrop(false, cx)
            .relative()
            .size_full()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .text_color(colors.text)
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| this.zoom_by(1.2, cx)))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| this.zoom_by(1. / 1.2, cx)))
            .on_action(cx.listener(|this, _: &ZoomFit, _, cx| this.fit(cx)))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| this.toggle_sidebar(cx)))
            .on_action(cx.listener(|this, _: &TogglePanel, _, cx| this.toggle_panel(cx)))
            .on_action(cx.listener(|this, _: &NewDiagram, window, cx| this.new_diagram(window, cx)))
            .on_action(
                cx.listener(|this, _: &OpenProject, window, cx| this.open_folder(window, cx)),
            )
            .on_action(cx.listener(|this, a: &OpenDemo, _, cx| this.show_demo(a.index, cx)))
            .on_action(cx.listener(|this, _: &DeleteSelected, _, cx| {
                if this.dialog.is_none() && this.fields.is_none() {
                    this.delete_selected(cx)
                }
            }))
            .on_action(cx.listener(|this, _: &Escape, _, cx| {
                if this.dialog.take().is_none() && this.connecting.take().is_none() {
                    this.walk = None;
                    this.select(None, cx);
                }
                cx.notify();
            }))
            .child(
                h_flex()
                    .size_full()
                    .when(self.sidebar_open(), |row| {
                        row.child(self.render_sidebar(window, cx))
                    })
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .gap(ferrum_ui::PANE_GAP)
                            .py(ferrum_ui::PANE_GAP)
                            .pr(ferrum_ui::PANE_GAP)
                            .when(!self.sidebar_open(), |r| r.pl(ferrum_ui::PANE_GAP))
                            .child(diagram)
                            .children(panel),
                    ),
            )
            .children(notice.map(|n| {
                div()
                    .absolute()
                    .bottom_4()
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(n)
            }))
            .children(dialog)
    }
}
