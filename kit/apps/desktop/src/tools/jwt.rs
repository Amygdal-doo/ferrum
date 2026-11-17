//! JWT decoder (no signature verification).

use super::{layout, split};
use crate::widgets::*;
use chrono::{Local, Utc};
use ferrum_ui::prelude::*;
use ferrumkit_core::jwt::{self, DecodedJwt, Validity};
use ferrumkit_core::timestamp::relative;

pub struct JwtTool {
    token: Entity<TextInput>,
    header: Entity<TextInput>,
    payload: Entity<TextInput>,
    decoded: Option<Result<DecodedJwt, String>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<KitEvent> for JwtTool {}

impl JwtTool {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let token = editor(
            "Paste a JWT (eyJ…), with or without “Bearer ”",
            4,
            window,
            cx,
        );
        let header = output(6, window, cx);
        let payload = output(12, window, cx);
        let subscriptions = vec![on_change(&token, cx, Self::run)];
        Self {
            token,
            header,
            payload,
            decoded: None,
            _subscriptions: subscriptions,
        }
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let token = text_of(&self.token, cx);
        if token.trim().is_empty() {
            self.decoded = None;
            set_text(&self.header, "", cx);
            set_text(&self.payload, "", cx);
            return;
        }
        let decoded = jwt::decode(&token);
        let (header, payload) = match &decoded {
            Ok(d) => (d.header_pretty.clone(), d.payload_pretty.clone()),
            Err(_) => (String::new(), String::new()),
        };
        set_text(&self.header, header, cx);
        set_text(&self.payload, payload, cx);
        self.decoded = Some(decoded);
    }

    fn render_summary(&self, decoded: &DecodedJwt, cx: &mut Context<Self>) -> impl IntoElement {
        let status = cx.theme().status().clone();
        let (label, color) = match decoded.validity(Utc::now()) {
            Validity::Valid => ("Not expired", status.success),
            Validity::Expired => ("Expired", status.error),
            Validity::NotYetValid => ("Not yet valid (nbf)", status.warning),
            Validity::NoExpiry => ("No exp claim", status.info),
        };
        let now = Utc::now();
        let claims: Vec<_> = decoded
            .time_claims
            .iter()
            .map(|claim| {
                let value = match claim.utc {
                    Some(date) => format!(
                        "{}  ·  {}  ·  {}  ({})",
                        claim.seconds,
                        date.format("%Y-%m-%d %H:%M:%S UTC"),
                        date.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S %:z"),
                        relative(date, now)
                    ),
                    None => format!("{} (out of range)", claim.seconds),
                };
                let name = match claim.name {
                    "exp" => "exp · expires",
                    "iat" => "iat · issued at",
                    _ => "nbf · not before",
                };
                value_row(claim.name, name, value, cx).into_any_element()
            })
            .collect();
        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_2()
                    .child(badge(label, color, cx))
                    .when_some(decoded.algorithm.clone(), |this, alg| {
                        this.child(badge(format!("alg {alg}"), status.info, cx))
                    }),
            )
            .children(claims)
    }
}

impl Render for JwtTool {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let warning = cx.theme().status().warning;
        let notice = h_flex()
            .gap_2()
            .child(Icon::new(IconName::Warning).size(IconSize::Small).color(Color::Warning))
            .child(
                div()
                    .text_sm()
                    .text_color(warning)
                    .child("Decode only: the signature is NOT verified. Never trust a token's claims based on this view."),
            );
        let summary = match &self.decoded {
            Some(Ok(decoded)) => Some(self.render_summary(&decoded.clone(), cx).into_any_element()),
            Some(Err(error)) => Some(status_line(Err(error.clone()), cx).into_any_element()),
            None => None,
        };
        let header_text = text_of(&self.header, cx);
        let payload_text = text_of(&self.payload, cx);
        layout(
            v_flex()
                .gap_2()
                .child(notice)
                .child(
                    div()
                        .id("jwt-token")
                        .max_h(px(120.))
                        .overflow_y_scroll()
                        .child(self.token.clone()),
                )
                .children(summary),
            split(
                panel(
                    "jwt-header",
                    "Header",
                    Some(copy_button("jwt-copy-header", header_text, cx).into_any_element()),
                    self.header.clone(),
                ),
                panel(
                    "jwt-payload",
                    "Payload",
                    Some(copy_button("jwt-copy-payload", payload_text, cx).into_any_element()),
                    self.payload.clone(),
                ),
            ),
        )
    }
}

impl super::KitTool for JwtTool {
    fn inputs(&self) -> Vec<Entity<TextInput>> {
        vec![self.token.clone()]
    }

    fn outputs(&self) -> Vec<Entity<TextInput>> {
        vec![self.header.clone(), self.payload.clone()]
    }

    fn rerun(&mut self, cx: &mut Context<Self>) {
        self.run(cx);
        cx.notify();
    }
}
