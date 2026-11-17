//! Line-based text diff.

use super::{layout, split};
use crate::widgets::*;
use ferrum_ui::prelude::*;
use ferrumkit_core::diff::{self, DiffLine, DiffStats, LineKind};

/// Rendered diff rows; the copied text always contains the full diff.
const MAX_ROWS: usize = 5_000;

pub struct DiffTool {
    left: Entity<TextInput>,
    right: Entity<TextInput>,
    lines: Vec<DiffLine>,
    stats: DiffStats,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<KitEvent> for DiffTool {}

impl DiffTool {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let left = editor("Original text…", 8, window, cx);
        let right = editor("Changed text…", 8, window, cx);
        let subscriptions = vec![
            on_change(&left, cx, Self::run),
            on_change(&right, cx, Self::run),
        ];
        Self {
            left,
            right,
            lines: Vec::new(),
            stats: DiffStats::default(),
            _subscriptions: subscriptions,
        }
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let (lines, stats) = diff::diff_lines(&text_of(&self.left, cx), &text_of(&self.right, cx));
        self.lines = lines;
        self.stats = stats;
    }

    fn render_rows(&self, cx: &App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let status = cx.theme().status();
        let number = |n: Option<usize>| n.map(|n| n.to_string()).unwrap_or_default();
        v_flex()
            .font_buffer(cx)
            .text_sm()
            .children(self.lines.iter().take(MAX_ROWS).map(|line| {
                let (sign, background, text) = match line.kind {
                    LineKind::Same => (" ", None, colors.text),
                    LineKind::Added => ("+", Some(status.created_background), status.created),
                    LineKind::Removed => ("−", Some(status.deleted_background), status.deleted),
                };
                h_flex()
                    .px_1()
                    .when_some(background, |this, bg| this.bg(bg))
                    .child(
                        div()
                            .w(px(44.))
                            .flex_none()
                            .text_color(colors.text_muted)
                            .child(number(line.old_line)),
                    )
                    .child(
                        div()
                            .w(px(44.))
                            .flex_none()
                            .text_color(colors.text_muted)
                            .child(number(line.new_line)),
                    )
                    .child(div().w(px(16.)).flex_none().text_color(text).child(sign))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(text)
                            .child(line.text.clone()),
                    )
            }))
            .when(self.lines.len() > MAX_ROWS, |this| {
                this.child(hint(format!(
                    "Showing the first {MAX_ROWS} lines; copy for the full diff."
                )))
            })
    }
}

impl Render for DiffTool {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let empty = self.lines.is_empty();
        let summary = if empty {
            None
        } else if self.stats.identical() {
            Some(Ok("No differences".to_string()))
        } else {
            Some(Err(format!(
                "+{} added  −{} removed  ({} unchanged)",
                self.stats.added, self.stats.removed, self.stats.same
            )))
        };
        let copy = copy_button("diff-copy", diff::to_text(&self.lines), cx).into_any_element();
        let rows = if empty {
            hint("Paste two texts above to compare them line by line.").into_any_element()
        } else {
            self.render_rows(cx).into_any_element()
        };
        layout(
            v_flex().h(px(240.)).flex_none().child(split(
                panel("diff-left", "Original", None, self.left.clone()),
                panel("diff-right", "Changed", None, self.right.clone()),
            )),
            v_flex()
                .flex_1()
                .min_h_0()
                .gap_1()
                .children(summary.map(|s| {
                    // Differences are not errors; show them in the warning colour.
                    let color = match s {
                        Ok(_) => cx.theme().status().success,
                        Err(_) => cx.theme().status().warning,
                    };
                    let text = s.unwrap_or_else(|e| e);
                    div().text_sm().text_color(color).child(text)
                }))
                .child(panel("diff-result", "Diff", Some(copy), rows)),
        )
    }
}

impl super::KitTool for DiffTool {
    fn inputs(&self) -> Vec<Entity<TextInput>> {
        vec![self.left.clone(), self.right.clone()]
    }

    fn rerun(&mut self, cx: &mut Context<Self>) {
        self.run(cx);
        cx.notify();
    }
}
