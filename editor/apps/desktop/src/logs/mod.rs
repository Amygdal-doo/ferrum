//! Logs in the editor (formerly the FerrumLogs app): live logs per project
//! from files, dev-server commands and containers, using `ferrumlogs-core`.
//! The page is sidebar | log view / status line. It owns one [`Session`]
//! per opened project (sources keep running when the editor switches
//! folders) and pumps runner events into them in batches. User commands
//! are in `commands`. Created the first time the Logs page opens (⌥⌘L).

mod commands;
mod dialog;
mod log_view;
mod sidebar;
mod widgets;

use dialog::SourceDialog;
use ferrum_ui::gpui::UniformListScrollHandle;
use ferrum_ui::prelude::*;
use ferrum_ui::project::{self, RecentProject};
use ferrumlogs_core::runner::{self, Event, EventReceiver, RunSpec};
use ferrumlogs_core::{Launcher, Levels, Session, SourceConfig, Store, store, suggest};
use futures::StreamExt;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

actions!(
    ferrum_logs,
    [
        FocusSearch,
        CopySelection,
        ClearLogs,
        ToggleErrorsOnly,
        JumpToLatest,
        ExportLogs
    ]
);

/// Key context of the Logs page (its shortcuts bind only here).
pub const KEY_CONTEXT: &str = "FerrumLogs";
/// Coalesces runner output into at most ~20 renders per second.
pub const BATCH_DELAY: Duration = Duration::from_millis(50);
/// Events applied per batch before yielding to rendering.
const MAX_BATCH: usize = 4096;

/// The page's shortcuts. The editor rebuilds its bindings whenever the
/// keymap reloads, so `actions::apply_keymap` adds these every time.
pub fn key_bindings() -> Vec<KeyBinding> {
    let context = Some(KEY_CONTEXT);
    vec![
        KeyBinding::new("cmd-f", FocusSearch, context),
        KeyBinding::new("ctrl-f", FocusSearch, context),
        KeyBinding::new("cmd-c", CopySelection, context),
        KeyBinding::new("cmd-k", ClearLogs, context),
        KeyBinding::new("ctrl-l", ClearLogs, context),
        KeyBinding::new("cmd-e", ToggleErrorsOnly, context),
        KeyBinding::new("cmd-down", JumpToLatest, context),
        KeyBinding::new("end", JumpToLatest, context),
        KeyBinding::new("cmd-s", ExportLogs, context),
    ]
}

/// Where state lives and how commands run (tests use temp files and `/bin/sh`).
pub struct Options {
    /// `~/.ferrum/logs/projects.json`.
    pub store_file: Option<PathBuf>,
    /// `~/.ferrum/projects.json` (recent projects, shared by every app).
    pub recent_file: Option<PathBuf>,
    pub shell: PathBuf,
    pub login: bool,
    pub home: Option<PathBuf>,
}

impl Options {
    pub fn user() -> Self {
        Self {
            store_file: store::store_file(),
            recent_file: project::projects_file(),
            shell: RunSpec::user_shell(),
            login: true,
            home: project::home_dir(),
        }
    }
}

/// Selected lines, by `seq` (survives filtering and new output).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: u64,
    pub head: u64,
}

impl Selection {
    pub fn range(&self) -> std::ops::RangeInclusive<u64> {
        self.anchor.min(self.head)..=self.anchor.max(self.head)
    }
}

/// Asks the workspace to open another project (picked in the sidebar).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogsEvent {
    OpenProject(PathBuf),
}

impl EventEmitter<LogsEvent> for LogsView {}

