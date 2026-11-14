//! The window: sidebar, the diagram card (header, the toolbar with the
//! cloud switch, then the canvas) and the panel card, over the backdrop.

use ferrum_ui::prelude::*;
use ferrum_ui::settings::widgets::Segmented;
use ferrumsystem_core::{EdgeStyle, Provider};

use crate::app::{DeleteSelected, Escape, FerrumSystem, KEY_CONTEXT, ZoomFit, ZoomIn, ZoomOut};

const PANEL_W: f32 = 430.;
const HEADER_H: f32 = 52.;
const TOOLBAR_H: f32 = 44.;

impl FerrumSystem {
    fn render_header(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        ferrum_ui::title_row("diagram-header", !self.sidebar_open, window)
            .h(px(HEADER_H))
            .gap_2()
            .border_b_1()
            .border_color(colors.border_variant)
            .when(!self.sidebar_open, |row| row.child(self.title_controls(cx)))
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
                                    .child(self.system.name.clone()),
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
                            .child(self.system.tagline.clone()),
                    ),
            )
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
                ferrum_ui::title_button(
                    "toggle-panel",
                    IconName::Info,
                    if self.show_panel {
                        "Hide overview, APIs, data and the rest"
                    } else {
                        "Show overview, APIs, data and the rest"
                    },
                )
                .toggle_state(self.show_panel)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show_panel = !this.show_panel;
                    cx.notify();
                })),
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
            let side = if self.sidebar_open { 250. } else { 0. };
            let panel = if self.show_panel { PANEL_W } else { 0. };
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
            .child(self.render_header(window, cx))
            .when(self.show_toolbar, |c| c.child(self.render_toolbar(cx)))
            .child(div().flex_1().min_h_0().child(self.render_canvas(cx)));
        let panel = self.show_panel.then(|| {
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
                    .when(self.sidebar_open, |row| {
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
                            .when(!self.sidebar_open, |r| r.pl(ferrum_ui::PANE_GAP))
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
