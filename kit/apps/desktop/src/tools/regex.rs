//! Regex tester (Rust `regex` crate syntax).

use super::{layout, split};
use crate::widgets::*;
use ferrum_ui::gpui::HighlightStyle;
use ferrum_ui::prelude::*;
use ferrumkit_core::regex_tool::{Flags, RegexResult, find_matches};

/// Matches listed in the side panel; all of them are still highlighted.
const LISTED_MATCHES: usize = 200;

pub struct RegexTool {
    pattern: Entity<TextInput>,
    flags: Entity<TextInput>,
    text: Entity<TextInput>,
    result: Option<Result<RegexResult, String>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<KitEvent> for RegexTool {}

impl RegexTool {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let pattern = line_input(r"Pattern, e.g. (?<user>\w+)@(\w+)\.com", true, window, cx);
        let flags = line_input("flags: i m s x", true, window, cx);
        let text = editor("Test text…", 16, window, cx);
        let subscriptions = vec![
            on_change(&pattern, cx, Self::run),
            on_change(&flags, cx, Self::run),
            on_change(&text, cx, Self::run),
        ];
        Self {
            pattern,
            flags,
            text,
            result: None,
            _subscriptions: subscriptions,
        }
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let pattern = text_of(&self.pattern, cx);
        let text = text_of(&self.text, cx);
        let result = Flags::parse(&text_of(&self.flags, cx))
            .and_then(|flags| find_matches(&pattern, flags, &text));
        let colors = cx.theme().colors();
        let styles = [
            colors.search_match_background,
            colors.search_active_match_background,
        ]
        .map(|background| HighlightStyle {
            background_color: Some(background),
            ..HighlightStyle::default()
        });
        let highlights = match &result {
            // Alternate colours so adjacent matches stay distinguishable;
            // empty matches have nothing to paint.
            Ok(result) => result
                .matches
                .iter()
                .filter(|m| !m.range.is_empty())
                .enumerate()
                .map(|(i, m)| (m.range.clone(), styles[i % 2]))
                .collect(),
            Err(_) => Vec::new(),
        };
        self.text
            .update(cx, |text, cx| text.set_highlights(highlights, cx));
        self.result = (!pattern.is_empty()).then_some(result);
    }

    fn render_matches(&self, result: &RegexResult, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let items = result
            .matches
            .iter()
            .take(LISTED_MATCHES)
            .enumerate()
            .map(|(i, m)| {
                let groups = m.groups.iter().map(|g| {
                    let name = match &g.name {
                        Some(name) => format!("  {} ({name})", g.index),
                        None => format!("  {}", g.index),
                    };
                    let value = g
                        .text
                        .clone()
                        .map_or_else(|| "— (no match)".to_string(), |t| format!("“{t}”"));
                    h_flex()
                        .gap_2()
                        .child(div().w(px(110.)).flex_none().child(caption(name)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .font_buffer(cx)
                                .text_sm()
                                .child(value),
                        )
                });
                v_flex()
                    .p_2()
                    .gap_0p5()
                    .rounded_sm()
                    .border_1()
                    .border_color(colors.border_variant)
                    .bg(colors.editor_background)
                    .child(
                        h_flex()
                            .gap_2()
                            .child(caption(format!(
                                "#{}  {}–{}",
                                i + 1,
                                m.range.start,
                                m.range.end
                            )))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .font_buffer(cx)
                                    .text_sm()
                                    .child(format!("“{}”", m.text)),
                            ),
                    )
                    .children(groups)
            });
        let count = result.matches.len();
        let summary = format!(
            "{count}{} match{}",
            if result.truncated { "+" } else { "" },
            if count == 1 { "" } else { "es" }
        );
        v_flex()
            .gap_1()
            .child(status_line(
                if count == 0 {
                    Err("No matches".into())
                } else {
                    Ok(summary)
                },
                cx,
            ))
            .children(items)
            .when(count > LISTED_MATCHES, |this| {
                this.child(hint(format!("Showing the first {LISTED_MATCHES} matches.")))
            })
    }
}

impl Render for RegexTool {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let side = match self.result.clone() {
            None => hint("Enter a pattern.").into_any_element(),
            Some(Err(error)) => status_line(Err(error), cx).into_any_element(),
            Some(Ok(result)) => self.render_matches(&result, cx).into_any_element(),
        };
        layout(
            v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .gap_2()
                        .child(div().flex_1().child(self.pattern.clone()))
                        .child(div().w(px(120.)).child(self.flags.clone())),
                )
                .child(hint(
                    "Rust regex syntax (regex crate): no look-around or backreferences. Flags: i = ignore case, m = multi-line ^/$, s = . matches newline, x = verbose.",
                )),
            split(
                panel("regex-text", "Test text", None, self.text.clone()),
                v_flex().w(px(320.)).flex_none().h_full().child(panel("regex-matches", "Matches", None, side)),
            ),
        )
    }
}