pub struct LogsView {
    pub(crate) options: Options,
    pub(crate) store: Store,
    pub(crate) launcher: Launcher,
    pub(crate) sessions: HashMap<PathBuf, Session>,
    pub(crate) current: Option<PathBuf>,
    pub(crate) recent: Vec<RecentProject>,
    /// For the current project, while it has no sources.
    pub(crate) suggestions: Vec<SourceConfig>,
    pub(crate) search: Entity<TextInput>,
    pub(crate) scroll: UniformListScrollHandle,
    /// Auto-scroll to new lines; off once the user scrolls up.
    pub(crate) follow: bool,
    /// Lines that arrived while not following.
    pub(crate) unseen: usize,
    /// The user scrolled down: resume following if that reached the end.
    pub(crate) resume_check: bool,
    pub(crate) selection: Option<Selection>,
    pub(crate) dialog: Option<SourceDialog>,
    /// Levels before "errors only" was switched on.
    pub(crate) saved_levels: Option<Levels>,
    pub(crate) notice: Option<Notice>,
    pub(crate) notice_task: Option<Task<()>>,
    focus_handle: FocusHandle,
    _pump: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl LogsView {
    pub fn new(
        options: Options,
        project: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Search logs")
                .borderless()
        });
        let subscriptions = vec![
            cx.subscribe(&search, |this: &mut Self, input, event, cx| {
                if *event == TextInputEvent::Changed {
                    let query = input.read(cx).text().to_owned();
                    this.update_filter(cx, |f| f.query = query);
                }
            }),
            cx.on_app_quit(|this: &mut Self, _| {
                for session in this.sessions.values_mut() {
                    session.stop_all();
                }
                async {}
            }),
        ];
        let (tx, rx) = runner::channel();
        let launcher = Launcher::new(
            tx,
            options.shell.clone(),
            options.login,
            options.home.clone(),
        );
        let store = options
            .store_file
            .as_deref()
            .map(Store::load)
            .unwrap_or_default();
        let recent = options
            .recent_file
            .as_deref()
            .map(project::load)
            .unwrap_or_default();
        let mut this = Self {
            options,
            store,
            launcher,
            sessions: HashMap::new(),
            current: None,
            recent,
            suggestions: Vec::new(),
            search,
            scroll: UniformListScrollHandle::new(),
            follow: true,
            unseen: 0,
            resume_check: false,
            selection: None,
            dialog: None,
            saved_levels: None,
            notice: None,
            notice_task: None,
            focus_handle: cx.focus_handle(),
            _pump: Self::pump(rx, cx),
            _subscriptions: subscriptions,
        };
        if let Some(project) = project {
            this.open_project(project, cx);
        }
        this
    }

    /// Drains runner events in batches and applies them.
    fn pump(mut rx: EventReceiver, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            while let Some(first) = rx.next().await {
                let mut batch = vec![first];
                while batch.len() < MAX_BATCH {
                    match rx.try_recv() {
                        Ok(event) => batch.push(event),
                        Err(_) => break,
                    }
                }
                if this
                    .update(cx, |app, cx| app.apply_events(batch, cx))
                    .is_err()
                {
                    return;
                }
                cx.background_executor().timer(BATCH_DELAY).await;
            }
        })
    }

    pub(crate) fn apply_events(&mut self, events: Vec<Event>, cx: &mut Context<Self>) {
        let mut routed: HashMap<PathBuf, Vec<Event>> = HashMap::new();
        for event in events {
            let run = event.run();
            if let Some((path, _)) = self.sessions.iter().find(|(_, s)| s.owns_run(run)) {
                routed.entry(path.clone()).or_default().push(event);
            }
        }
        for (path, events) in routed {
            let Some(session) = self.sessions.get_mut(&path) else {
                continue;
            };
            let added = session.apply(events);
            if Some(&path) == self.current.as_ref() && added > 0 {
                if self.follow {
                    self.scroll_to_end();
                } else {
                    self.unseen += added;
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn session(&self) -> Option<&Session> {
        self.sessions.get(self.current.as_ref()?)
    }

    pub(crate) fn session_mut(&mut self) -> Option<&mut Session> {
        self.sessions.get_mut(self.current.as_ref()?)
    }

    /// Follows the editor's project (`None`: no folder open). The same
    /// project again keeps the view as it is.
    pub fn set_project(&mut self, path: Option<PathBuf>, cx: &mut Context<Self>) {
        match path {
            Some(path) if self.current.as_ref() != Some(&path) => self.open_project(path, cx),
            Some(_) => {}
            None => {
                self.current = None;
                self.suggestions.clear();
                cx.notify();
            }
        }
    }

    /// Switches to `path`, starting its autostart sources the first time.
    pub fn open_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.recent = self
            .options
            .recent_file
            .as_deref()
            .map(project::load)
            .unwrap_or_default();
        if !self.sessions.contains_key(&path) {
            let mut session = Session::new(&path, self.store.sources(&path));
            let autostart: Vec<String> = session
                .sources
                .iter()
                .filter(|s| s.config.autostart)
                .map(|s| s.config.id.clone())
                .collect();
            for id in autostart {
                session.start(&id, &self.launcher);
            }
            self.sessions.insert(path.clone(), session);
        }
        self.current = Some(path);
        self.selection = None;
        self.unseen = 0;
        self.follow = true;
        self.refresh_suggestions();
        let query = self
            .session()
            .map(|s| s.filter().query.clone())
            .unwrap_or_default();
        self.search.update(cx, |input, cx| {
            if input.text() != query {
                input.set_text(query, cx);
            }
        });
        self.scroll_to_end();
        cx.notify();
    }

    fn refresh_suggestions(&mut self) {
        self.suggestions = match self.session() {
            Some(s) if s.sources.is_empty() => suggest::suggest(&s.project, &[]),
            _ => Vec::new(),
        };
    }

    /// Saves the current project's sources to the store file.
    pub(crate) fn persist(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session() else {
            return;
        };
        let (project, configs) = (session.project.clone(), session.configs());
        self.store.set_sources(&project, configs);
        if let Some(file) = &self.options.store_file
            && let Err(e) = self.store.save(file)
        {
            self.notify(Notice::error(format!("Could not save sources: {e}")), cx);
        }
        self.refresh_suggestions();
    }

    pub fn open_dialog(
        &mut self,
        editing: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let config = editing
            .and_then(|id| self.session()?.source(id))
            .map(|s| s.config.clone());
        self.dialog = Some(SourceDialog::new(config, window, cx));
        cx.notify();
    }

    pub fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(crate) fn project_path(&self) -> Option<&Path> {
        self.current.as_deref()
    }
}

impl Focusable for LogsView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for LogsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.check_resume();
        let colors = cx.theme().colors();
        v_flex()
            .relative()
            .size_full()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .bg(colors.background)
            .text_color(colors.text)
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.search.update(cx, |input, cx| {
                    let len = input.text().len();
                    input.select_range(0..len, cx);
                    input.focus(window, cx);
                });
            }))
            .on_action(cx.listener(|this, _: &CopySelection, _, cx| this.copy_selection(cx)))
            .on_action(cx.listener(|this, _: &ClearLogs, _, cx| this.clear(cx)))
            .on_action(cx.listener(|this, _: &ToggleErrorsOnly, _, cx| {
                this.toggle_errors_only(cx);
            }))
            .on_action(cx.listener(|this, _: &JumpToLatest, _, cx| this.set_follow(true, cx)))
            .on_action(cx.listener(|this, _: &ExportLogs, _, cx| this.export(cx)))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(sidebar::render(self, cx))
                    .child(log_view::render(self, window, cx)),
            )
            .child(log_view::status_line(self, cx))
            .when(self.dialog.is_some(), |root| {
                root.child(dialog::render(self, cx))
            })
            .when_some(self.notice.as_ref(), |root, notice| {
                root.child(render_notice(
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
