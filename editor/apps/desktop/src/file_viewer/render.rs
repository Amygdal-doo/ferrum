//! Drawing the viewer: the image on a neutral backdrop with its size,
//! format and zoom in a footer, or a message with buttons for other apps.

use std::path::Path;
use std::sync::Arc;

use ferrum_ui::components::Tooltip;
use ferrum_ui::prelude::*;
use ferrumeditor_core::images::{self, ImageFormat};
use gpui::{Bounds, ObjectFit, StyledImage, canvas, img};

use super::{FIT_PADDING, FileViewer, FileViewerEvent, ImageInfo, ViewerContent};
use crate::settings::MAC;

fn reveal_label() -> &'static str {
    if MAC {
        "Reveal in Finder"
    } else {
        "Show in File Manager"
    }
}

/// "Open with Default App" / "Reveal in Finder" (and "Open as Text" for
/// SVGs) for `path`.
fn other_app_buttons(path: &Path, svg: bool, cx: &mut Context<FileViewer>) -> impl IntoElement {
    let open = path.to_path_buf();
    let reveal = path.to_path_buf();
    let as_text = path.to_path_buf();
    h_flex()
        .gap_2()
        .when(svg, |this| {
            this.child(
                Button::new("viewer-open-as-text", "Open as Text")
                    .style(ButtonStyle::Outlined)
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(FileViewerEvent::OpenAsText(as_text.clone()));
                    })),
            )
        })
        .child(
            Button::new("viewer-open-default", "Open with Default App")
                .style(ButtonStyle::Outlined)
                .on_click(move |_, _, cx| cx.open_with_system(&open)),
        )
        .child(
            Button::new("viewer-reveal", reveal_label())
                .style(ButtonStyle::Outlined)
                .on_click(move |_, _, cx| cx.reveal_path(&reveal)),
        )
}

impl FileViewer {
    fn render_message(
        &self,
        title: SharedString,
        detail: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let svg = self.format == Some(ImageFormat::Svg);
        let size = self.file_size.map(images::format_size);
        v_flex()
            .debug_selector(|| "viewer-message".into())
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .p_6()
            .child(
                Icon::new(if self.is_image() {
                    IconName::Warning
                } else {
                    IconName::File
                })
                .size(IconSize::Medium)
                .color(Color::Muted),
            )
            .child(Label::new(title))
            .child(
                Label::new(detail)
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .when_some(size, |this, size| {
                this.child(
                    Label::new(size)
                        .size(LabelSize::Small)
                        .color(Color::Placeholder),
                )
            })
            .child(div().pt_2().child(other_app_buttons(&self.path, svg, cx)))
            .into_any_element()
    }

    fn render_image(
        &self,
        info: ImageInfo,
        image: Arc<gpui::Image>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let viewport = self.viewport.clone();
        let failed = |error: SharedString| {
            move || {
                Label::new(error.clone())
                    .color(Color::Muted)
                    .into_any_element()
            }
        };
        let picture = img(image).with_fallback(failed("The image could not be drawn.".into()));
        let stage = match self.zoom {
            None => div()
                .size_full()
                .p(px(FIT_PADDING))
                .flex()
                .items_center()
                .justify_center()
                .child(picture.size_full().object_fit(ObjectFit::ScaleDown))
                .into_any_element(),
            Some(zoom) => div()
                .id("viewer-scroll")
                .size_full()
                .overflow_scroll()
                .child(
                    div()
                        .min_w_full()
                        .min_h_full()
                        .p(px(FIT_PADDING))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            picture
                                .flex_none()
                                .w(px(info.width as f32 * zoom))
                                .h(px(info.height as f32 * zoom)),
                        ),
                )
                .into_any_element(),
        };
        let percent = (self.scale() * 100.0).round();
        let zoom_label = match self.zoom {
            None => format!("Fit · {percent}%"),
            Some(_) => format!("{percent}%"),
        };
        let mut details = vec![
            format!("{} × {}", info.width, info.height),
            info.format.label().to_string(),
        ];
        if let Some(size) = self.file_size {
            details.push(images::format_size(size));
        }
        let svg = info.format == ImageFormat::Svg;
        let as_text = self.path.clone();
        v_flex()
            .size_full()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    // A neutral backdrop, so transparent pixels show.
                    .bg(colors.element_background)
                    .child(
                        canvas(
                            move |bounds: Bounds<Pixels>, _, _| viewport.set(bounds.size),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .child(stage),
            )
            .child(
                h_flex()
                    .debug_selector(|| "viewer-footer".into())
                    .flex_none()
                    .h(px(30.))
                    .px_2()
                    .gap_2()
                    .border_t_1()
                    .border_color(colors.border_variant)
                    .child(
                        Label::new(details.join(" · "))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(div().flex_1())
                    .when(svg, |this| {
                        this.child(
                            Button::new("viewer-svg-text", "Open as Text")
                                .label_size(LabelSize::Small)
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.emit(FileViewerEvent::OpenAsText(as_text.clone()));
                                })),
                        )
                    })
                    .child(
                        IconButton::new("viewer-zoom-out", IconName::Dash)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Zoom Out"))
                            .on_click(cx.listener(|this, _, _, cx| this.zoom_out(cx))),
                    )
                    .child(
                        Button::new("viewer-zoom-fit", zoom_label)
                            .label_size(LabelSize::Small)
                            .tooltip(Tooltip::text("Fit to Pane"))
                            .on_click(cx.listener(|this, _, _, cx| this.zoom_to_fit(cx))),
                    )
                    .child(
                        IconButton::new("viewer-zoom-in", IconName::Plus)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Zoom In"))
                            .on_click(cx.listener(|this, _, _, cx| this.zoom_in(cx))),
                    ),
            )
            .into_any_element()
    }
}

impl Render for FileViewer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().colors().editor_background;
        let content = match self.content.clone() {
            ViewerContent::Loading => empty_state("Loading…", self.title()).into_any_element(),
            ViewerContent::Image { info, image } => self.render_image(info, image, cx),
            ViewerContent::Failed(error) => {
                self.render_message("Can't show this image".into(), error, cx)
            }
            ViewerContent::Binary => self.render_message(
                "This file is not text".into(),
                "FerrumIDE can't show it; open it in another app.".into(),
                cx,
            ),
        };
        div()
            .id("file-viewer")
            .key_context("FerrumFileViewer")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_run_command))
            .size_full()
            .bg(background)
            .child(content)
    }
}
