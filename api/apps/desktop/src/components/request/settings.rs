//! Per-request transport settings.

use crate::app::FerrumApi;
use crate::components::fields::{FieldSpec, on_change};
use crate::components::{checkbox, hint};
use ferrumapi_domain::{HttpRequest, RequestKind, TabId};
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*, px};
use ui::{Label, h_flex, v_flex};

impl FerrumApi {
    pub(super) fn render_settings_editor(
        &mut self,
        tab_id: TabId,
        request: &HttpRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = request.settings;
        let kind = request.kind;
        let timeout = self.field(
            FieldSpec::new(format!("timeout:{tab_id}")).initial(settings.timeout_ms.to_string()),
            on_change(move |this, text, _| {
                if let Ok(ms) = text.trim().parse::<u64>() {
                    this.state
                        .edit_request(tab_id, |r| r.settings.timeout_ms = ms);
                }
            }),
            window,
            cx,
        );
        let follow = toggle_row(
            format!("follow-{tab_id}"),
            "Follow redirects",
            "Automatically follow 3xx responses.",
            settings.follow_redirects,
            move |this, on, _| {
                this.state
                    .edit_request(tab_id, |r| r.settings.follow_redirects = on)
            },
            cx,
        );
        let verify = toggle_row(
            format!("verify-{tab_id}"),
            "Verify TLS certificates",
            "Turn off only for local servers with self-signed certificates.",
            settings.verify_tls,
            move |this, on, _| {
                this.state
                    .edit_request(tab_id, |r| r.settings.verify_tls = on)
            },
            cx,
        );
        v_flex()
            .max_w(px(576.))
            .gap_4()
            .child(
                h_flex()
                    .gap_3()
                    .child(div().w_40().flex_none().child(hint("Timeout (ms)")))
                    .child(div().w_40().child(timeout))
                    .child(hint("0 = no timeout")),
            )
            .when(kind.is_http(), |this| this.child(follow))
            .child(verify)
            .when(kind == RequestKind::Grpc, |this| {
                let tls = request.specs.grpc.tls;
                this.child(toggle_row(
                    format!("grpc-tls-{tab_id}"),
                    "Use TLS",
                    "Also implied by a grpcs:// or https:// address.",
                    tls,
                    move |this, on, _| {
                        this.state.edit_request(tab_id, |r| r.specs.grpc.tls = on);
                    },
                    cx,
                ))
            })
            .into_any_element()
    }
}

pub(super) fn toggle_row(
    id: String,
    label: &'static str,
    detail: &'static str,
    on: bool,
    on_change: impl Fn(&mut FerrumApi, bool, &mut Context<FerrumApi>) + 'static,
    cx: &mut Context<FerrumApi>,
) -> AnyElement {
    h_flex()
        .items_start()
        .gap_3()
        .child(checkbox(SharedString::from(id), on, on_change, cx))
        .child(
            v_flex()
                .gap_0p5()
                .child(Label::new(label))
                .child(hint(detail)),
        )
        .into_any_element()
}
