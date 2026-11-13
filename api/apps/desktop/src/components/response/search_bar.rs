//! Find-in-body bar (⌘F): query, match count, previous / next, case toggle.

use super::display::MAX_MATCHES;
use crate::app::FerrumApi;
use crate::components::fields::{FieldEvent, FieldSpec};
use crate::state::{BodyMode, ResponseSearch, ResponseView, TabResponse};
use ferrum_ui::components::{
    ActiveTheme as _, ButtonCommon as _, Clickable as _, Disableable as _, IconButton, IconName,
    IconSize, Toggleable as _, Tooltip, h_flex,
};
use gpui::{AnyElement, Context, Focusable as _, Window, div, prelude::*};

impl FerrumApi {
    /// Opens (or re-focuses) the find bar on the response body.
    pub fn open_response_search(&mut self) {
        let state = &mut self.state;
        let has_response = state
            .active_tab
            .and_then(|id| state.responses.get(&id))
            .is_some_and(|r| matches!(r, TabResponse::Done(_)));
        if !has_response {
            return;
        }
        state.response_view = ResponseView::Body;
        if state.body_mode == BodyMode::Preview {
            state.body_mode = BodyMode::Pretty;
        }
        let search = state
            .response_search
            .get_or_insert_with(ResponseSearch::default);
        search.focus = true;
        search.reveal = true;
    }

    /// Moves to the next (`1`) or previous (`-1`) match, wrapping around.
    pub fn step_response_search(&mut self, delta: isize) {
        let count = self.body_display.as_ref().map_or(0, |d| d.matches.len());
        let Some(search) = self.state.response_search.as_mut() else {
            return;
        };
        if count == 0 {
            return;
        }
        let current = search.current.cast_signed() + delta;
        search.current = current.rem_euclid(count.cast_signed()).cast_unsigned();
        search.reveal = true;
    }

    pub(super) fn render_search_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let search = self.state.response_search.clone()?;
        let colors = cx.theme().colors().clone();
        let input = self.field(
            FieldSpec::new("response-search")
                .value(search.query.clone())
                .placeholder("Find in body"),
            |this, event, _, _| match event {
                FieldEvent::Changed(query) => {
                    if let Some(s) = this.state.response_search.as_mut() {
                        s.query = query;
                        s.current = 0;
                        s.reveal = true;
                    }
                }
                FieldEvent::Submit => this.step_response_search(1),
                FieldEvent::Blurred => {}
            },
            window,
            cx,
        );
        if search.focus {
            if let Some(s) = self.state.response_search.as_mut() {
                s.focus = false;
            }
            let handle = input.focus_handle(cx);
            window.focus(&handle, cx);
            let len = search.query.len();
            input.update(cx, |input, cx| input.select_range(0..len, cx));
        }

        let count = self.body_display.as_ref().map_or(0, |d| d.matches.len());
        let summary = match count {
            _ if search.query.is_empty() => String::new(),
            0 => "No results".to_owned(),
            MAX_MATCHES => format!("{} of {MAX_MATCHES}+", search.current + 1),
            n => format!("{} of {n}", search.current + 1),
        };
        let button = |id: &'static str, icon: IconName, tip: &'static str| {
            IconButton::new(id, icon)
                .icon_size(IconSize::Small)
                .tooltip(Tooltip::text(tip))
        };

        Some(
            h_flex()
                .flex_none()
                .gap_1()
                .px_3()
                .py_1()
                .border_b_1()
                .border_color(colors.border_variant)
                .child(div().w_64().child(input))
                .child(
                    div()
                        .min_w_20()
                        .text_xs()
                        .text_color(colors.text_muted)
                        .child(summary),
                )
                .child(
                    button("search-prev", IconName::ChevronUp, "Previous match (⇧⌘G)")
                        .disabled(count == 0)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.step_response_search(-1);
                            cx.notify();
                        })),
                )
                .child(
                    button(
                        "search-next",
                        IconName::ChevronDown,
                        "Next match (Enter, ⌘G)",
                    )
                    .disabled(count == 0)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.step_response_search(1);
                        cx.notify();
                    })),
                )
                .child(
                    button("search-case", IconName::CaseSensitive, "Match case")
                        .toggle_state(search.case_sensitive)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(s) = this.state.response_search.as_mut() {
                                s.case_sensitive = !s.case_sensitive;
                                s.current = 0;
                                s.reveal = true;
                            }
                            cx.notify();
                        })),
                )
                .child(div().flex_1())
                .child(
                    button("search-close", IconName::Close, "Close (Esc)").on_click(cx.listener(
                        |this, _, window, cx| {
                            this.state.response_search = None;
                            window.focus(&this.root_focus(), cx);
                            cx.notify();
                        },
                    )),
                )
                .into_any_element(),
        )
    }
}
