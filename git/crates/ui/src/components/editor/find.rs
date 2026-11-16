//! The find bar above the text: a query field, the match count and
//! previous / next. Matches are highlighted while it is open.

use std::ops::Range;

use ferrum_ui::components::{
    ButtonCommon, Clickable, Color, IconButton, IconName, IconSize, Label, LabelCommon, LabelSize,
    Tooltip, h_flex,
};
use ferrum_ui::theme::ActiveTheme as _;
use ferrum_ui::{TextInput, TextInputEvent};
use ferrumeditor_core::editor::EditorState;
use ferrumeditor_core::find::{CompiledQuery, SearchQuery, next_match_index, previous_match_index};
use ferrumeditor_core::selection::Selection;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, Subscription, Window, div, prelude::*, px,
};

use super::CodeEditor;

/// Key context of the find bar (Escape closes it).
const FIND_CONTEXT: &str = "FerrumGitFind";

#[derive(Default)]
pub struct FindBar {
    input: Option<Entity<TextInput>>,
    compiled: Option<CompiledQuery>,
    pub matches: Vec<Range<usize>>,
    pub active: Option<usize>,
    /// Buffer version `matches` were computed for.
    version: Option<u64>,
    _input_events: Option<Subscription>,
}

impl FindBar {
    fn set_pattern(&mut self, pattern: &str, state: &EditorState) {
        self.compiled = SearchQuery::plain(pattern).compile().ok();
        self.matches.clear();
        self.active = None;
        self.version = None;
        self.refresh(state);
    }

    /// Recomputes matches if the buffer changed.
    pub fn refresh(&mut self, state: &EditorState) {
        let Some(compiled) = &self.compiled else {
            return;
        };
        if self.version == Some(state.buffer().version()) {
            return;
        }
        self.version = Some(state.buffer().version());
        self.matches = compiled.find_all(&state.text());
        self.active = self.active.filter(|i| *i < self.matches.len());
    }
}

impl CodeEditor {
    /// Shows the find bar (seeded with a one-line selection) and focuses it.
    pub fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let primary = self.state.primary();
        let seed = (!primary.is_empty())
            .then(|| self.state.buffer().slice(primary.range()))
            .filter(|s| !s.contains('\n'));
        let input = match self.find.input.clone() {
            Some(input) => input,
            None => {
                let input = cx.new(|cx| TextInput::new(window, cx).placeholder("Find"));
                let events =
                    cx.subscribe(&input, |this: &mut Self, input, event, cx| match event {
                        TextInputEvent::Changed => {
                            let text = input.read(cx).text().to_owned();
                            this.find.set_pattern(&text, &this.state);
                            let from = this.state.primary().start();
                            if let Some(i) = next_match_index(&this.find.matches, from) {
                                this.select_match(i, cx);
                            }
                            cx.notify();
                        }
                        TextInputEvent::Submit => this.find_next(true, cx),
                        TextInputEvent::Blurred => {}
                    });
                self.find._input_events = Some(events);
                self.find.input = Some(input.clone());
                input
            }
        };
        if let Some(seed) = seed {
            input.update(cx, |input, cx| input.set_text(seed.clone(), cx));
            self.find.set_pattern(&seed, &self.state);
        }
        input.update(cx, |input, cx| {
            let len = input.text().len();
            input.select_range(0..len, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    /// Hides the find bar and gives the text the focus back.
    pub fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find = FindBar::default();
        self.focus(window, cx);
        cx.notify();
    }

    /// The selection becomes the query (the bar opens if needed).
    pub(super) fn use_selection_as_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_find(window, cx);
        self.focus(window, cx);
    }

    /// Selects the next (or previous) match, wrapping around.
    pub fn find_next(&mut self, forward: bool, cx: &mut Context<Self>) {
        self.find.refresh(&self.state);
        let primary = self.state.primary();
        let matches = &self.find.matches;
        let index = if forward {
            next_match_index(matches, primary.end().max(primary.start() + 1))
                .or_else(|| next_match_index(matches, 0))
        } else {
            previous_match_index(matches, primary.start()).or_else(|| matches.len().checked_sub(1))
        };
        if let Some(index) = index {
            self.select_match(index, cx);
        }
    }

    fn select_match(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(range) = self.find.matches.get(index).cloned() else {
            return;
        };
        self.find.active = Some(index);
        self.state
            .set_selections(vec![Selection::new(range.start, range.end)]);
        self.autoscroll = true;
        cx.notify();
    }

    pub(super) fn render_find_bar(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let input = self.find.input.clone()?;
        let colors = cx.theme().colors();
        let count = match (self.find.active, self.find.matches.len()) {
            (_, 0) if self.find.compiled.is_some() => "No results".to_owned(),
            (_, 0) => String::new(),
            (Some(i), n) => format!("{} of {n}", i + 1),
            (None, n) => format!("{n} found"),
        };
        let button = |id: &'static str, icon: IconName, tip: &'static str| {
            IconButton::new(id, icon)
                .icon_size(IconSize::Small)
                .tooltip(Tooltip::text(tip))
        };
        Some(
            h_flex()
                .key_context(FIND_CONTEXT)
                .flex_none()
                .gap_1()
                .px_2()
                .py_1()
                .border_b_1()
                .border_color(colors.border)
                .bg(colors.editor_background)
                .on_action(
                    cx.listener(|this, _: &ferrum_ui::menu::Cancel, window, cx| {
                        this.close_find(window, cx);
                    }),
                )
                .child(div().flex_1().min_w_0().max_w(px(360.)).child(input))
                .child(Label::new(count).size(LabelSize::Small).color(Color::Muted))
                .child(div().flex_1())
                .child(
                    button("find-previous", IconName::ArrowUp, "Previous match (⇧⌘G)")
                        .on_click(cx.listener(|this, _, _, cx| this.find_next(false, cx))),
                )
                .child(
                    button("find-next", IconName::ArrowDown, "Next match (⌘G)")
                        .on_click(cx.listener(|this, _, _, cx| this.find_next(true, cx))),
                )
                .child(
                    button("find-close", IconName::Close, "Close (Esc)")
                        .on_click(cx.listener(|this, _, window, cx| this.close_find(window, cx))),
                )
                .into_any_element(),
        )
    }
}
