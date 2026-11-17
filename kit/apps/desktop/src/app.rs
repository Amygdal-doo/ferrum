//! Root view: searchable tool list in a sidebar on the window backdrop, the
//! selected tool in a pane card on the right, and the tool's history and
//! saved inputs in a panel at the card's right edge.

use std::path::PathBuf;
use std::time::Duration;

use crate::tools::{self, ToolView};
use crate::widgets::KitEvent;
use ferrum_ui::gpui::KeyBinding;
use ferrum_ui::prelude::*;
use ferrumkit_core::store::KitStore;
use ferrumkit_core::{Tool, filter_tools};
use std::collections::HashMap;

actions!(ferrum_kit, [FocusSearch, ToggleSidebar, ToggleHistory]);

const SIDEBAR_WIDTH: f32 = 232.;
/// A pause in typing this long records the input in the history.
const RECORD_AFTER: Duration = Duration::from_millis(1500);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-k", FocusSearch, None),
        KeyBinding::new("ctrl-k", FocusSearch, None),
        KeyBinding::new("cmd-b", ToggleSidebar, None),
        KeyBinding::new("cmd-y", ToggleHistory, None),
    ]);
}

/// `~/.ferrum/kit/inputs.json`.
pub fn store_path() -> Option<PathBuf> {
    ferrum_ui::project::home_dir().map(|home| home.join(".ferrum").join("kit").join("inputs.json"))
}

