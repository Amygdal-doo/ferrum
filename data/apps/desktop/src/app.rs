//! Root view and window layout.

use crate::commands::keys::KeyInputs;
use crate::components::connection::{ConnectionForm, ConnectionFormEvent};
use crate::components::sql_editor::{CompletionPopup, apply_highlights};
use crate::state::{AppState, Dialog, ToolTab};
use ferrum_ui::gpui::{Axis, MouseDownEvent, Pixels, UniformListScrollHandle};
use ferrum_ui::prelude::*;
use ferrumdata_application::AppServices;
use ferrumdata_domain::{DatabaseEngine, EditorTabId, ProfileId};
use std::collections::HashMap;

actions!(ferrum_data, [ToggleSidebar]);

const SIDEBAR_SPLITTER: &str = "sidebar";
const RESULTS_SPLITTER: &str = "results";

/// A tab's text input plus the subscription that mirrors it into state.
pub struct TabInput {
    pub input: Entity<TextInput>,
    /// Engine whose query syntax the placeholder currently describes.
    engine: Option<DatabaseEngine>,
    _subscription: Subscription,
}

/// Empty-editor placeholder: documents the input syntax of the tab's engine.
fn editor_placeholder(engine: Option<DatabaseEngine>) -> &'static str {
    engine.map_or(
        DatabaseEngine::Postgres.editor_placeholder(),
        DatabaseEngine::editor_placeholder,
    )
}

pub struct FerrumData {
    pub services: AppServices,
    pub state: AppState,
    pub focus_handle: FocusHandle,
    /// Multi-line SQL editors for query tabs, created on first display.
    pub editors: HashMap<EditorTabId, TabInput>,
    /// One-line SQL bars for table-browser tabs.
    pub table_inputs: HashMap<EditorTabId, TabInput>,
    /// Value editor and TTL box of key tabs (Redis).
    pub key_inputs: HashMap<EditorTabId, KeyInputs>,
    /// SCAN pattern box above each Redis connection's key tree.
    pub key_patterns: HashMap<ProfileId, (Entity<TextInput>, Subscription)>,
    pub filter: Entity<TextInput>,
    /// Search box of the History panel.
    pub history_filter: Entity<TextInput>,
    /// Focus target of the results grid (⌘C / ⌘A while it has focus).
    pub grid_focus: FocusHandle,
    pub form: Option<Entity<ConnectionForm>>,
    pub grid_scroll: UniformListScrollHandle,
    pub notice: Option<Notice>,
    /// Shared Ferrum project bar, inline under the sidebar's title row.
    pub project_bar: Entity<ProjectBar>,
    /// Connection the bar was last set from (`Some(None)`: none).
    pub(crate) project_synced: Option<Option<String>>,
    /// FerrumChat beside the cards (see `agent`).
    pub(crate) agent: ferrumagent_ui::AgentPane,
    /// The connection the agent's tools use, set before each message.
    pub(crate) agent_profile: crate::agent::ProfileSlot,
    /// Open completion list of the SQL editor.
    pub(crate) completion: Option<CompletionPopup>,
    /// Set while a completion is inserted, so the edit does not reopen the list.
    pub(crate) accepting_completion: bool,
    /// Canvas bounds of the ER diagram, as last painted (pan / zoom maths).
    pub(crate) diagram_bounds: std::rc::Rc<std::cell::Cell<ferrum_ui::gpui::Bounds<Pixels>>>,
    /// Inline editor of a table tab's cell (row editing).
    pub(crate) cell_editor: Option<crate::commands::edit::CellEditor>,
    notice_task: Option<Task<()>>,
    _autosave: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl FerrumData {
    pub fn new(services: AppServices, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter =
            cx.new(|cx| TextInput::new(window, cx).placeholder("Filter connections and tables"));
        let filter_changed = cx.observe(&filter, |_, _, cx| cx.notify());
        let history_filter =
            cx.new(|cx| TextInput::new(window, cx).placeholder("Search query history"));
        // The panel reloads on render when the search text differs from the loaded one.
        let history_changed = cx.observe(&history_filter, |_, _, cx| cx.notify());
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        crate::components::result_grid::bind_grid_keys(cx);
        crate::components::sql_editor::bind_editor_keys(cx);
        cx.bind_keys([
            KeyBinding::new("cmd-b", ToggleSidebar, None),
            KeyBinding::new("ctrl-b", ToggleSidebar, None),
        ]);
        let project_bar = crate::components::shell::project::new_bar(window, cx);

        let mut this = Self {
            services,
            state: AppState::default(),
            focus_handle,
            editors: HashMap::new(),
            table_inputs: HashMap::new(),
            key_inputs: HashMap::new(),
            key_patterns: HashMap::new(),
            filter,
            history_filter,
            grid_focus: cx.focus_handle(),
            form: None,
            grid_scroll: UniformListScrollHandle::new(),
            notice: None,
            project_bar,
            project_synced: None,
            agent: ferrumagent_ui::AgentPane::new(),
            agent_profile: crate::agent::ProfileSlot::default(),
            completion: None,
            accepting_completion: false,
            cell_editor: None,
            diagram_bounds: std::rc::Rc::default(),
            notice_task: None,
            _autosave: Task::ready(()),
            _subscriptions: vec![filter_changed, history_changed],
        };
        this.bootstrap(cx);
        this._autosave = this.autosave_loop(cx);
        this
    }

    pub fn notify_user(&mut self, message: impl Into<String>, error: bool, cx: &mut Context<Self>) {
        let message: SharedString = message.into().into();
        self.notice = Some(if error {
            Notice::error(message)
        } else {
            Notice::info(message)
        });
        self.notice_task = Some(Notice::dismiss_later(cx, |this: &mut Self, _| {
            this.notice = None;
        }));
        cx.notify();
    }

    pub fn open_dialog(&mut self, dialog: Dialog, window: &mut Window, cx: &mut Context<Self>) {
        self.form = match &dialog {
            Dialog::ConnectionForm(existing) => {
                let services = self.services.clone();
                let existing = existing.clone();
                let form = cx.new(|cx| ConnectionForm::new(services, existing, window, cx));
                self._subscriptions
                    .push(cx.subscribe_in(&form, window, Self::on_form_event));
                Some(form)
            }
            _ => None,
        };
        self.state.dialog = Some(dialog);
        cx.notify();
    }

    pub fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.dialog = None;
        self.form = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn on_form_event(
        &mut self,
        _: &Entity<ConnectionForm>,
        event: &ConnectionFormEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ConnectionFormEvent::Cancelled => self.close_dialog(window, cx),
            ConnectionFormEvent::Saved(saved) => {
                self.state.conn_status.remove(&saved.id);
                // Bind the active tab to a newly created profile.
                if let Some(tab_id) = self.state.active_tab {
                    let id = saved.id;
                    self.state.update_tab(tab_id, |t| {
                        if t.profile_id.is_none() {
                            t.profile_id = Some(id);
                            t.unsaved = true;
                        }
                    });
                }
                self.reload_profiles(cx);
                self.close_dialog(window, cx);
            }
        }
    }

