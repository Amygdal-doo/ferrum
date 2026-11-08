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
            .child(self.render_tab_bar(cx))
            .child(self.render_editor(window, cx))
            .into_any_element()
    }

    pub fn render_tab_bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let active = self.state.active_tab;
        let tabs: Vec<AnyElement> = self
            .state
            .tabs
            .iter()
            .map(|tab| {
                let id = tab.id;
                let selected = active == Some(id);
                h_flex()
                    .id(SharedString::from(format!("tab-{id}")))
                    .flex_none()
                    .h(px(32.))
                    .gap_2()
                    .px_3()
                    .rounded_t_md()
                    .text_xs()
                    .when(selected, |this| {
                        this.bg(colors.tab_active_background)
                            .border_x_1()
                            .border_t_1()
                            .border_color(colors.border)
                            .text_color(colors.text)
                    })
                    .when(!selected, |this| {
                        this.text_color(colors.text_muted)
                            .hover(|s| s.text_color(colors.text))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.active_tab = Some(id);
                        cx.notify();
                    }))
                    .when(tab.table.is_some(), |this| {
                        this.child(div().text_color(colors.text_muted).child("▦"))
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

        h_flex()
            .id("tab-bar")
            .flex_none()
            .h(px(36.))
            .items_end()
            .gap_0p5()
            .px_2()
            .overflow_x_scroll()
            .bg(colors.tab_bar_background)
            .border_b_1()
            .border_color(colors.border)
            .children(tabs)
            .child(
                div().mb_1().child(
                    IconButton::new("new-tab", IconName::Plus)
                        .tooltip(Tooltip::text("New tab"))
                        .on_click(cx.listener(|this, _, _, cx| this.new_tab(cx))),
                ),
            )
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
            .bg(colors.editor_background)
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
                            .border_color(colors.border)
                            .bg(colors.editor_gutter_background)
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
