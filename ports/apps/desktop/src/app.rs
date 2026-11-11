//! Root view: project strip / toolbar / table | detail / status bar.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use ferrum_ui::gpui::KeyBinding;
use ferrum_ui::prelude::*;
use ferrumports_core::{Listener, ListenerKey, Row, View, table};

use crate::{chrome, detail};

actions!(ferrum_ports, [Refresh, FocusSearch, TogglePause]);

const KEY_CONTEXT: &str = "FerrumPorts";

pub fn bind_keys(cx: &mut App) {
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("cmd-r", Refresh, context),
        KeyBinding::new("ctrl-r", Refresh, context),
        KeyBinding::new("cmd-f", FocusSearch, context),
        KeyBinding::new("ctrl-f", FocusSearch, context),
        KeyBinding::new("cmd-shift-p", TogglePause, context),
    ]);
}

pub struct Options {
    /// Project shown in the strip at start.
    pub project: Option<PathBuf>,
    /// Scan now and every [`crate::commands::POLL`]; off in tests.
    pub autostart: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KillStage {
    /// Ask for SIGTERM.
    Term,
    /// SIGTERM didn't end it: offer SIGKILL.
    Force,
}

/// The kill confirmation; `listener` is a copy, so a refresh that drops
/// the row does not close the dialog.
pub struct KillDialog {
    pub listener: Listener,
    pub stage: KillStage,
    /// A signal is in flight.
    pub busy: bool,
}

pub struct FerrumPorts {
    pub(crate) project_bar: Entity<ProjectBar>,
    pub(crate) search: Entity<TextInput>,
    pub(crate) port_input: Entity<TextInput>,
    pub(crate) listeners: Vec<Listener>,
    pub(crate) selected: Option<ListenerKey>,
    pub(crate) grouped: bool,
    pub(crate) only_project: bool,
    pub(crate) udp: bool,
    pub(crate) paused: bool,
    pub(crate) scanning: bool,
    /// Bumped per scan; only the newest scan's result is applied.
    pub(crate) generation: u64,
    /// At least one scan finished (before that the table says "Scanning…").
    pub(crate) loaded: bool,
    pub(crate) last_refresh: Option<DateTime<Local>>,
    /// The last scan failed (e.g. `lsof` missing); the old rows stay.
    pub(crate) error: Option<String>,
    pub(crate) warnings: Vec<String>,
    pub(crate) dialog: Option<KillDialog>,
    pub(crate) notice: Option<Notice>,
    pub(crate) notice_task: Option<Task<()>>,
    focus_handle: FocusHandle,
    _poll: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl FerrumPorts {
    pub fn new(options: Options, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let project_bar = cx.new(|cx| ProjectBar::new("ferrumports", window, cx));
        if let Some(path) = options.project {
            project_bar.update(cx, |bar, cx| bar.set_project(Some(path), cx));
        }
        let search = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Search port, process, command, project")
                .borderless()
        });
        let port_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Port free?")
                .borderless()
        });
        let subscriptions = vec![
            cx.subscribe(&project_bar, |_: &mut Self, bar, event, cx| match event {
                ProjectBarEvent::Open(path) => {
                    let path = path.clone();
                    bar.update(cx, |bar, cx| bar.set_project(Some(path), cx));
                    cx.notify();
                }
            }),
            cx.subscribe(&search, |_: &mut Self, _, event, cx| {
                if *event == TextInputEvent::Changed {
                    cx.notify();
                }
            }),
            cx.subscribe(&port_input, |app: &mut Self, _, event, cx| {
                if *event == TextInputEvent::Submit {
                    app.check_port(cx);
                }
            }),
        ];
        let mut app = Self {
            project_bar,
            search,
            port_input,
            listeners: Vec::new(),
            selected: None,
            grouped: false,
            only_project: false,
            udp: false,
            paused: false,
            scanning: false,
            generation: 0,
            loaded: false,
            last_refresh: None,
            error: None,
            warnings: Vec::new(),
            dialog: None,
            notice: None,
            notice_task: None,
            focus_handle: cx.focus_handle(),
            _poll: None,
            _subscriptions: subscriptions,
        };
        window.focus(&app.focus_handle, cx);
        if options.autostart {
            app._poll = Some(Self::start_polling(cx));
            app.refresh(cx);
        }
        app
    }

    /// The project in the strip.
    pub fn project(&self, cx: &App) -> Option<PathBuf> {
        self.project_bar.read(cx).project().map(Path::to_path_buf)
    }

    /// Table rows for the current search, project filter and grouping.
    pub fn rows(&self, cx: &App) -> Vec<Row> {
        let project = self.only_project.then(|| self.project(cx)).flatten();
        let query = self.search.read(cx).text().to_string();
        table::rows(
            &self.listeners,
            View {
                query: &query,
                project: project.as_deref(),
                grouped: self.grouped,
            },
        )
    }

    pub fn selected_listener(&self) -> Option<&Listener> {
        let key = self.selected?;
        self.listeners.iter().find(|l| l.key() == key)
    }

    fn on_refresh(&mut self, _: &Refresh, _: &mut Window, cx: &mut Context<Self>) {
        self.refresh(cx);
    }

    fn on_toggle_pause(&mut self, _: &TogglePause, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_pause(cx);
    }

    fn on_focus_search(&mut self, _: &FocusSearch, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |input, cx| {
            let len = input.text().len();
            input.select_range(0..len, cx);
            input.focus(window, cx);
        });
    }

    fn on_cancel(&mut self, _: &menu::Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog.take().is_some() {
            window.focus(&self.focus_handle, cx);
        } else {
            self.selected = None;
        }
        cx.notify();
    }
}

impl Render for FerrumPorts {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let (background, text) = (colors.background, colors.text);
        let toolbar = chrome::toolbar(self, cx);
        let table = crate::table::render(self, cx);
        let detail = detail::render(self, cx);
        let status = chrome::status_bar(self, cx);
        let dialog = chrome::kill_dialog(self, cx);
        let notice = chrome::notice(self, cx);
        v_flex()
            .id("ferrumports")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_refresh))
            .on_action(cx.listener(Self::on_toggle_pause))
            .on_action(cx.listener(Self::on_focus_search))
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(|this, _: &ToggleProjectSwitcher, window, cx| {
                this.project_bar
                    .update(cx, |bar, cx| bar.toggle_switcher(window, cx))
            }))
            .relative()
            .size_full()
            .bg(background)
            .text_color(text)
            .child(self.project_bar.clone())
            .child(toolbar)
            .child(h_flex().flex_1().min_h_0().child(table).children(detail))
            .child(status)
            .children(dialog)
            .children(notice)
    }
}

impl Focusable for FerrumPorts {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
