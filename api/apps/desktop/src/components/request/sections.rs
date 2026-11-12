//! The request's sub-tabs: Params | Auth | Headers | Body | Settings for
//! REST, and each kind's own sections (see `RequestSection::for_kind`).

use crate::app::FerrumApi;
use crate::components::widgets::{AuthTarget, AuthView, RowEditor};
use crate::components::{count_pill, tab_button};
use crate::state::RequestSection;
use crate::state::kv::{self, active_count};
use ferrumapi_domain::urlparams::url_with_params;
use ferrumapi_domain::{Auth, RequestBody, RequestTab, VariableSet};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*};
use std::rc::Rc;
use ui::{ActiveTheme as _, h_flex};

fn body_count(body: &RequestBody) -> usize {
    match body {
        RequestBody::None => 0,
        RequestBody::Raw { text, .. } => usize::from(!text.is_empty()),
        RequestBody::UrlEncoded { fields } => active_count(fields),
        RequestBody::FormData { fields } => active_count(fields),
        RequestBody::Binary { path } => usize::from(path.is_some()),
    }
}

impl FerrumApi {
    pub(super) fn render_section_tabs(
        &mut self,
        tab: &RequestTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let r = &tab.request;
        let kind = r.kind;
        let current = RequestSection::effective(self.state.section, kind);
        let auth_on = usize::from(!matches!(r.auth, Auth::None | Auth::Inherit));
        let count = |section: RequestSection| match section {
            RequestSection::Params => active_count(&r.params),
            RequestSection::Auth => auth_on,
            RequestSection::Headers => active_count(&r.headers),
            RequestSection::Body => body_count(&r.body),
            RequestSection::Variables => usize::from(!r.specs.graphql.variables.trim().is_empty()),
            RequestSection::Builder => {
                ferrumapi_domain::protocol::odata::options(&r.specs.odata).len()
            }
            _ => 0,
        };
        h_flex()
            .flex_none()
            .h_9()
            .gap_4()
            .px_3()
            .border_b_1()
            .border_color(colors.border)
            .children(
                RequestSection::for_kind(kind)
                    .iter()
                    .copied()
                    .map(|section| {
                        let n = count(section);
                        tab_button(
                            SharedString::from(format!("section-{section:?}")),
                            section.label(kind),
                            section == current,
                            cx,
                        )
                        .when(n > 0, |this| this.child(count_pill(n, cx)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.state.section = section;
                            cx.notify();
                        }))
                    }),
            )
            .into_any_element()
    }

    pub(super) fn render_section_body(
        &mut self,
        tab: &RequestTab,
        vars: &VariableSet,
        inherited: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = tab.id;
        let r = tab.request.clone();
        let section = RequestSection::effective(self.state.section, r.kind);
        let content = match section {
            RequestSection::Params => {
                let on_edit: RowEditor = Rc::new(move |this, i, edit, _| {
                    this.state.edit_request(id, |r| {
                        kv::apply(&mut r.params, i, edit);
                        r.url = url_with_params(&r.url, &r.params);
                    });
                });
                self.render_kv_table(
                    format!("params:{id}"),
                    r.params,
                    vars,
                    "Query parameter",
                    on_edit,
                    window,
                    cx,
                )
            }
            RequestSection::Headers => {
                let on_edit: RowEditor = Rc::new(move |this, i, edit, _| {
                    this.state
                        .edit_request(id, |r| kv::apply(&mut r.headers, i, edit));
                });
                self.render_kv_table(
                    format!("headers:{id}"),
                    r.headers,
                    vars,
                    "Header",
                    on_edit,
                    window,
                    cx,
                )
            }
            RequestSection::Auth => {
                let target: AuthTarget = Rc::new(move |this, f| {
                    this.state.edit_request(id, |r| f(&mut r.auth));
                });
                self.render_auth_editor(
                    AuthView {
                        auth: r.auth,
                        scope: format!("auth:{id}"),
                        allow_inherit: true,
                        inherited,
                        request_fields: true,
                        target,
                    },
                    window,
                    cx,
                )
            }
            RequestSection::Body => self.render_body_editor(id, r.body, vars, window, cx),
            RequestSection::Settings => self.render_settings_editor(id, &r, window, cx),
            RequestSection::Query
            | RequestSection::Variables
            | RequestSection::Envelope
            | RequestSection::Builder
            | RequestSection::Procedure
            | RequestSection::Message => self.render_kind_section(tab, section, vars, window, cx),
        };
        div()
            .id("request-section")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_3()
            .child(content)
            .into_any_element()
    }
}
