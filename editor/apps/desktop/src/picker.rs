//! A modal fuzzy picker used by the command palette, the file finder,
//! go-to-line and the theme / icon theme selectors: a query field over a
//! filtered, keyboard-navigable list. Moving the selection emits
//! [`PickerEvent::Highlighted`] (live theme preview).

use std::path::PathBuf;
use std::sync::Arc;

use ferrumeditor_core::icons::{EntryKind, Icon};

use ferrum_ui::prelude::*;
use ferrum_ui::{TextInput, TextInputEvent};
use ferrumeditor_core::fuzzy::{FuzzyMatch, fuzzy_filter};
use gpui::{Action, HighlightStyle, ScrollStrategy, StyledText, UniformListScrollHandle};

/// Rows shown at most (the list scrolls beyond that).
const MAX_RESULTS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    Commands,
    Files,
    GoToLine,
    Theme,
    IconTheme,
    /// Errors and warnings from language servers.
    Problems,
    /// Several definitions to choose from.
    Locations,
}

#[derive(Debug, Clone)]
pub struct PickerItem {
    pub label: SharedString,
    /// Right-hand hint (key binding) or secondary text (folder).
    pub detail: SharedString,
    pub payload: String,
    /// File whose icon (from the active icon theme) leads the row.
    pub icon: Option<PathBuf>,
    /// Fixed icons shown after the label (icon theme samples).
    pub samples: Vec<Icon>,
}

impl PickerItem {
    pub fn new(
        label: impl Into<SharedString>,
        detail: impl Into<SharedString>,
        payload: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            detail: detail.into(),
            payload: payload.into(),
            icon: None,
            samples: Vec::new(),
        }
    }
}

pub enum PickerEvent {
    Confirmed {
        kind: PickerKind,
        payload: String,
    },
    /// The selected row changed (by keyboard or filtering).
    Highlighted {
        kind: PickerKind,
        payload: String,
    },
    Dismissed,
}

pub struct Picker {
    kind: PickerKind,
    input: Entity<TextInput>,
    items: Arc<Vec<PickerItem>>,
    matches: Vec<(usize, FuzzyMatch)>,
    selected: usize,
    scroll: UniformListScrollHandle,
    up: Option<Box<dyn Action>>,
    down: Option<Box<dyn Action>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<PickerEvent> for Picker {}

impl Picker {
    pub fn new(
        kind: PickerKind,
        items: Vec<PickerItem>,
        placeholder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| TextInput::new(window, cx).placeholder(placeholder.to_string()));
        let subscription =
            cx.subscribe_in(
                &input,
                window,
                |this: &mut Self, _, event, _, cx| match event {
                    TextInputEvent::Changed => this.refilter(cx),
                    TextInputEvent::Submit => this.confirm(cx),
                    TextInputEvent::Blurred => {}
                },
            );
        input.update(cx, |input, cx| input.focus(window, cx));
        let mut this = Self {
            kind,
            input,
            items: Arc::new(items),
            matches: Vec::new(),
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            up: cx.build_action("text_input::Up", None).ok(),
            down: cx.build_action("text_input::Down", None).ok(),
            _subscriptions: vec![subscription],
        };
        this.refilter(cx);
        this
    }

    pub fn kind(&self) -> PickerKind {
        self.kind
    }

