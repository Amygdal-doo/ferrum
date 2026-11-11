//! The find / replace bar under the panes. It owns only the query fields
//! and toggles; the workspace applies its events to the active editor.

use ferrum_ui::prelude::*;
use ferrum_ui::{TextInput, TextInputEvent};
use ferrumeditor_core::find::SearchQuery;
use ui::{IconButton, IconName, IconSize};

pub enum FindBarEvent {
    QueryChanged,
    Next,
    Previous,
    Replace,
    ReplaceAll,
    SelectAll,
    Close,
}

pub struct FindBar {
    pub find: Entity<TextInput>,
    pub replace: Entity<TextInput>,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
    pub show_replace: bool,
    /// "3 of 12", "No results" or a regex error, set by the workspace.
    pub status: SharedString,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<FindBarEvent> for FindBar {}

impl FindBar {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let find = cx.new(|cx| TextInput::new(window, cx).placeholder("Find").monospace());
        let replace = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Replace")
                .monospace()
        });
        let subscriptions = vec![
            cx.subscribe(&find, |_, _, event, cx| match event {
                TextInputEvent::Changed => cx.emit(FindBarEvent::QueryChanged),
                TextInputEvent::Submit => cx.emit(FindBarEvent::Next),
                TextInputEvent::Blurred => {}
            }),
            cx.subscribe(&replace, |_, _, event, cx| {
                if *event == TextInputEvent::Submit {
                    cx.emit(FindBarEvent::Replace);
                }
            }),
        ];
        Self {
            find,
            replace,
            case_sensitive: false,
            whole_word: false,
            regex: false,
            show_replace: false,
            status: SharedString::default(),
            _subscriptions: subscriptions,
        }
    }

    pub fn query(&self, cx: &App) -> SearchQuery {
        SearchQuery {
            pattern: self.find.read(cx).text().to_string(),
            case_sensitive: self.case_sensitive,
            whole_word: self.whole_word,
            regex: self.regex,
        }
    }

    pub fn replacement(&self, cx: &App) -> String {
        self.replace.read(cx).text().to_string()
    }

    /// Focuses the find field, optionally pre-filled, with its text selected.
    pub fn focus_find(
        &mut self,
        prefill: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.find.update(cx, |input, cx| {
            if let Some(text) = prefill.filter(|t| !t.contains('\n')) {
                input.set_text(text, cx);
            }
            let len = input.text().len();
            input.select_range(0..len, cx);
            input.focus(window, cx);
        });
        cx.emit(FindBarEvent::QueryChanged);
    }

    pub fn focus_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.replace.update(cx, |input, cx| input.focus(window, cx));
    }

    fn toggle(&mut self, which: u8, cx: &mut Context<Self>) {
        match which {
            0 => self.case_sensitive = !self.case_sensitive,
            1 => self.whole_word = !self.whole_word,
            _ => self.regex = !self.regex,
        }
        cx.emit(FindBarEvent::QueryChanged);
        cx.notify();
    }
}

fn toggle_button(id: &'static str, icon: IconName, on: bool, tooltip: &'static str) -> IconButton {
    IconButton::new(id, icon)
        .icon_size(IconSize::Small)
        .toggle_state(on)
        .tooltip(Tooltip::text(tooltip))
}

impl Render for FindBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let event_button = |id: &'static str,
                            icon: IconName,
                            tooltip: &'static str,
                            event: fn() -> FindBarEvent,
                            cx: &mut Context<Self>| {
            IconButton::new(id, icon)
                .icon_size(IconSize::Small)
                .tooltip(Tooltip::text(tooltip))
                .on_click(cx.listener(move |_, _, _, cx| cx.emit(event())))
        };
        let find_row = h_flex()
            .gap_1()
            .child(div().flex_1().min_w_0().child(self.find.clone()))
            .child(
                toggle_button(
                    "find-case",
                    IconName::CaseSensitive,
                    self.case_sensitive,
                    "Match Case",
                )
                .on_click(cx.listener(|this, _, _, cx| this.toggle(0, cx))),
            )
            .child(
                toggle_button(
                    "find-word",
                    IconName::WholeWord,
                    self.whole_word,
                    "Whole Word",
                )
                .on_click(cx.listener(|this, _, _, cx| this.toggle(1, cx))),
            )
            .child(
                toggle_button(
                    "find-regex",
                    IconName::Regex,
                    self.regex,
                    "Regular Expression",
                )
                .on_click(cx.listener(|this, _, _, cx| this.toggle(2, cx))),
            )
            .child(
                div().w(px(110.)).child(
                    Label::new(self.status.clone())
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
            )
            .child(event_button(
                "find-prev",
                IconName::ArrowUp,
                "Find Previous (⇧↩)",
                || FindBarEvent::Previous,
                cx,
            ))
            .child(event_button(
                "find-next",
                IconName::ArrowDown,
                "Find Next (↩)",
                || FindBarEvent::Next,
                cx,
            ))
            .child(event_button(
                "find-all",
                IconName::Check,
                "Select All Matches",
                || FindBarEvent::SelectAll,
                cx,
            ))
            .child(
                IconButton::new("find-toggle-replace", IconName::Replace)
                    .icon_size(IconSize::Small)
                    .toggle_state(self.show_replace)
                    .tooltip(Tooltip::text("Toggle Replace"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_replace = !this.show_replace;
                        cx.notify();
                    })),
            )
            .child(event_button(
                "find-close",
                IconName::Close,
                "Close (esc)",
                || FindBarEvent::Close,
                cx,
            ));
        v_flex()
            .key_context("FerrumFindBar")
            .flex_none()
            .px_2()
            .py_1p5()
            .gap_1()
            .bg(colors.panel_background)
            .border_t_1()
            .border_color(colors.border)
            .on_action(cx.listener(|_, _: &menu::Cancel, _, cx| cx.emit(FindBarEvent::Close)))
            .child(find_row)
            .when(self.show_replace, |this| {
                this.child(
                    h_flex()
                        .gap_1()
                        .child(div().flex_1().min_w_0().child(self.replace.clone()))
                        .child(event_button(
                            "replace-one",
                            IconName::ReplaceNext,
                            "Replace",
                            || FindBarEvent::Replace,
                            cx,
                        ))
                        .child(event_button(
                            "replace-all",
                            IconName::ReplaceAll,
                            "Replace All",
                            || FindBarEvent::ReplaceAll,
                            cx,
                        ))
                        .child(div().w(px(110.))),
                )
            })
    }
}