    /// The editor (or table SQL bar) for a tab, created on first use.
    pub fn tab_input(
        &mut self,
        tab_id: EditorTabId,
        table: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        let engine = self
            .state
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.profile_id)
            .and_then(|id| self.state.profile(id))
            .map(|p| p.engine);
        let existing = if table {
            self.table_inputs.get_mut(&tab_id)
        } else {
            self.editors.get_mut(&tab_id)
        };
        if let Some(existing) = existing {
            // The tab's connection may have changed to another engine.
            if !table && existing.engine != engine {
                existing.engine = engine;
                existing.input.update(cx, |i, cx| {
                    i.set_placeholder(editor_placeholder(engine), cx)
                });
                apply_highlights(&existing.input, engine, cx);
            }
            return existing.input.clone();
        }
        let sql = self
            .state
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .map(|t| t.sql.clone())
            .unwrap_or_default();
        let input = cx.new(|cx| {
            let input = TextInput::new(window, cx).monospace().with_text(sql);
            if table {
                input
            } else {
                input
                    .multiline(1)
                    .borderless()
                    .placeholder(editor_placeholder(engine))
            }
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            move |this, input, event, _window, cx| match event {
                TextInputEvent::Changed => {
                    let sql = input.read(cx).text().to_owned();
                    this.state.update_tab(tab_id, |t| {
                        t.sql = sql;
                        t.unsaved = true;
                    });
                    if table {
                        let engine = this.tab_engine(tab_id).1;
                        apply_highlights(input, engine, cx);
                    } else {
                        this.editor_changed(tab_id, cx);
                    }
                    cx.notify();
                }
                TextInputEvent::Submit => this.run_from_editor(cx),
                TextInputEvent::Blurred => {
                    if !table {
                        this.dismiss_completion(cx);
                    }
                }
            },
        );
        apply_highlights(&input, engine, cx);
        let entry = TabInput {
            input: input.clone(),
            engine,
            _subscription: subscription,
        };
        if table {
            self.table_inputs.insert(tab_id, entry);
        } else {
            self.editors.insert(tab_id, entry);
        }
        input
    }

    fn on_drag_sidebar(
        &mut self,
        event: &DragMoveEvent<DraggedSplitter>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.drag(cx).id.as_ref() != SIDEBAR_SPLITTER {
            return;
        }
        let layout = &mut self.state.settings.layout;
        layout.sidebar_width = f64::from(f32::from(event.event.position.x - event.bounds.left()));
        *layout = layout.clamped();
        cx.notify();
    }