    /// Replaces the items (e.g. when a background file walk finishes),
    /// keeping the query.
    pub fn set_items(&mut self, items: Vec<PickerItem>, cx: &mut Context<Self>) {
        self.items = Arc::new(items);
        self.refilter(cx);
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).text().to_string();
        self.matches = if self.kind == PickerKind::GoToLine {
            Vec::new()
        } else {
            fuzzy_filter(&query, &self.items, |item| item.label.as_ref(), MAX_RESULTS)
        };
        self.selected = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.emit_highlighted(cx);
        cx.notify();
    }

    fn selected_payload(&self) -> Option<String> {
        self.matches
            .get(self.selected)
            .and_then(|(index, _)| self.items.get(*index))
            .map(|item| item.payload.clone())
    }

    fn emit_highlighted(&self, cx: &mut Context<Self>) {
        if let Some(payload) = self.selected_payload() {
            cx.emit(PickerEvent::Highlighted {
                kind: self.kind,
                payload,
            });
        }
    }

    /// Selects the row whose payload is `payload` (no event).
    pub fn select_payload(&mut self, payload: &str, cx: &mut Context<Self>) {
        if let Some(position) = self
            .matches
            .iter()
            .position(|(index, _)| self.items.get(*index).is_some_and(|i| i.payload == payload))
        {
            self.selected = position;
            self.scroll.scroll_to_item(position, ScrollStrategy::Center);
            cx.notify();
        }
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.matches.is_empty() {
            return;
        }
        let len = self.matches.len() as isize;
        self.selected = ((self.selected as isize + delta).rem_euclid(len)) as usize;
        self.scroll
            .scroll_to_item(self.selected, ScrollStrategy::Nearest);
        self.emit_highlighted(cx);
        cx.notify();
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        let payload = if self.kind == PickerKind::GoToLine {
            Some(self.input.read(cx).text().to_string())
        } else {
            self.selected_payload()
        };
        match payload {
            Some(payload) => cx.emit(PickerEvent::Confirmed {
                kind: self.kind,
                payload,
            }),
            None => cx.emit(PickerEvent::Dismissed),
        }
    }

    fn render_row(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some((index, matched)) = self.matches.get(ix) else {
            return div().into_any_element();
        };
        let Some(item) = self.items.get(*index) else {
            return div().into_any_element();
        };
        let colors = cx.theme().colors();
        let accent = colors.text_accent;
        let label: &str = item.label.as_ref();
        let highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = label
            .char_indices()
            .enumerate()
            .filter(|(char_ix, _)| matched.indices.contains(char_ix))
            .map(|(_, (byte, c))| {
                (
                    byte..byte + c.len_utf8(),
                    HighlightStyle {
                        color: Some(accent),
                        font_weight: Some(gpui::FontWeight::BOLD),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let selected = ix == self.selected;
        let detail = item.detail.clone();
        let kind = self.kind;
        let icon = item
            .icon
            .as_ref()
            .and_then(|path| crate::file_icons::icon_element(path, EntryKind::File, cx));
        let muted = cx
            .global::<crate::settings::Settings>()
            .theme
            .ui("text_muted");
        let samples: Vec<AnyElement> = item
            .samples
            .iter()
            .map(|icon| {
                gpui::svg()
                    .data(&icon.svg)
                    .flex_none()
                    .size(px(crate::file_icons::ICON_SIZE))
                    .text_color(crate::theme_apply::hsla(icon.color.unwrap_or(muted)))
                    .into_any_element()
            })
            .collect();
        h_flex()
            .id(("picker-row", ix))
            .w_full()
            .px_3()
            .py_1()
            .gap_2()
            .justify_between()
            .rounded_sm()
            .when(selected, |this| this.bg(colors.element_selected))
            .hover(|this| this.bg(colors.element_hover))
            .child(
                h_flex()
                    .min_w_0()
                    .gap_2()
                    .when_some(icon, |this, icon| this.child(icon))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(StyledText::new(item.label.clone()).with_highlights(highlights)),
                    )
                    .when(!samples.is_empty(), |this| {
                        this.child(h_flex().gap_1().flex_none().children(samples))
                    }),
            )
            .when(!detail.is_empty(), |this| {
                this.child(if kind == PickerKind::Commands {
                    kbd(detail, cx).into_any_element()
                } else {
                    Label::new(detail)
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .into_any_element()
                })
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected = ix;
                this.confirm(cx);
            }))
            .into_any_element()
    }
}

impl Render for Picker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let count = self.matches.len();
        let hint = match self.kind {
            PickerKind::GoToLine => Some("Line number, or line:column"),
            PickerKind::Problems if count == 0 && self.items.is_empty() => {
                Some("No problems. Files with a language server are checked as you edit.")
            }
            _ if count == 0 => Some("No matches"),
            _ => None,
        };
        let mut root = v_flex()
            .key_context("FerrumPicker")
            .w(px(600.))
            .max_h(px(440.))
            .p_1()
            .gap_1()
            .rounded_lg()
            .border_1()
            .border_color(colors.border)
            .bg(colors.elevated_surface_background)
            .shadow_lg()
            .on_action(cx.listener(|_, _: &menu::Cancel, _, cx| cx.emit(PickerEvent::Dismissed)))
            .child(self.input.clone());
        if let Some(up) = &self.up {
            root = root.on_boxed_action(
                up.as_ref(),
                cx.listener(|this, _: &dyn Action, _, cx| this.move_selection(-1, cx)),
            );
        }
        if let Some(down) = &self.down {
            root = root.on_boxed_action(
                down.as_ref(),
                cx.listener(|this, _: &dyn Action, _, cx| this.move_selection(1, cx)),
            );
        }
        root.when_some(hint, |this, hint| {
            this.child(
                div()
                    .px_3()
                    .py_2()
                    .child(Label::new(hint).size(LabelSize::Small).color(Color::Muted)),
            )
        })
        .when(count > 0, |this| {
            this.child(
                uniform_list(
                    "picker-list",
                    count,
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        range.map(|ix| this.render_row(ix, cx)).collect()
                    }),
                )
                .track_scroll(&self.scroll)
                .h(px((count.min(12) as f32) * 28.)),
            )
        })
    }
}
