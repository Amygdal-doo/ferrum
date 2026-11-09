//! Root view and window layout.

use crate::components::connection::{ConnectionForm, ConnectionFormEvent};
use crate::state::{AppState, Dialog};
use ferrum_ui::gpui::{Axis, UniformListScrollHandle};
use ferrum_ui::prelude::*;
use ferrumdata_application::AppServices;
use ferrumdata_domain::{DatabaseEngine, EditorTabId};
use std::collections::HashMap;

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
    pub filter: Entity<TextInput>,
    pub form: Option<Entity<ConnectionForm>>,
    pub grid_scroll: UniformListScrollHandle,
    pub notice: Option<Notice>,
    notice_task: Option<Task<()>>,
    _autosave: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl FerrumData {
    pub fn new(services: AppServices, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter =
            cx.new(|cx| TextInput::new(window, cx).placeholder("Filter connections and tables"));
        let filter_changed = cx.observe(&filter, |_, _, cx| cx.notify());
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);

        let mut this = Self {
            services,
            state: AppState::default(),
            focus_handle,
            editors: HashMap::new(),
            table_inputs: HashMap::new(),
            filter,
            form: None,
            grid_scroll: UniformListScrollHandle::new(),
            notice: None,
            notice_task: None,
            _autosave: Task::ready(()),
            _subscriptions: vec![filter_changed],
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
                    cx.notify();
                }
                TextInputEvent::Submit => this.run_from_editor(cx),
                TextInputEvent::Blurred => {}
            },
        );
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

    fn render_main(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let layout = self.state.settings.layout.clamped();
        let table_tab = self.state.active_tab().is_some_and(|t| t.table.is_some());
        let main = if table_tab {
            // Tab bar stays visible; the table view replaces editor + results.
            v_flex()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .child(self.render_tab_bar(cx))
                .child(self.render_table_view(window, cx))
                .into_any_element()
        } else {
            v_flex()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .on_drag_move(cx.listener(Self::on_drag_results))
                .child(self.render_editor_pane(window, cx))
                .child(splitter(RESULTS_SPLITTER, Axis::Vertical, cx))
                .child(
                    div()
                        .flex_none()
                        .h(px(layout.results_height as f32))
                        .child(self.render_results(cx)),
                )
                .into_any_element()
        };

        h_flex()
            .flex_1()
            .min_h_0()
            .items_stretch()
            .on_drag_move(cx.listener(Self::on_drag_sidebar))
            .child(
                div()
                    .flex_none()
                    .h_full()
                    .overflow_hidden()
                    .w(px(layout.sidebar_width as f32))
                    .child(self.render_sidebar(window, cx)),
            )
            .child(splitter(SIDEBAR_SPLITTER, Axis::Horizontal, cx))
            .child(main)
            .into_any_element()
    }
}

impl Render for FerrumData {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
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
        let header = self.render_header(window, cx);
        let status = self.render_status_bar(cx);
        let dialog = self.render_dialog(cx);

        v_flex()
            .relative()
            .size_full()
            .track_focus(&self.focus_handle)
            .key_context("FerrumData")
            .bg(colors.background)
            .text_color(colors.text)
            .on_action(cx.listener(|this, _: &menu::Cancel, window, cx| {
                if this.state.dialog.is_some() {
                    this.close_dialog(window, cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_drop(cx.listener(|this, _: &DraggedSplitter, _, cx| {
                this.save_settings(cx);
            }))
            .child(header)
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