    fn on_drag_results(
        &mut self,
        event: &DragMoveEvent<DraggedSplitter>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.drag(cx).id.as_ref() != RESULTS_SPLITTER {
            return;
        }
        let layout = &mut self.state.settings.layout;
        layout.results_height =
            f64::from(f32::from(event.bounds.bottom() - event.event.position.y));
        *layout = layout.clamped();
        cx.notify();
    }

    /// Sidebar on the window backdrop, then the cards: the editor (tabs in
    /// its header) and the results, or one card for a table / key tab.
    fn render_main(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let layout = self.state.settings.layout.clamped();
        let gap = ferrum_ui::PANE_GAP;
        let sidebar_open = !self.state.sidebar_hidden;
        let table_tab = self.state.active_tab().is_some_and(|t| t.table.is_some());
        let key_tab = self.state.active_tab().is_some_and(|t| t.key.is_some());
        let tool_tab = self
            .state
            .active_tab()
            .and_then(|t| t.tool.map(|tool| (t.id, tool)));
        let results_active = self.state.results_active;
        let use_editor = cx.listener(|this, _: &MouseDownEvent, _, cx| {
            if this.state.results_active {
                this.state.results_active = false;
                cx.notify();
            }
        });
        let main = if table_tab || key_tab || tool_tab.is_some() {
            // Tabs stay in the header; the table/key/tool view replaces editor + results.
            let view = if let Some((tab_id, tool)) = tool_tab {
                match tool {
                    ToolTab::Diagram => self.render_diagram_view(tab_id, cx),
                }
            } else if key_tab {
                self.render_key_view(window, cx)
            } else {
                self.render_table_view(window, cx)
            };
            ferrum_ui::pane_card(true, cx)
                .flex_1()
                .w_full()
                .child(self.render_tab_bar(window, cx))
                .child(view)
                .into_any_element()
        } else {
            let use_results = cx.listener(|this, _: &MouseDownEvent, _, cx| {
                if !this.state.results_active {
                    this.state.results_active = true;
                    cx.notify();
                }
            });
            v_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .on_drag_move(cx.listener(Self::on_drag_results))
                .child(
                    ferrum_ui::pane_card(!results_active, cx)
                        .flex_1()
                        .w_full()
                        .capture_any_mouse_down(use_editor)
                        .child(self.render_editor_pane(window, cx)),
                )
                .child(ferrum_ui::gap_splitter(
                    RESULTS_SPLITTER,
                    Axis::Vertical,
                    cx,
                ))
                .child(
                    ferrum_ui::pane_card(results_active, cx)
                        .flex_none()
                        .w_full()
                        .h(px(layout.results_height as f32))
                        .capture_any_mouse_down(use_results)
                        .child(self.render_results(window, cx)),
                )
                .into_any_element()
        };

        h_flex()
            .flex_1()
            .min_h_0()
            .items_stretch()
            .on_drag_move(cx.listener(Self::on_drag_sidebar))
            .when(sidebar_open, |row| {
                row.child(
                    div()
                        .flex_none()
                        .h_full()
                        .overflow_hidden()
                        .w(px(layout.sidebar_width as f32))
                        .child(self.render_sidebar(window, cx)),
                )
                .child(ferrum_ui::gap_splitter(
                    SIDEBAR_SPLITTER,
                    Axis::Horizontal,
                    cx,
                ))
            })
            .child(
                // The Agent pane, when open, is a card right of the others.
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .pt(gap)
                    .pr(gap)
                    .when(!sidebar_open, |main| main.pl(gap))
                    .gap(gap)
                    .child(v_flex().flex_1().min_w_0().h_full().child(main))
                    .children(self.agent.card(cx)),
            )
            .into_any_element()
    }
}

impl Render for FerrumData {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        if self.state.loaded {
            self.sync_project_bar(cx);
        }
        let body = if self.state.loaded {
            self.render_main(window, cx)
        } else {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Label::new("Loading workspace…")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element()
        };
        let status = self.render_status_bar(cx);
        let dialog = self.render_dialog(cx);

        v_flex()
            .relative()
            .size_full()
            .track_focus(&self.focus_handle)
            .key_context("FerrumData")
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| this.toggle_sidebar(cx)))
            .text_color(colors.text)
            .on_action(
                cx.listener(|this, _: &ferrum_ui::menu::Cancel, window, cx| {
                    if this.state.dialog.is_some() {
                        this.close_dialog(window, cx);
                    } else {
                        cx.propagate();
                    }
                }),
            )
            .on_drop(cx.listener(|this, _: &DraggedSplitter, _, cx| {
                this.save_settings(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleProjectSwitcher, window, cx| {
                this.project_bar
                    .update(cx, |bar, cx| bar.toggle_switcher(window, cx));
            }))
            .child(body)
            .child(status)
            .children(dialog)
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

impl Focusable for FerrumData {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
