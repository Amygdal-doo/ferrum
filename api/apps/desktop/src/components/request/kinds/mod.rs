//! Editors of the non-REST kinds' own sections.

mod graphql;
mod graphql_schema;
mod grpc;
mod odata;
mod soap;
mod trpc;
mod websocket;

use crate::app::FerrumApi;
use crate::components::fields::{FieldSpec, on_request_edit};
use crate::components::hint;
use crate::state::RequestSection;
use ferrum_ui::TextInput;
use ferrum_ui::components::{
    Button, ButtonCommon as _, ButtonSize, ButtonStyle, Clickable as _, Color, Label,
    LabelCommon as _, LabelSize, Tooltip, h_flex, v_flex,
};
use ferrumapi_domain::text::pretty_json;
use ferrumapi_domain::{HttpRequest, RequestKind, RequestTab, TabId, VariableSet};
use gpui::{AnyElement, Context, Entity, SharedString, Window, div, prelude::*};
use std::rc::Rc;

/// Writes edited text into the request.
pub type SpecSetter = Rc<dyn Fn(&mut HttpRequest, String)>;

pub fn setter(f: impl Fn(&mut HttpRequest, String) + 'static) -> SpecSetter {
    Rc::new(f)
}

/// A labelled row: small muted label, then the control.
pub fn labelled(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap_1()
        .child(
            Label::new(label)
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        )
        .child(control)
}

impl FerrumApi {
    pub(super) fn render_kind_section(
        &mut self,
        tab: &RequestTab,
        section: RequestSection,
        vars: &VariableSet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let _ = vars;
        match (section, tab.request.kind) {
            (RequestSection::Query, _) => self.render_graphql_query(tab, window, cx),
            (RequestSection::Variables, _) => self.render_graphql_variables(tab, window, cx),
            (RequestSection::Envelope, _) => self.render_soap(tab, window, cx),
            (RequestSection::Builder, _) => self.render_odata(tab, window, cx),
            (RequestSection::Procedure, _) => self.render_trpc(tab, window, cx),
            (RequestSection::Message, RequestKind::WebSocket) => {
                self.render_ws_composer(tab, window, cx)
            }
            (RequestSection::Message, _) => self.render_grpc(tab, window, cx),
            _ => hint("Nothing to edit here.").into_any_element(),
        }
    }

    /// A text field bound to a spec value of tab `tab_id`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn spec_field(
        &mut self,
        tab_id: TabId,
        key: &str,
        value: &str,
        placeholder: &'static str,
        lines: Option<usize>,
        set: SpecSetter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        let mut spec = FieldSpec::new(format!("{key}:{tab_id}"))
            .value(value.to_owned())
            .placeholder(placeholder);
        if let Some(lines) = lines {
            spec = spec.multiline(lines).mono();
        }
        self.field(
            spec,
            on_request_edit(move |this, text, _| {
                let set = set.clone();
                this.state.edit_request(tab_id, move |r| set(r, text));
            }),
            window,
            cx,
        )
    }

    /// A multi-line JSON editor with a Beautify button.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn json_editor(
        &mut self,
        tab_id: TabId,
        key: &'static str,
        value: &str,
        placeholder: &'static str,
        lines: usize,
        set: SpecSetter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let editor = self.spec_field(
            tab_id,
            key,
            value,
            placeholder,
            Some(lines),
            set.clone(),
            window,
            cx,
        );
        let text = value.to_owned();
        v_flex()
            .gap_1()
            .child(
                h_flex().child(
                    Button::new(SharedString::from(format!("beautify-{key}")), "Beautify")
                        .style(ButtonStyle::Subtle)
                        .size(ButtonSize::Compact)
                        .tooltip(Tooltip::text("Pretty-print the JSON"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            match pretty_json(&text) {
                                Some(pretty) => {
                                    let set = set.clone();
                                    this.state.edit_request(tab_id, move |r| set(r, pretty));
                                }
                                None => this.notify("Not valid JSON.", true, cx),
                            }
                            cx.notify();
                        })),
                ),
            )
            .child(div().min_h_32().child(editor))
            .into_any_element()
    }
}

/// Small "Load" / "Reload" button.
pub fn load_button(
    id: impl Into<SharedString>,
    loaded: bool,
    tooltip: &'static str,
    cx: &mut Context<FerrumApi>,
    on_click: impl Fn(&mut FerrumApi, &mut Context<FerrumApi>) + 'static,
) -> AnyElement {
    Button::new(id.into(), if loaded { "Reload" } else { "Load" })
        .style(ButtonStyle::Outlined)
        .size(ButtonSize::Compact)
        .tooltip(Tooltip::text(tooltip))
        .on_click(cx.listener(move |this, _, _, cx| {
            on_click(this, cx);
            cx.notify();
        }))
        .into_any_element()
}
