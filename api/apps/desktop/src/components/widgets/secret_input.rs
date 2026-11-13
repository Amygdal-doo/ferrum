//! Masked input with a show / hide toggle.

use crate::app::FerrumApi;
use crate::components::fields::{FieldEvent, FieldSpec};
use ferrum_ui::components::{
    Button, ButtonCommon as _, ButtonSize, ButtonStyle, Clickable as _, Toggleable as _, h_flex,
};
use gpui::{AnyElement, Context, SharedString, Window, prelude::*};

impl FerrumApi {
    pub fn render_secret_input(
        &mut self,
        key: String,
        value: &str,
        placeholder: &'static str,
        handler: impl Fn(&mut FerrumApi, FieldEvent, &mut Window, &mut Context<FerrumApi>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shown = self.state.revealed.contains(&key);
        let input = self.field(
            FieldSpec::new(key.clone())
                .value(value)
                .placeholder(placeholder)
                .masked(!shown)
                .mono(),
            handler,
            window,
            cx,
        );
        h_flex()
            .gap_1()
            .child(input)
            .child(
                Button::new(
                    SharedString::from(format!("{key}-reveal")),
                    if shown { "Hide" } else { "Show" },
                )
                .style(ButtonStyle::Subtle)
                .size(ButtonSize::Compact)
                .toggle_state(shown)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.state.revealed.remove(&key) {
                        this.state.revealed.insert(key.clone());
                    }
                    cx.notify();
                })),
            )
            .into_any_element()
    }
}
