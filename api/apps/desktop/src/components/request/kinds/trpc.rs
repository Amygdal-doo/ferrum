//! tRPC: procedure path, query / mutation, batch toggle, JSON input.

use super::{labelled, setter};
use crate::app::FerrumApi;
use crate::components::{checkbox, hint, select};
use ferrum_ui::components::{ActiveTheme as _, StyledTypography as _, h_flex, v_flex};
use ferrumapi_domain::RequestTab;
use ferrumapi_domain::protocol::TrpcType;
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*};

impl FerrumApi {
    pub(super) fn render_trpc(
        &mut self,
        tab: &RequestTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = tab.id;
        let spec = tab.request.specs.trpc.clone();
        let procedure = self.spec_field(
            id,
            "trpc-proc",
            &spec.procedure,
            "user.byId",
            None,
            setter(|r, t| r.specs.trpc.procedure = t),
            window,
            cx,
        );
        let kind = select(
            SharedString::from(format!("trpc-type-{id}")),
            spec.procedure_type.label(),
            TrpcType::ALL
                .into_iter()
                .map(|t| (SharedString::from(t.label()), t))
                .collect(),
            move |this, t, _| {
                this.state
                    .edit_request(id, |r| r.specs.trpc.procedure_type = t)
            },
            window,
            cx,
        );
        let batch = checkbox(
            SharedString::from(format!("trpc-batch-{id}")),
            spec.batch,
            move |this, on, _| this.state.edit_request(id, |r| r.specs.trpc.batch = on),
            cx,
        );
        let input = self.json_editor(
            id,
            "trpc-input",
            &spec.input,
            "{ \"id\": 1 }  (blank = no input)",
            10,
            setter(|r, t| r.specs.trpc.input = t),
            window,
            cx,
        );
        let sends = match ferrumapi_domain::protocol::lower(&tab.request) {
            Ok(r) => format!("{} {}", r.method, r.url),
            Err(e) => e.to_string(),
        };
        let colors = cx.theme().colors().clone();
        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .gap_3()
                    .items_end()
                    .child(div().flex_1().child(labelled("Procedure", procedure)))
                    .child(labelled("Type", kind))
                    .child(h_flex().gap_1().child(batch).child(hint("Batch"))),
            )
            .child(labelled("Input (JSON)", input))
            .child(labelled(
                "Sends",
                div()
                    .text_xs()
                    .font_buffer(cx)
                    .text_color(colors.text_muted)
                    .child(sends),
            ))
            .child(hint(
                "The response view's Data tab shows result.data unwrapped. Transformers such as superjson are unwrapped when the data is a { json } envelope; inputs are sent as plain JSON.",
            ))
            .into_any_element()
    }
}
