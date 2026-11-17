//! UUID v4 / v7 and ULID generator.

use super::layout;
use crate::widgets::*;
use ferrum_ui::components::Checkbox;
use ferrum_ui::prelude::*;
use ferrumkit_core::ids::{self, IdKind};

pub struct IdsTool {
    count: Entity<TextInput>,
    output: Entity<TextInput>,
    kind: IdKind,
    uppercase: bool,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<KitEvent> for IdsTool {}

impl IdsTool {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let count = cx.new(|cx| TextInput::new(window, cx).with_text("5"));
        let output = output(12, window, cx);
        let subscriptions = vec![
            on_change(&count, cx, Self::generate),
            cx.subscribe(&count, |this: &mut Self, _, event, cx| {
                if *event == TextInputEvent::Submit {
                    this.generate(cx);
                    cx.notify();
                }
            }),
        ];
        let mut this = Self {
            count,
            output,
            kind: IdKind::UuidV4,
            uppercase: false,
            error: None,
            _subscriptions: subscriptions,
        };
        this.generate(cx);
        this
    }

    fn generate(&mut self, cx: &mut Context<Self>) {
        match ids::parse_count(&text_of(&self.count, cx)) {
            Ok(count) => {
                self.error = None;
                let ids = ids::generate(self.kind, count, self.uppercase);
                set_text(&self.output, ids.join("\n"), cx);
            }
            Err(error) => self.error = Some(error),
        }
    }
}

impl Render for IdsTool {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let kinds: Vec<(IdKind, &'static str)> =
            IdKind::ALL.iter().map(|k| (*k, k.label())).collect();
        let kind = choice(
            "ids-kind",
            &kinds,
            self.kind,
            |this: &mut Self, kind, _, cx| {
                this.kind = kind;
                this.generate(cx);
            },
            cx,
        );
        let upper = Checkbox::new(
            "ids-upper",
            if self.uppercase {
                ToggleState::Selected
            } else {
                ToggleState::Unselected
            },
        )
        .label("Uppercase")
        .disabled(self.kind == IdKind::Ulid)
        .on_click(cx.listener(|this, state: &ToggleState, _, cx| {
            this.uppercase = *state == ToggleState::Selected;
            this.generate(cx);
            cx.notify();
        }));
        let output_text = text_of(&self.output, cx);
        let note = match self.kind {
            IdKind::UuidV4 => "Random (122 random bits).",
            IdKind::UuidV7 => {
                "Time-ordered: Unix-ms timestamp + random bits; sorts by creation time."
            }
            IdKind::Ulid => "Time-ordered, 26 chars, Crockford base32 (always uppercase).",
        };
        layout(
            v_flex()
                .gap_2()
                .child(
                    controls()
                        .child(kind)
                        .child(caption("Count"))
                        .child(div().w(px(80.)).child(self.count.clone()))
                        .child(upper)
                        .child(
                            Button::new("ids-generate", "Generate")
                                .style(ButtonStyle::Filled)
                                .start_icon(Icon::new(IconName::RotateCw))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.generate(cx);
                                    cx.notify();
                                })),
                        ),
                )
                .child(hint(note))
                .children(self.error.clone().map(|e| status_line(Err(e), cx))),
            panel(
                "ids-out",
                "Generated",
                Some(copy_button("ids-copy", output_text, cx).into_any_element()),
                self.output.clone(),
            ),
        )
    }
}

impl super::KitTool for IdsTool {
    fn inputs(&self) -> Vec<Entity<TextInput>> {
        vec![self.count.clone()]
    }

    fn outputs(&self) -> Vec<Entity<TextInput>> {
        vec![self.output.clone()]
    }

    fn rerun(&mut self, cx: &mut Context<Self>) {
        self.generate(cx);
        cx.notify();
    }
}
