//! SQL editor pane: tab bar + multi-line monospace input with a line-number
//! gutter. The IDE-grade editor is gated on the ADR-003 spike.

use crate::app::FerrumData;
use ferrum_ui::prelude::*;

impl FerrumData {
    pub fn render_editor_pane(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        v_flex()
            .flex_1()
            .min_h_0()
            .child(self.render_tab_bar(window, cx))
            .child(self.render_editor(window, cx))
            .into_any_element()
    }

    /// The top card's header: tabs, then the connection flags and the
    /// Run / Cancel / Theme tools; ◧ ⌘ + first while the sidebar is hidden.
    pub fn render_tab_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let active = self.state.active_tab;
        let tabs: Vec<AnyElement> = self
            .state
            .tabs
            .iter()
            .map(|tab| {
                let id = tab.id;
                let selected = active == Some(id);
                ferrum_ui::header_tab(SharedString::from(format!("tab-{id}")), selected, cx)
                    .pr_0p5()
                    .text_xs()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.active_tab = Some(id);
                        cx.notify();
                    }))
                    .when(tab.table.is_some(), |this| {
                        this.child(div().text_color(colors.text_muted).child("▦"))
                    })
                    .when(tab.key.is_some(), |this| {
                        this.child(div().text_color(colors.text_muted).child("⚷"))
                    })
                    .child(tab.title.clone())
                    .when(tab.unsaved, |this| {
                        this.child(
                            div()
                                .id(SharedString::from(format!("unsaved-{id}")))
                                .size(px(6.))
                                .rounded_full()
                                .bg(colors.text_muted)
                                .tooltip(Tooltip::text("Not yet saved locally")),
                        )
                    })
                    .child(
                        IconButton::new(SharedString::from(format!("close-{id}")), IconName::Close)
                            .icon_size(IconSize::XSmall)
                            .tooltip(Tooltip::text("Close tab"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.close_tab(id, cx);
                            })),
                    )
                    .into_any_element()
            })
            .collect();

        let sidebar_open = !self.state.sidebar_hidden;
        let controls = (!sidebar_open).then(|| self.render_title_controls(cx));
        let tools = self.render_header_tools(cx);
        ferrum_ui::title_row("tab-bar-row", !sidebar_open, window)
            .gap_1()
            .border_b_1()
            .border_color(colors.border_variant)
            .children(controls)
            .child(
                h_flex()
                    .id("tab-bar")
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .overflow_x_scroll()
                    .children(tabs)
                    .child(
                        ferrum_ui::title_button("new-tab", IconName::Plus, "New tab")
                            .on_click(cx.listener(|this, _, _, cx| this.new_tab(cx))),
                    ),
            )
            .when(!sidebar_open, |row| {
                row.child(
                    div()
                        .flex_none()
                        .max_w(px(240.))
                        .child(self.project_bar.clone()),
                )
            })
            .child(tools)
            .into_any_element()
    }

    fn render_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let Some(tab) = self.state.active_tab() else {
            return div().flex_1().into_any_element();
        };
        let lines = tab.sql.split('\n').count();
        let numbers = (1..=lines)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let id = tab.id;
        let input = self.tab_input(id, false, window, cx);

        // Gutter and input share one scroll container so they stay aligned.
        div()
            .id(SharedString::from(format!("editor-{id}")))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .font_buffer(cx)
            .text_buffer(cx)
            .child(
                h_flex()
                    .items_start()
                    .min_h_full()
                    .child(
                        div()
                            .flex_none()
                            .w(px(48.))
                            .min_h_full()
                            .py_2()
                            .pr_2()
                            .border_r_1()
                            .border_color(colors.border_variant)
                            .text_right()
                            .text_color(colors.editor_line_number)
                            .whitespace_nowrap()
                            .child(numbers),
                    )
                    .child(div().flex_1().min_w_0().p_2().child(input)),
            )
            .into_any_element()
    }
}
