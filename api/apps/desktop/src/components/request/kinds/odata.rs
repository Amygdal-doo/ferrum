//! OData: entity set, query options builder, URL preview, `$metadata`.

use super::{labelled, load_button, setter};
use crate::app::FerrumApi;
use crate::components::{checkbox, hint};
use crate::state::protocols::Loadable;
use ferrum_ui::components::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonSize, ButtonStyle, Clickable as _,
    LabelSize, LoadingLabel, StyledTypography as _, Tooltip, h_flex, v_flex,
};
use ferrumapi_domain::RequestTab;
use ferrumapi_domain::protocol::odata::build_url;
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};

type Setter = fn(&mut ferrumapi_domain::HttpRequest, String);

impl FerrumApi {
    pub(super) fn render_odata(
        &mut self,
        tab: &RequestTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = tab.id;
        let spec = tab.request.specs.odata.clone();
        let rows: [(&str, &'static str, &'static str, &str, Setter); 8] = [
            (
                "odata-set",
                "Entity set / path",
                "People or People('russell')",
                &spec.entity_set,
                |r, t| r.specs.odata.entity_set = t,
            ),
            (
                "odata-filter",
                "$filter",
                "Age gt 30 and startswith(LastName, 'S')",
                &spec.filter,
                |r, t| r.specs.odata.filter = t,
            ),
            (
                "odata-select",
                "$select",
                "FirstName,LastName",
                &spec.select,
                |r, t| r.specs.odata.select = t,
            ),
            (
                "odata-expand",
                "$expand",
                "Friends($select=UserName)",
                &spec.expand,
                |r, t| r.specs.odata.expand = t,
            ),
            (
                "odata-orderby",
                "$orderby",
                "LastName desc",
                &spec.orderby,
                |r, t| r.specs.odata.orderby = t,
            ),
            ("odata-top", "$top", "10", &spec.top, |r, t| {
                r.specs.odata.top = t
            }),
            ("odata-skip", "$skip", "0", &spec.skip, |r, t| {
                r.specs.odata.skip = t
            }),
            (
                "odata-search",
                "$search",
                "free text",
                &spec.search,
                |r, t| r.specs.odata.search = t,
            ),
        ];
        let mut fields: Vec<AnyElement> = Vec::new();
        for (key, label, placeholder, value, set) in rows {
            let input = self.spec_field(id, key, value, placeholder, None, setter(set), window, cx);
            fields.push(labelled_static(label, input).into_any_element());
        }
        let count = checkbox(
            SharedString::from(format!("odata-count-{id}")),
            spec.count,
            move |this, on, _| this.state.edit_request(id, |r| r.specs.odata.count = on),
            cx,
        );
        let preview = build_url(&tab.request.url, &spec);
        let colors = cx.theme().colors().clone();
        let metadata = self.render_odata_metadata(id, cx);
        h_flex()
            .items_start()
            .gap_3()
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .children(fields)
                    .child(h_flex().gap_1().child(count).child(hint("$count=true")))
                    .child(labelled(
                        "Request URL",
                        div()
                            .id(SharedString::from(format!("odata-preview-{id}")))
                            .text_xs()
                            .font_buffer(cx)
                            .text_color(colors.text_muted)
                            .tooltip(Tooltip::text("Built from the service root and the options"))
                            .child(preview),
                    )),
            )
            .child(
                div()
                    .w(px(260.))
                    .flex_none()
                    .border_l_1()
                    .border_color(colors.border)
                    .pl_3()
                    .child(metadata),
            )
            .into_any_element()
    }

    fn render_odata_metadata(
        &mut self,
        id: ferrumapi_domain::TabId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.state.protocols.odata.get(&id).cloned();
        let loaded = matches!(state, Some(Loadable::Ready(_)));
        let body = match state {
            None => hint("Load $metadata to list the service's entity sets.").into_any_element(),
            Some(Loadable::Loading) => LoadingLabel::new("Loading $metadata").into_any_element(),
            Some(Loadable::Failed(e)) => div()
                .text_xs()
                .text_color(cx.theme().status().error)
                .child(e)
                .into_any_element(),
            Some(Loadable::Ready(meta)) => v_flex()
                .id(SharedString::from(format!("odata-sets-{id}")))
                .max_h(px(380.))
                .overflow_y_scroll()
                .gap_0p5()
                .child(hint(format!(
                    "OData {} · {} entity sets",
                    meta.version,
                    meta.entity_sets.len()
                )))
                .children(meta.entity_sets.into_iter().map(|set| {
                    let name = set.name.clone();
                    let props: Vec<String> =
                        set.properties.iter().map(|(n, _)| n.clone()).collect();
                    let tooltip = format!(
                        "{}\nKey: {}\nProperties: {}\nNavigation: {}",
                        set.entity_type,
                        set.keys.join(", "),
                        props.join(", "),
                        set.navigation.join(", ")
                    );
                    Button::new(
                        SharedString::from(format!("odata-set-{name}")),
                        name.clone(),
                    )
                    .style(ButtonStyle::Subtle)
                    .size(ButtonSize::Compact)
                    .label_size(LabelSize::Small)
                    .tooltip(Tooltip::text(tooltip))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let name = name.clone();
                        this.state
                            .edit_request(id, |r| r.specs.odata.entity_set = name);
                        cx.notify();
                    }))
                }))
                .into_any_element(),
        };
        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .child(div().flex_1().child(hint("$metadata")))
                    .child(load_button(
                        format!("odata-meta-load-{id}"),
                        loaded,
                        "Fetch <service root>/$metadata (cached per URL)",
                        cx,
                        move |this, cx| this.load_odata_metadata(id, loaded, cx),
                    )),
            )
            .child(body)
            .into_any_element()
    }
}

fn labelled_static(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    h_flex()
        .gap_3()
        .child(div().w(px(120.)).flex_none().child(hint(label)))
        .child(div().flex_1().min_w_0().child(control))
}
