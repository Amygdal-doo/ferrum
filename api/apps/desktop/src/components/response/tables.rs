//! Response headers and cookies tables.

use crate::app::FerrumApi;
use crate::components::widgets::{header_cell, row_border};
use ferrumapi_domain::ResponseCookie;
use gpui::{AnyElement, Context, div, prelude::*, relative};
use ui::{Color, Label, LabelCommon as _, LabelSize, StyledTypography as _, h_flex, v_flex};

fn empty(text: &'static str) -> AnyElement {
    div()
        .p_3()
        .child(Label::new(text).size(LabelSize::Small).color(Color::Muted))
        .into_any_element()
}

fn cell(text: String, bold: bool, cx: &Context<FerrumApi>) -> impl IntoElement {
    div()
        .px_3()
        .py_1()
        .text_xs()
        .font_buffer(cx)
        .when(bold, |this| this.font_weight(gpui::FontWeight::SEMIBOLD))
        .child(text)
}

pub fn headers_table(headers: Vec<(String, String)>, cx: &mut Context<FerrumApi>) -> AnyElement {
    if headers.is_empty() {
        return empty("No headers.");
    }
    let border = row_border(cx);
    v_flex()
        .id("response-headers")
        .size_full()
        .overflow_y_scroll()
        .child(
            h_flex()
                .border_b_1()
                .border_color(border)
                .child(div().w(relative(0.33)).child(header_cell("Name")))
                .child(div().flex_1().child(header_cell("Value"))),
        )
        .children(headers.into_iter().map(|(k, v)| {
            h_flex()
                .items_start()
                .border_b_1()
                .border_color(border)
                .child(div().w(relative(0.33)).child(cell(k, true, cx)))
                .child(div().flex_1().min_w_0().child(cell(v, false, cx)))
        }))
        .into_any_element()
}

pub fn cookies_table(cookies: Vec<ResponseCookie>, cx: &mut Context<FerrumApi>) -> AnyElement {
    if cookies.is_empty() {
        return empty("No cookies were set by this response.");
    }
    let border = row_border(cx);
    let yes = |b: bool| if b { "Yes".to_owned() } else { String::new() };
    let headers = [
        "Name", "Value", "Domain", "Path", "Expires", "HttpOnly", "Secure",
    ];
    v_flex()
        .id("response-cookies")
        .size_full()
        .overflow_y_scroll()
        .child(
            h_flex()
                .border_b_1()
                .border_color(border)
                .children(headers.map(|h| div().flex_1().child(header_cell(h)))),
        )
        .children(cookies.into_iter().map(|c| {
            let values = [
                c.name,
                c.value,
                c.domain.unwrap_or_default(),
                c.path.unwrap_or_default(),
                c.expires.unwrap_or_default(),
                yes(c.http_only),
                yes(c.secure),
            ];
            h_flex()
                .items_start()
                .border_b_1()
                .border_color(border)
                .children(
                    values
                        .into_iter()
                        .enumerate()
                        .map(|(i, v)| div().flex_1().min_w_0().child(cell(v, i == 0, cx))),
                )
        }))
        .into_any_element()
}
