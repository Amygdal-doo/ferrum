//! Cookie jar viewer: cookies grouped by domain, delete one, clear a domain
//! or everything.

use super::modal;
use crate::app::FerrumApi;
use crate::components::hint;
use crate::components::widgets::{header_cell, row_border};
use ferrumapi_domain::{CookieJar, StoredCookie};
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px, relative};
use ui::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonSize, ButtonStyle, Clickable as _,
    Disableable as _, IconButton, IconName, IconSize, StyledTypography as _, TintColor, Tooltip,
    h_flex, v_flex,
};

fn expires_label(c: &StoredCookie) -> String {
    c.expires.map_or_else(
        || "Session".to_owned(),
        |at| at.format("%Y-%m-%d %H:%M UTC").to_string(),
    )
}

fn flags(c: &StoredCookie) -> String {
    let mut out = Vec::new();
    if c.secure {
        out.push("Secure");
    }
    if c.http_only {
        out.push("HttpOnly");
    }
    if !c.host_only {
        out.push("Subdomains");
    }
    out.join(" · ")
}

impl FerrumApi {
    pub(super) fn render_cookies_dialog(
        &mut self,
        jar: CookieJar,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let border = row_border(cx);
        let mut body = v_flex().gap_3().child(hint(
            "Cookies set by responses are stored here and sent with later requests to matching domains and paths. A Cookie header you set on a request wins.",
        ));
        if jar.is_empty() {
            body = body.child(div().py_4().child(hint("The cookie jar is empty.")));
        }
        for domain in jar.domains() {
            let cookies: Vec<&StoredCookie> = jar
                .cookies()
                .iter()
                .filter(|c| c.domain == domain)
                .collect();
            let clear_domain = domain.clone();
            let header = h_flex()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(domain.clone()),
                )
                .child(
                    Button::new(
                        SharedString::from(format!("clear-domain-{domain}")),
                        "Clear",
                    )
                    .style(ButtonStyle::Subtle)
                    .size(ButtonSize::Compact)
                    .tooltip(Tooltip::text("Delete every cookie of this domain"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.clear_cookies(Some(clear_domain.clone()), cx);
                    })),
                );
            let columns = h_flex()
                .border_b_1()
                .border_color(border)
                .child(div().w(relative(0.2)).child(header_cell("Name")))
                .child(div().flex_1().child(header_cell("Value")))
                .child(div().w(relative(0.12)).child(header_cell("Path")))
                .child(div().w(relative(0.2)).child(header_cell("Expires")))
                .child(div().w(px(28.)));
            let rows = cookies.into_iter().map(|c| {
                let key = (c.domain.clone(), c.path.clone(), c.name.clone());
                let id = SharedString::from(format!("delete-cookie-{}-{}-{}", key.0, key.1, key.2));
                let cell = |text: String| {
                    div()
                        .px_3()
                        .py_1()
                        .text_xs()
                        .font_buffer(cx)
                        .truncate()
                        .child(text)
                };
                h_flex()
                    .border_b_1()
                    .border_color(border)
                    .child(div().w(relative(0.2)).min_w_0().child(cell(c.name.clone())))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(cell(c.value.clone()))
                            .child(
                                div()
                                    .px_3()
                                    .pb_1()
                                    .text_xs()
                                    .text_color(colors.text_muted)
                                    .child(flags(c)),
                            ),
                    )
                    .child(
                        div()
                            .w(relative(0.12))
                            .min_w_0()
                            .child(cell(c.path.clone())),
                    )
                    .child(
                        div()
                            .w(relative(0.2))
                            .min_w_0()
                            .child(cell(expires_label(c))),
                    )
                    .child(
                        div().w(px(28.)).child(
                            IconButton::new(id, IconName::Close)
                                .icon_size(IconSize::XSmall)
                                .tooltip(Tooltip::text("Delete cookie"))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let (domain, path, name) = key.clone();
                                    this.remove_cookie(domain, path, name, cx);
                                })),
                        ),
                    )
            });
            body = body.child(v_flex().gap_1().child(header).child(columns).children(rows));
        }

        let empty = jar.is_empty();
        modal("Cookies", px(768.), cx)
            .child(body)
            .footer(
                Button::new("cookies-clear-all", "Clear all")
                    .style(ButtonStyle::Tinted(TintColor::Error))
                    .disabled(empty)
                    .on_click(cx.listener(|this, _, _, cx| this.clear_cookies(None, cx))),
            )
            .footer(
                Button::new("cookies-close", "Close")
                    .style(ButtonStyle::Filled)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.dialog = None;
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}
