//! Line tools: operations rewrite the text in place (with undo), counts are live.

use super::layout;
use crate::widgets::*;
use ferrum_ui::prelude::*;
use ferrumkit_core::lines::{self, LineOp};

/// Undo steps kept per session.
const MAX_UNDO: usize = 50;

pub struct LinesTool {
    input: Entity<TextInput>,
    undo: Vec<String>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<KitEvent> for LinesTool {}

impl LinesTool {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = editor("One item per line…", 16, window, cx);
        let subscriptions = vec![on_change(&input, cx, |_, _| {})];
        Self {
            input,
            undo: Vec::new(),
            _subscriptions: subscriptions,
        }
    }

    fn apply(&mut self, op: LineOp, cx: &mut Context<Self>) {
        let text = text_of(&self.input, cx);
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_nanos() as u64);
        let result = lines::apply(&text, op, seed);
        if result == text {
            return;
        }
        self.undo.push(text);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        set_text(&self.input, result, cx);
    }

    fn undo(&mut self, cx: &mut Context<Self>) {
        if let Some(previous) = self.undo.pop() {
            set_text(&self.input, previous, cx);
        }
    }
}

impl Render for LinesTool {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let text = text_of(&self.input, cx);
        let stats = lines::stats(&text);
        let buttons = LineOp::ALL
            .iter()
            .enumerate()
            .map(|(i, op)| {
                let op = *op;
                Button::new(SharedString::from(format!("lines-op-{i}")), op.label())
                    .style(ButtonStyle::Outlined)
                    .size(ButtonSize::Compact)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.apply(op, cx);
                        cx.notify();
                    }))
            })
            .collect::<Vec<_>>();
        let counts = format!(
            "{} lines · {} non-empty · {} unique · {} words · {} chars · {} bytes",
            stats.lines,
            stats.non_empty_lines,
            stats.unique_lines,
            stats.words,
            stats.chars,
            stats.bytes
        );
        let trailing = h_flex()
            .gap_1()
            .child(
                IconButton::new("lines-undo", IconName::Undo)
                    .icon_size(IconSize::Small)
                    .disabled(self.undo.is_empty())
                    .tooltip(Tooltip::text("Undo last operation"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.undo(cx);
                        cx.notify();
                    })),
            )
            .child(copy_button("lines-copy", text, cx))
            .into_any_element();
        layout(
            v_flex()
                .gap_2()
                .child(controls().children(buttons))
                .child(caption(counts)),
            panel("lines-in", "Text", Some(trailing), self.input.clone()),
        )
    }
}

impl super::KitTool for LinesTool {
    fn inputs(&self) -> Vec<Entity<TextInput>> {
        vec![self.input.clone()]
    }
}
