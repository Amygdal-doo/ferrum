//! Request editor: tab bar, URL bar, sub-tabs.

mod body;
mod form_data;
mod kinds;
mod method_picker;
mod scripts;
mod sections;
mod settings;
mod tab_bar;
mod url_bar;

use crate::app::FerrumApi;
use ferrum_ui::components::{ActiveTheme as _, StyledTypography as _, h_flex, v_flex};
use ferrumapi_application::http::unresolved_variables;
use gpui::{AnyElement, Context, Window, div, prelude::*};

impl FerrumApi {
    pub fn render_request_pane(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tab_bar = self.render_tab_bar(window, cx);
        let Some(tab) = self.state.active_tab().cloned() else {
            return v_flex().size_full().child(tab_bar).into_any_element();
        };
        let prepared = self.state.prepare_tab(&tab);
        let vars = prepared.variables.clone();
        let unresolved = unresolved_variables(&prepared.request, &vars).join(", ");
        let inherited = (tab.request.auth.is_inherit() && tab.origin.is_some())
            .then(|| prepared.request.auth.kind_label().to_owned());
        let secrets = self.state.secret_keys(&tab);
        let loading = self.state.is_loading(tab.id);

        let url_bar = self.render_url_bar(&tab, &vars, secrets, loading, window, cx);
        let section_tabs = self.render_section_tabs(&tab, cx);
        let section_body = self.render_section_body(&tab, &vars, inherited, window, cx);
        let status = cx.theme().status().clone();

        v_flex()
            .size_full()
            .child(tab_bar)
            .child(url_bar)
            .when(!unresolved.is_empty(), |this| {
                this.child(
                    h_flex()
                        .mx_3()
                        .mt_2()
                        .px_3()
                        .py_1p5()
                        .gap_1()
                        .rounded_md()
                        .bg(status.warning_background)
                        .text_color(status.warning)
                        .text_xs()
                        .child("Unresolved variables:")
                        .child(div().font_buffer(cx).child(unresolved))
                        .child(
                            "— define them in the active environment, the collection or globals.",
                        ),
                )
            })
            .child(section_tabs)
            .child(section_body)
            .into_any_element()
    }
}
