//! Root view: the table card (header row with the traffic lights, project,
//! search and toggles; quiet status line at its foot) and the detail card.

use std::path::{Path, PathBuf};
use std::time::Instant;

use chrono::{DateTime, Local};
use ferrum_ui::gpui::{KeyBinding, MouseButton};
use ferrum_ui::prelude::*;
use ferrumports_core::forward::Forward;
use ferrumports_core::history::PortHistory;
use ferrumports_core::{Listener, ListenerKey, Row, View, remote, table};
use ferrumterm_net::ssh_config::ConfigHost;
use ferrumterm_net::{Host, HostStore};

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
    /// Home folder for `~/.ssh/config` and FerrumPorts' files in
    /// `~/.ferrum/ports/`; `None` in tests (nothing is read or written).
    pub home: Option<PathBuf>,
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
    /// The remote machine the process runs on (`None`: this Mac).
    pub host: Option<Host>,
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
    /// The detail card was clicked last (it is drawn a little lighter).
    pub(crate) detail_active: bool,
    /// Machines added in FerrumPorts and the `~/.ssh/config` aliases.
    pub(crate) hosts: HostStore,
    pub(crate) config_hosts: Vec<ConfigHost>,
    pub(crate) hosts_path: Option<PathBuf>,
    /// The remote machine listed (by name); `None` for this Mac.
    pub(crate) target: Option<String>,
    /// When the last scan started (remote machines are scanned less often).
    pub(crate) last_scan: Option<Instant>,
    /// The "Add a machine" dialog's host field while it is open.
    pub(crate) add_host: Option<Entity<TextInput>>,
    /// `ssh -L` forwards started here (stopped when the app quits).
    pub(crate) forwards: Vec<Forward>,
    /// Which process held which port on this Mac, and when.
    pub(crate) history: PortHistory,
    pub(crate) history_path: Option<PathBuf>,
    pub(crate) history_saved: Option<Instant>,
    /// The table shows the history instead of the listeners.
    pub(crate) show_history: bool,
    focus_handle: FocusHandle,
    _poll: Option<Task<()>>,
    pub(crate) _subscriptions: Vec<Subscription>,
}

impl FerrumPorts {
    pub fn new(options: Options, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let project_bar = cx.new(|cx| ProjectBar::new("ferrumports", window, cx).inline(true));
        if let Some(path) = options.project {
            project_bar.update(cx, |bar, cx| bar.set_project(Some(path), cx));
        }
        let search = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Search ports, processes, projects  ⌘F")
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
        let hosts_path = options.home.as_deref().map(remote::store_path);
        let mut load_error = None;
        let hosts = match hosts_path.as_deref().map(HostStore::load) {
            Some(Ok(store)) => store,
            Some(Err(e)) => {
                load_error = Some(e);
                HostStore::default()
            }
            None => HostStore::default(),
        };
        let history_path = options
            .home
            .as_deref()
            .map(|home| home.join(".ferrum").join("ports").join("history.json"));
        let history = match history_path.as_deref().map(PortHistory::load) {
            Some(Ok(history)) => history,
            Some(Err(e)) => {
                load_error = Some(e);
                PortHistory::default()
            }
            None => PortHistory::default(),
        };
        let config_hosts = if options.home.is_some() {
            ferrumterm_net::ssh_config::load_user()
        } else {
            Vec::new()
        };
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
            detail_active: false,
            hosts,
            config_hosts,
            hosts_path,
            target: None,
            last_scan: None,
            add_host: None,
            forwards: Vec::new(),
            history,
            history_path,
            history_saved: None,
            show_history: false,
            focus_handle: cx.focus_handle(),
            _poll: None,
            _subscriptions: subscriptions,
        };
        if let Some(e) = load_error {
            app.show_notice(Notice::error(e), cx);
        }
        // ssh children would outlive the app otherwise.
        app._subscriptions.push(cx.on_app_quit(|app, _| {
            app.stop_forwards();
            app.history.close_all();
            app.save_history();
            async {}
        }));
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

    fn on_cancel(
        &mut self,
        _: &ferrum_ui::menu::Cancel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A dialog closes first (the add-machine one, then the kill one).
        if self.add_host.take().is_some() || self.dialog.take().is_some() {
            window.focus(&self.focus_handle, cx);
        } else {
            self.selected = None;
        }
        cx.notify();
    }
}

impl Render for FerrumPorts {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let text = cx.theme().colors().text;
        let header = chrome::table_header(self, window, cx);
        let forwards = crate::forwards::bar(self, cx);
        let table = if self.show_history {
            crate::history::render(self, cx)
        } else {
            crate::table::render(self, cx)
        };
        let detail = detail::render(self, window, cx);
        let footer = chrome::table_footer(self, cx);
        let dialog =
            chrome::kill_dialog(self, cx).or_else(|| crate::remote::add_host_dialog(self, cx));
        let notice = chrome::notice(self, cx);
        let detail_active = self.detail_active && detail.is_some();
        let table_card = ferrum_ui::pane_card(!detail_active, cx)
            .id("table-card")
            .flex_1()
            .h_full()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|app, _, _, _| app.detail_active = false),
            )
            .child(header)
            .children(forwards)
            .child(table)
            .child(footer);
        let detail_card = detail.map(|detail| {
            ferrum_ui::pane_card(detail_active, cx)
                .id("detail-card")
                .flex_none()
                .h_full()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|app, _, _, _| app.detail_active = true),
                )
                .child(detail)
        });
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
            .text_color(text)
            // No sidebar: the cards start at the top, each with its header.
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .gap(ferrum_ui::PANE_GAP)
                    .p(ferrum_ui::PANE_GAP)
                    .child(table_card)
                    .children(detail_card),
            )
            .children(dialog)
            .children(notice)
    }
}

impl Focusable for FerrumPorts {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
