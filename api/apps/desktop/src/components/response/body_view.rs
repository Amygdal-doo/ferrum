//! Response body: Pretty | Raw | Preview.

use crate::app::FerrumApi;
use crate::components::fields::FieldSpec;
use crate::components::hint;
use crate::state::BodyMode;
use ferrumapi_domain::text::pretty_json;
use ferrumapi_domain::{HttpResponse, TabId};
use gpui::{
    AnyElement, Context, Image, ImageFormat, ObjectFit, SharedString, Window, div, img, prelude::*,
};
use std::sync::Arc;
use ui::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonSize, ButtonStyle, Clickable as _,
    LabelSize, Toggleable as _, h_flex, v_flex,
};

/// Large bodies are cut for display (Copy / Save still use the full body).
const MAX_RENDER_CHARS: usize = 400_000;

fn clip(text: String) -> (String, bool) {
    if text.len() <= MAX_RENDER_CHARS {
        return (text, false);
    }
    let mut end = MAX_RENDER_CHARS;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_owned(), true)
}

fn pretty(resp: &HttpResponse) -> String {
    let text = resp.body_text();
    let looks_json = resp.is_json() || text.trim_start().starts_with(['{', '[']);
    if looks_json {
        pretty_json(&text).unwrap_or(text)
    } else {
        text
    }
}

impl FerrumApi {
    pub(super) fn render_body_view(
        &mut self,
        tab_id: TabId,
        response: &HttpResponse,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = self.state.body_mode;
        let modes = [
            (BodyMode::Pretty, "Pretty"),
            (BodyMode::Raw, "Raw"),
            (BodyMode::Preview, "Preview"),
        ];
        let content_type = response.content_type().unwrap_or_default().to_owned();
        let is_image = content_type.starts_with("image/");

        let body = match current {
            BodyMode::Pretty | BodyMode::Raw if is_image => div()
                .p_3()
                .child(hint(format!(
                    "Binary image body ({content_type}). Use Preview to view it."
                )))
                .into_any_element(),
            BodyMode::Pretty | BodyMode::Raw => {
                let text = if current == BodyMode::Pretty {
                    pretty(response)
                } else {
                    response.body_text()
                };
                let (text, clipped) = clip(text);
                let status = cx.theme().status().clone();
                let mode = if current == BodyMode::Pretty {
                    "pretty"
                } else {
                    "raw"
                };
                let view = self.field(
                    FieldSpec::new(format!("response-body:{tab_id}:{mode}"))
                        .value(text)
                        .multiline(1)
                        .mono()
                        .borderless()
                        .read_only(),
                    |_, _, _, _| {},
                    window,
                    cx,
                );
                v_flex()
                    .size_full()
                    .when(clipped, |this| {
                        this.child(
                            div()
                                .px_3()
                                .py_1()
                                .bg(status.warning_background)
                                .text_color(status.warning)
                                .text_xs()
                                .child("Large body: showing the first 400 KB. Use Save to get the full body."),
                        )
                    })
                    .child(
                        div()
                            .id(SharedString::from(format!("response-scroll-{mode}")))
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .p_3()
                            .child(view),
                    )
                    .into_any_element()
            }
            BodyMode::Preview if is_image => match ImageFormat::from_mime_type(&content_type) {
                Some(format) => {
                    let image = self.preview_image(tab_id, response, format);
                    div()
                        .id("image-preview")
                        .size_full()
                        .overflow_y_scroll()
                        .p_3()
                        .flex()
                        .justify_center()
                        .child(img(image).max_w_full().object_fit(ObjectFit::ScaleDown))
                        .into_any_element()
                }
                None => div()
                    .p_3()
                    .child(hint(format!(
                        "Preview is not supported for {content_type}."
                    )))
                    .into_any_element(),
            },
            BodyMode::Preview if response.is_html() => {
                let html = response.body_text();
                v_flex()
                    .p_3()
                    .gap_2()
                    .child(hint("HTML previews open in your default browser, where scripts in the response will run. Only open responses you trust."))
                    .child(
                        Button::new("open-html-preview", "Open in browser")
                            .style(ButtonStyle::Outlined)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_html_preview(html.clone(), cx);
                            })),
                    )
                    .into_any_element()
            }
            BodyMode::Preview => div()
                .p_3()
                .child(hint("Preview is available for HTML and image responses."))
                .into_any_element(),
        };

        v_flex()
            .size_full()
            .child(
                h_flex()
                    .flex_none()
                    .h_8()
                    .gap_1()
                    .px_3()
                    .children(modes.into_iter().map(|(mode, label)| {
                        Button::new(SharedString::from(format!("body-view-{label}")), label)
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .label_size(LabelSize::Small)
                            .toggle_state(mode == current)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.state.body_mode = mode;
                                cx.notify();
                            }))
                    })),
            )
            .child(div().flex_1().min_h_0().child(body))
            .into_any_element()
    }

    fn preview_image(
        &mut self,
        tab_id: TabId,
        response: &HttpResponse,
        format: ImageFormat,
    ) -> Arc<Image> {
        let key = (tab_id, response.time_ms, response.body.len());
        if let Some((cached, image)) = &self.image_cache
            && *cached == key
        {
            return image.clone();
        }
        let image = Arc::new(Image::from_bytes(format, response.body.clone()));
        self.image_cache = Some((key, image.clone()));
        image
    }
}
