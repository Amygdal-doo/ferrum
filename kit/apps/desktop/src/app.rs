//! Root view: searchable tool list on the left, the selected tool on the right.

use crate::tools;
use crate::widgets::KitEvent;
use ferrum_ui::gpui::{AnyView, KeyBinding};
use ferrum_ui::prelude::*;
use ferrumkit_core::{Tool, filter_tools};
use std::collections::HashMap;

actions!(ferrum_kit, [FocusSearch]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-k", FocusSearch, None),
        KeyBinding::new("ctrl-k", FocusSearch, None),
    ]);
}

pub struct FerrumKit {
    search: Entity<TextInput>,
    selected: Tool,
    /// Tool views created so far; kept for the session so each tool keeps
    /// its last input (in memory only).
    views: HashMap<Tool, AnyView>,
    /// Keeps the root in the dispatch path so ⌘K works with nothing focused.
    focus_handle: FocusHandle,
    notice: Option<Notice>,
    notice_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl FerrumKit {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| TextInput::new(window, cx).placeholder("Search tools…"));
        let subscriptions = vec![cx.subscribe(&search, |this: &mut Self, _, event, cx| {
            match event {
                TextInputEvent::Changed => cx.notify(),
                // Enter opens the first match.
                TextInputEvent::Submit => {
                    let query = this.search.read(cx).text().to_string();
                    if let Some(tool) = filter_tools(&query).first() {
                        this.selected = *tool;
                        cx.notify();
                    }
                }
                TextInputEvent::Blurred => {}
            }
        })];
        let mut this = Self {
            search,
            selected: Tool::Json,
            views: HashMap::new(),
            focus_handle: cx.focus_handle(),
            notice: None,
            notice_task: None,
            _subscriptions: subscriptions,
        };
        this.ensure_view(Tool::Json, window, cx);
        this.search
            .update(cx, |search, cx| search.focus(window, cx));
        this
    }

    /// Shows `tool` (its view is built on the next render).
    pub fn select(&mut self, tool: Tool) {
        self.selected = tool;
    }

    #[cfg(test)]
    pub fn selected(&self) -> Tool {
        self.selected
    }

    fn ensure_view(&mut self, tool: Tool, window: &mut Window, cx: &mut Context<Self>) -> AnyView {
        if let Some(view) = self.views.get(&tool) {
            return view.clone();
        }
        let (view, subscription) = tools::build(tool, window, cx);
        self._subscriptions.push(subscription);
        self.views.insert(tool, view.clone());
        view
    }

    pub fn handle_event(&mut self, event: &KitEvent, cx: &mut Context<Self>) {
        match event {
            KitEvent::Notice(notice) => {
                self.notice = Some(notice.clone());
                self.notice_task = Some(Notice::dismiss_later(cx, |this: &mut Self, _| {
                    this.notice = None;
                }));
                cx.notify();
            }
        }
    }

    fn focus_search(&mut self, _: &FocusSearch, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |search, cx| {
            let len = search.text().len();
            search.select_range(0..len, cx);
            search.focus(window, cx);
        });
    }

    fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let query = self.search.read(cx).text().to_string();
        let matches = filter_tools(&query);
        v_flex()
            .w(px(232.))
            .flex_none()
            .h_full()
            .bg(colors.panel_background)
            .border_r_1()
            .border_color(colors.border)
            .child(
                h_flex()
                    .p_2()
                    .gap_1()
                    .child(self.search.clone())
                    .child(kbd("⌘K", cx)),
            )
            .child(
                div()
                    .id("tool-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_1()
                    .when(matches.is_empty(), |this| {
                        this.child(empty_state("No tools match", "Try another word"))
                    })
                    .children(matches.into_iter().map(|tool| {
                        ListItem::new(SharedString::from(format!("tool-{tool:?}")))
                            .rounded()
                            .toggle_state(tool == self.selected)
                            .tooltip(Tooltip::text(tool.description()))
                            .child(Label::new(tool.name()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.select(tool);
                                cx.notify();
                            }))
                    })),
            )
    }
}

impl Render for FerrumKit {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tool = self.selected;
        let view = self.ensure_view(tool, window, cx);
        let colors = cx.theme().colors();
        v_flex()
            .relative()
            .size_full()
            .bg(colors.background)
            .text_color(colors.text)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::focus_search))
            .child(
                toolbar(cx)
                    .justify_between()
                    .child(Label::new("FerrumKit"))
                    .child(
                        Label::new("Runs locally · nothing is sent over the network")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_sidebar(cx))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(
                                v_flex()
                                    .flex_none()
                                    .px_4()
                                    .pt_3()
                                    .pb_2()
                                    .child(Headline::new(tool.name()).size(HeadlineSize::Small))
                                    .child(
                                        Label::new(tool.description())
                                            .size(LabelSize::Small)
                                            .color(Color::Muted),
                                    ),
                            )
                            .child(div().flex_1().min_h_0().px_4().pb_3().child(view)),
                    ),
            )
            .child(
                status_bar(cx)
                    .child(Label::new(format!("{} tools", Tool::ALL.len())).size(LabelSize::Small))
                    .child(
                        Label::new("Offline · inputs are kept in memory only")
                            .size(LabelSize::Small),
                    ),
            )
            .when_some(self.notice.as_ref(), |this, notice| {
                this.child(render_notice(
                    notice,
                    cx.listener(|this, _, _, cx| {
                        this.notice = None;
                        cx.notify();
                    }),
                    cx,
                ))
            })
    }
}