pub struct FerrumKit {
    search: Entity<TextInput>,
    selected: Tool,
    /// Tool views created so far; kept for the session so each tool keeps
    /// its last input.
    views: HashMap<Tool, ToolView>,
    /// History and saved inputs; `store_path` is `None` in tests (nothing
    /// is written).
    pub(crate) store: KitStore,
    store_path: Option<PathBuf>,
    pub(crate) history_open: bool,
    /// Name field of "Save input".
    pub(crate) save_name: Entity<TextInput>,
    /// Records the input after a pause in typing.
    pending_record: Option<(Tool, Task<()>)>,
    /// The tool whose newest history entry belongs to the current editing
    /// session (it is updated instead of adding one per pause).
    session: Option<Tool>,
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
        Self::with_store(store_path(), window, cx)
    }

    /// Like [`Self::new`], with history and saved inputs in `store_path`
    /// (`None`: in memory only).
    pub fn with_store(
        store_path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| TextInput::new(window, cx).placeholder("Search tools…"));
        let save_name = cx.new(|cx| TextInput::new(window, cx).placeholder("Name"));
        let subscriptions = vec![
            cx.subscribe(&search, |this: &mut Self, _, event, cx| {
                match event {
                    TextInputEvent::Changed => cx.notify(),
                    // Enter opens the first match.
                    TextInputEvent::Submit => {
                        let query = this.search.read(cx).text().to_string();
                        if let Some(tool) = filter_tools(&query).first() {
                            this.select(*tool, cx);
                        }
                    }
                    TextInputEvent::Blurred => {}
                }
            }),
            cx.subscribe(&save_name, |this: &mut Self, _, event, cx| {
                if *event == TextInputEvent::Submit {
                    this.save_input(cx);
                }
            }),
        ];
        let (store, error) = match store_path.as_deref().map(KitStore::load) {
            Some(Ok(store)) => (store, None),
            Some(Err(error)) => (KitStore::default(), Some(error)),
            None => (KitStore::default(), None),
        };
        let mut this = Self {
            search,
            selected: Tool::Json,
            views: HashMap::new(),
            store,
            store_path,
            history_open: false,
            save_name,
            pending_record: None,
            session: None,
            focus_handle: cx.focus_handle(),
            card_focus: cx.focus_handle(),
            sidebar_open: true,
            notice: None,
            notice_task: None,
            _subscriptions: subscriptions,
        };
        if let Some(error) = error {
            this.handle_event(&KitEvent::Notice(Notice::error(error)), cx);
        }
        this.ensure_view(Tool::Json, window, cx);
        this.search
            .update(cx, |search, cx| search.focus(window, cx));
        this
    }

    /// Shows `tool` (its view is built on the next render). A pending
    /// history entry of the previous tool is recorded first.
    pub fn select(&mut self, tool: Tool, cx: &mut Context<Self>) {
        if tool != self.selected {
            self.flush_record(cx);
            self.session = None;
        }
        self.selected = tool;
        cx.notify();
    }

    #[cfg(test)]
    pub fn selected(&self) -> Tool {
        self.selected
    }

    #[cfg(test)]
    pub fn view(&self, tool: Tool) -> Option<&ToolView> {
        self.views.get(&tool)
    }

    fn ensure_view(&mut self, tool: Tool, window: &mut Window, cx: &mut Context<Self>) -> ToolView {
        if let Some(view) = self.views.get(&tool) {
            return view.clone();
        }
        let (view, subscription) = tools::build(tool, window, cx);
        self._subscriptions.push(subscription);
        for input in view.inputs() {
            self._subscriptions
                .push(cx.subscribe(input, move |this: &mut Self, _, event, cx| {
                    if *event == TextInputEvent::Changed {
                        this.input_edited(tool, cx);
                    }
                }));
        }
        self.views.insert(tool, view.clone());
        view
    }

    /// Schedules recording `tool`'s input once typing pauses.
    fn input_edited(&mut self, tool: Tool, cx: &mut Context<Self>) {
        if !self.store.keeps_history(tool) {
            return;
        }
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(RECORD_AFTER).await;
            this.update(cx, |this, cx| {
                this.pending_record = None;
                this.record(tool, cx);
            })
            .ok();
        });
        self.pending_record = Some((tool, task));
    }

    /// Records a scheduled entry now.
    pub fn flush_record(&mut self, cx: &mut Context<Self>) {
        if let Some((tool, _task)) = self.pending_record.take() {
            self.record(tool, cx);
        }
    }

    fn record(&mut self, tool: Tool, cx: &mut Context<Self>) {
        let Some(view) = self.views.get(&tool) else {
            return;
        };
        let values = view.values(cx);
        let now = chrono::Utc::now().timestamp();
        let replace = self.session == Some(tool);
        if self.store.record(tool, &values, now, replace) {
            self.session = Some(tool);
            self.persist(cx);
        }
    }

    /// Puts stored `values` into the selected tool.
    pub fn restore(&mut self, values: Vec<String>, cx: &mut Context<Self>) {
        self.pending_record = None;
        self.session = None;
        if let Some(view) = self.views.get(&self.selected) {
            view.restore(&values, cx);
        }
        cx.notify();
    }

    /// Saves the selected tool's input under the name in the name field.
    pub fn save_input(&mut self, cx: &mut Context<Self>) {
        let tool = self.selected;
        let Some(view) = self.views.get(&tool) else {
            return;
        };
        let values = view.values(cx);
        let name = self.save_name.read(cx).text().to_string();
        match self.store.save_named(tool, &name, &values) {
            Ok(()) => {
                self.save_name
                    .update(cx, |input, cx| input.set_text("", cx));
                self.persist(cx);
                self.show_notice(Notice::info(format!("Saved “{}”", name.trim())), cx);
            }
            Err(error) => self.show_notice(Notice::error(error), cx),
        }
    }

    /// Changes the store with `change` and writes it.
    pub fn update_store(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut KitStore)) {
        change(&mut self.store);
        self.persist(cx);
        cx.notify();
    }

    fn persist(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.store_path.as_deref() else {
            return;
        };
        if let Err(error) = self.store.save(path) {
            self.show_notice(Notice::error(error), cx);
        }
    }

    fn show_notice(&mut self, notice: Notice, cx: &mut Context<Self>) {
        self.handle_event(&KitEvent::Notice(notice), cx);
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

    pub fn toggle_history(&mut self, _: &ToggleHistory, _: &mut Window, cx: &mut Context<Self>) {
        self.flush_record(cx);
        self.history_open = !self.history_open;
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
            .child(ferrum_ui::settings_button("open-settings"))
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
                            this.select(tool, cx);
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
    /// tool's name and what it does, the history toggle.
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
            .child(div().flex_1())
            .child(
                ferrum_ui::title_button(
                    "toggle-history",
                    IconName::Clock,
                    "History and saved inputs (⌘Y)",
                )
                .toggle_state(self.history_open)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.toggle_history(&ToggleHistory, window, cx)
                })),
            )
            // The Ferrum account (Sign in / avatar) at the top right.
            .children(ferrum_account_ui::account_button(cx))
    }
}

impl Render for FerrumKit {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tool = self.selected;
        let view = self.ensure_view(tool, window, cx).view;
        let colors = cx.theme().colors().clone();
        let sidebar_open = self.sidebar_open;
        let active = self.card_focus.contains_focused(window, cx);
        let history = self
            .history_open
            .then(|| crate::history::render(self, tool, cx));
        let card = ferrum_ui::pane_card(active, cx)
            .track_focus(&self.card_focus)
            .flex_1()
            .h_full()
            .child(self.render_tool_header(tool, window, cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .child(div().flex_1().min_w_0().px_4().py_3().child(view))
                    .children(history),
            );
        h_flex()
            .relative()
            .size_full()
            .text_color(colors.text)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::focus_search))
            .on_action(cx.listener(Self::toggle_sidebar))
            .on_action(cx.listener(Self::toggle_history))
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
