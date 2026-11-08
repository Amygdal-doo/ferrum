//! Table browser tab: breadcrumb, refresh, one-line SQL bar, full-height grid.

use crate::app::FerrumData;
use ferrum_ui::prelude::*;

impl FerrumData {
    pub fn render_table_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let Some(tab) = self.state.active_tab() else {
            return div().into_any_element();
        };
        let Some(table) = tab.table.clone() else {
            return div().into_any_element();
        };
        let tab_id = tab.id;
        let running = self.state.active_run_in_flight();
        let input = self.tab_input(tab_id, true, window, cx);
        let crumb = |text: String| Label::new(text).size(LabelSize::Small).color(Color::Muted);

        v_flex()
            .flex_1()
            .min_h_0()
            .child(
                h_flex()
                    .flex_none()
                    .gap_3()
                    .px_4()
                    .pt_3()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(crumb("Tables".into()))
                            .when_some(table.schema.clone(), |this, schema| {
                                this.child(crumb("/".into())).child(crumb(schema))
                            })
                            .child(crumb("/".into()))
                            .child(Label::new(table.name.clone()).size(LabelSize::Small)),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("refresh-table", "Refresh")
                            .start_icon(Icon::new(IconName::RotateCw).size(IconSize::Small))
                            .style(ButtonStyle::Outlined)
                            .disabled(running)
                            .tooltip(Tooltip::text("Refresh rows"))
                            .on_click(cx.listener(|this, _, _, cx| this.refresh_table(cx))),
                    ),
            )
            .child(div().flex_none().px_4().py_3().child(input))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .border_t_1()
                    .border_color(colors.border)
                    .child(self.render_results(cx)),
            )
            .into_any_element()
    }
}
