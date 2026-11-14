//! Root view: searchable tool list in a sidebar on the window backdrop, the
//! selected tool in a pane card on the right.

use crate::tools;
use crate::widgets::KitEvent;
use ferrum_ui::gpui::{AnyView, KeyBinding};
use ferrum_ui::prelude::*;
use ferrumkit_core::{Tool, filter_tools};
use std::collections::HashMap;

actions!(ferrum_kit, [FocusSearch, ToggleSidebar]);

const SIDEBAR_WIDTH: f32 = 232.;

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-k", FocusSearch, None),
        KeyBinding::new("ctrl-k", FocusSearch, None),
        KeyBinding::new("cmd-b", ToggleSidebar, None),
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
    /// Inside the tool card; the card is lighter while it holds focus.
    card_focus: FocusHandle,
    sidebar_open: bool,
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
            card_focus: cx.focus_handle(),
            sidebar_open: true,
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

    fn toggle_sidebar(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_open = !self.sidebar_open;
        cx.notify();
    }

    /// ◧ and ⌘: in the sidebar's title row, or at the start of the tool
    /// card's header while the sidebar is hidden.
    fn title_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .flex_none()
            .gap_0p5()
            .child(
                ferrum_ui::sidebar_toggle("toggle-sidebar", self.sidebar_open).on_click(
                    cx.listener(|this, _, window, cx| {
                        this.toggle_sidebar(&ToggleSidebar, window, cx)
                    }),
                ),
            )
            .child(
                ferrum_ui::command_button("search-tools", "Search tools (⌘K)").on_click(
                    cx.listener(|this, _, window, cx| {
                        this.sidebar_open = true;
                        this.focus_search(&FocusSearch, window, cx);
                        cx.notify();
                    }),
                ),
            )
    }

    /// The sidebar on the window backdrop: title row (traffic lights, ◧ ⌘),
    /// search, then the tools.
    fn render_sidebar(&mut self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let query = self.search.read(cx).text().to_string();
        let matches = filter_tools(&query);
        v_flex()
            .w(px(SIDEBAR_WIDTH))
            .flex_none()
            .h_full()
            .child(
                ferrum_ui::title_row("sidebar-title", true, window)
                    .child(div().flex_1())
                    .child(self.title_controls(cx)),
            )
            .child(div().flex_none().px_2().pb_2().child(self.search.clone()))
            .child(
                v_flex()
                    .id("tool-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .when(matches.is_empty(), |this| {
                        this.child(empty_state("No tools match", "Try another word"))
                    })
                    .children(matches.into_iter().map(|tool| {
                        ferrum_ui::sidebar_item(
                            SharedString::from(format!("tool-{tool:?}")),
                            tool == self.selected,
                            cx,
                        )
                        .tooltip(Tooltip::text(tool.description()))
                        .text_sm()
                        .child(div().min_w_0().truncate().child(tool.name()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.select(tool);
                            cx.notify();
                        }))
                    })),
            )
            .child(
                div()
                    .flex_none()
                    .px_3p5()
                    .py_2()
                    .text_xs()
                    .text_color(colors.text_muted)
                    .child(format!("{} tools · offline", Tool::ALL.len())),
            )
    }

    /// The tool card's header: (◧ ⌘ while the sidebar is hidden), the
    /// tool's name and what it does.
    fn render_tool_header(
        &self,
        tool: Tool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        ferrum_ui::title_row("tool-header", !self.sidebar_open, window)
            .gap_2()
            .border_b_1()
            .border_color(colors.border_variant)
            .when(!self.sidebar_open, |row| {
                row.child(self.title_controls(cx)).child(div().w(px(4.)))
            })
            .child(
                div()
                    .flex_none()
                    .pl_1()
                    .text_sm()
                    .text_color(colors.text)
                    .child(tool.name()),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(colors.text_muted)
                    .child(tool.description()),
            )
    }
}

impl Render for FerrumKit {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tool = self.selected;
        let view = self.ensure_view(tool, window, cx);
        let colors = cx.theme().colors().clone();
        let sidebar_open = self.sidebar_open;
        let active = self.card_focus.contains_focused(window, cx);
        let card = ferrum_ui::pane_card(active, cx)
            .track_focus(&self.card_focus)
            .flex_1()
            .h_full()
            .child(self.render_tool_header(tool, window, cx))
            .child(div().flex_1().min_h_0().px_4().py_3().child(view));
        h_flex()
            .relative()
            .size_full()
            .text_color(colors.text)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::focus_search))
            .on_action(cx.listener(Self::toggle_sidebar))
            .when(sidebar_open, |root| {
                root.child(self.render_sidebar(window, cx))
            })
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .py(ferrum_ui::PANE_GAP)
                    .pr(ferrum_ui::PANE_GAP)
                    .pl(ferrum_ui::PANE_GAP)
                    .child(card),
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
