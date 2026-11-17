//! QR code generator: the image updates as you type; copy it or save it
//! as a PNG.

use std::sync::Arc;

use super::layout;
use crate::widgets::*;
use ferrum_ui::gpui::{ClipboardItem, Image, ImageFormat, ObjectFit, img};
use ferrum_ui::prelude::*;
use ferrumkit_core::qr::{self, Correction};

/// Pixels per module in the PNG (copied and saved).
const SCALE: u32 = 10;
/// Shown size of the code.
const PREVIEW: f32 = 280.;

pub struct QrTool {
    input: Entity<TextInput>,
    correction: Correction,
    /// The PNG and the code's size in modules, or why there is none.
    result: Option<Result<(Arc<Image>, usize), String>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<KitEvent> for QrTool {}

impl QrTool {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = editor("Text or a URL to encode…", 6, window, cx);
        let subscriptions = vec![on_change(&input, cx, Self::run)];
        Self {
            input,
            correction: Correction::Medium,
            result: None,
            _subscriptions: subscriptions,
        }
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let text = text_of(&self.input, cx);
        self.result = (!text.is_empty()).then(|| {
            let code = qr::encode(&text, self.correction)?;
            let png = code.to_png(SCALE)?;
            Ok((
                Arc::new(Image::from_bytes(ImageFormat::Png, png)),
                code.width,
            ))
        });
    }

    fn image(&self) -> Option<Arc<Image>> {
        match &self.result {
            Some(Ok((image, _))) => Some(image.clone()),
            _ => None,
        }
    }

    fn copy_image(&mut self, cx: &mut Context<Self>) {
        if let Some(image) = self.image() {
            cx.write_to_clipboard(ClipboardItem::new_image(&image));
            cx.emit(KitEvent::Notice(Notice::info("QR code copied as an image")));
        }
    }

    fn save_png(&mut self, cx: &mut Context<Self>) {
        let Some(image) = self.image() else {
            return;
        };
        let dir = ferrum_ui::project::home_dir()
            .map(|home| home.join("Downloads"))
            .filter(|dir| dir.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        let chosen = cx.prompt_for_new_path(&dir, Some("qr-code.png"));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = chosen.await else {
                return;
            };
            let notice = match std::fs::write(&path, image.bytes()) {
                Ok(()) => Notice::info(format!("Saved {}", path.display())),
                Err(e) => Notice::error(format!("Could not save {}: {e}", path.display())),
            };
            this.update(cx, |_, cx| cx.emit(KitEvent::Notice(notice)))
                .ok();
        })
        .detach();
    }
}

impl Render for QrTool {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let options: Vec<(Correction, &'static str)> =
            Correction::ALL.iter().map(|c| (*c, c.label())).collect();
        let correction = choice(
            "qr-correction",
            &options,
            self.correction,
            |this: &mut Self, correction, _, cx| {
                this.correction = correction;
                this.run(cx);
            },
            cx,
        );
        let has_image = self.image().is_some();
        let actions = h_flex()
            .gap_1()
            .child(
                Button::new("qr-copy", "Copy image")
                    .style(ButtonStyle::Outlined)
                    .size(ButtonSize::Compact)
                    .disabled(!has_image)
                    .on_click(cx.listener(|this, _, _, cx| this.copy_image(cx))),
            )
            .child(
                Button::new("qr-save", "Save PNG…")
                    .style(ButtonStyle::Outlined)
                    .size(ButtonSize::Compact)
                    .disabled(!has_image)
                    .on_click(cx.listener(|this, _, _, cx| this.save_png(cx))),
            );
        let preview = match self.result.clone() {
            None => hint("The code appears here as you type.").into_any_element(),
            Some(Err(error)) => status_line(Err(error), cx),
            Some(Ok((image, width))) => v_flex()
                .gap_2()
                .child(
                    // The quiet zone is white, so the code reads on dark themes too.
                    img(image).size(px(PREVIEW)).object_fit(ObjectFit::Contain),
                )
                .child(hint(format!(
                    "{width} × {width} modules · PNG {} px",
                    (width + 2 * qr::QUIET_ZONE) * SCALE as usize
                )))
                .into_any_element(),
        };
        layout(
            controls()
                .child(
                    h_flex()
                        .gap_2()
                        .child(caption("Error correction"))
                        .child(correction),
                )
                .child(actions),
            h_flex()
                .flex_1()
                .min_h_0()
                .items_start()
                .gap_4()
                .child(v_flex().flex_1().min_w_0().h(px(PREVIEW)).child(panel(
                    "qr-in",
                    "Text",
                    None,
                    self.input.clone(),
                )))
                .child(div().flex_none().child(preview)),
        )
    }
}

impl super::KitTool for QrTool {
    fn inputs(&self) -> Vec<Entity<TextInput>> {
        vec![self.input.clone()]
    }

    fn rerun(&mut self, cx: &mut Context<Self>) {
        self.run(cx);
        cx.notify();
    }
}
