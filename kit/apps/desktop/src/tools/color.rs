//! Colour converter with a swatch.

use super::layout;
use crate::widgets::*;
use ferrum_ui::gpui::{Hsla, Rgba as GpuiRgba};
use ferrum_ui::prelude::*;
use ferrumkit_core::color::{self, Rgba};

pub struct ColorTool {
    input: Entity<TextInput>,
    result: Option<Result<Rgba, String>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<KitEvent> for ColorTool {}

impl ColorTool {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            TextInput::new(window, cx)
                .monospace()
                .placeholder("#ff8000, rgb(255 128 0), hsl(30, 100%, 50%)")
                .with_text("#e8590c")
        });
        let subscriptions = vec![on_change(&input, cx, Self::run)];
        let mut this = Self {
            input,
            result: None,
            _subscriptions: subscriptions,
        };
        this.run(cx);
        this
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let input = text_of(&self.input, cx);
        self.result = (!input.trim().is_empty()).then(|| color::parse(&input));
    }
}

fn to_hsla(color: Rgba) -> Hsla {
    GpuiRgba {
        r: f32::from(color.r) / 255.0,
        g: f32::from(color.g) / 255.0,
        b: f32::from(color.b) / 255.0,
        a: color.a,
    }
    .into()
}

impl Render for ColorTool {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.result.clone() {
            None => hint("Enter a colour as hex, rgb() or hsl().").into_any_element(),
            Some(Err(error)) => status_line(Err(error), cx).into_any_element(),
            Some(Ok(rgba)) => {
                let hsl = rgba.to_hsl();
                h_flex()
                    .items_start()
                    .gap_4()
                    .child(swatch(to_hsla(rgba), px(140.), cx))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(value_row("hex", "Hex", rgba.to_hex(), cx))
                            .child(value_row("rgb", "RGB", rgba.to_rgb_string(), cx))
                            .child(value_row("hsl", "HSL", rgba.to_hsl_string(), cx))
                            .child(value_row(
                                "channels",
                                "Channels",
                                format!(
                                    "R {}  G {}  B {}  ·  H {:.1}°  S {:.1}%  L {:.1}%  ·  A {:.2}",
                                    rgba.r, rgba.g, rgba.b, hsl.h, hsl.s, hsl.l, rgba.a
                                ),
                                cx,
                            )),
                    )
                    .into_any_element()
            }
        };
        layout(
            v_flex().gap_2().child(self.input.clone()).child(hint(
                "Accepts #rgb, #rgba, #rrggbb, #rrggbbaa, rgb()/rgba() and hsl()/hsla().",
            )),
            div()
                .id("color-body")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(body),
        )
    }
}
