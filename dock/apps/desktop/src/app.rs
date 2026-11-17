//! Root view and window layout: sidebar (title row, project, sections) |
//! section card (header row, body, status line) and the container detail card.

use crate::services::Services;
use crate::state::{AppState, EngineStatus};
use crate::views;
use ferrum_ui::components::{h_flex, v_flex};
use ferrum_ui::theme::ActiveTheme;
use ferrum_ui::{ProjectBar, ProjectBarEvent, TextInput, TextInputEvent, ToggleProjectSwitcher};
use ferrumdock_engine::DockerCli;
use gpui::{
    App, Context, Entity, FocusHandle, KeyBinding, Subscription, Task, UniformListScrollHandle,
    Window, actions, prelude::*,
};

actions!(ferrumdock, [Refresh, FocusFilter, ToggleSidebar]);

/// The executable name recorded in `~/.ferrum/projects.json`.
pub const APP_BINARY: &str = "ferrumdock";

const KEY_CONTEXT: &str = "FerrumContainers";

pub fn bind_keys(cx: &mut App) {
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("cmd-r", Refresh, context),
        KeyBinding::new("ctrl-r", Refresh, context),
        KeyBinding::new("cmd-f", FocusFilter, context),
        KeyBinding::new("ctrl-f", FocusFilter, context),
        KeyBinding::new("cmd-b", ToggleSidebar, context),
    ]);
}

/// Text inputs that live as long as the window.
pub struct Inputs {
    pub filter: Entity<TextInput>,
    pub log_search: Entity<TextInput>,
    pub pull: Entity<TextInput>,
    pub run: RunInputs,
    /// New reference in the tag dialog.
    pub tag: Entity<TextInput>,
    pub registry: RegistryInputs,
    /// New `ssh://` Docker context in the engine picker.
    pub context_name: Entity<TextInput>,
    pub context_host: Entity<TextInput>,
    /// The folder shown in the Files tab (editable).
    pub files_path: Entity<TextInput>,
    /// The file viewer (read-only).
    pub file_view: Entity<TextInput>,
    /// The compose file editor (YAML highlighted).
    pub compose_editor: Entity<TextInput>,
    /// A pod's logs (read-only).
    pub pod_logs: Entity<TextInput>,
}

/// The registry sign-in form.
pub struct RegistryInputs {
    pub server: Entity<TextInput>,
    pub username: Entity<TextInput>,
    /// Masked; cleared after every attempt.
    pub secret: Entity<TextInput>,
}

impl RegistryInputs {
    fn new(window: &mut Window, cx: &mut Context<FerrumContainers>) -> Self {
        let server = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("registry.example.com:5000")
                .monospace()
                .with_text("docker.io")
        });
        let username = cx.new(|cx| TextInput::new(window, cx).placeholder("username"));
        let secret = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("password or token")
                .masked(true)
        });
        Self {
            server,
            username,
            secret,
        }
    }
}

/// The run dialog's text fields.
pub struct RunInputs {
    pub image: Entity<TextInput>,
    pub name: Entity<TextInput>,
    pub ports: Entity<TextInput>,
    pub env: Entity<TextInput>,
    pub volumes: Entity<TextInput>,
    pub command: Entity<TextInput>,
}

impl RunInputs {
    fn new(window: &mut Window, cx: &mut Context<FerrumContainers>) -> Self {
        let mut field = |placeholder: &'static str, lines: usize| {
            cx.new(|cx| {
                let input = TextInput::new(window, cx)
                    .placeholder(placeholder)
                    .monospace();
                if lines > 1 {
                    input.multiline(lines)
                } else {
                    input
                }
            })
        };
        Self {
            image: field("nginx:latest", 1),
            name: field("optional, e.g. web", 1),
            ports: field("8080:80, 127.0.0.1:5432:5432", 1),
            env: field("KEY=value, one per line", 3),
            volumes: field("~/site:/usr/share/nginx/html:ro, data:/data", 1),
            command: field("optional, overrides the image's CMD", 1),
        }
    }
}

pub struct FerrumContainers {
    pub state: AppState,
    pub status: EngineStatus,
    pub services: Services,
    /// `docker` CLI for compose and "open shell" (None: those actions explain why).
    pub docker_cli: Option<DockerCli>,
    pub inputs: Inputs,
    pub project_bar: Entity<ProjectBar>,
    pub log_scroll: UniformListScrollHandle,
    pub detail_width: f32,
    pub sidebar_open: bool,
    /// The detail card was used last (it is drawn a little lighter).
    pub detail_active: bool,
    /// Clock for stats history timestamps.
    pub started: std::time::Instant,
    pub(crate) connecting: bool,
    pub(crate) polling: bool,
    /// Since when no container has run (for the VM's auto-stop).
    pub(crate) idle_since: Option<std::time::Instant>,
    /// The VM is being started or stopped.
    pub(crate) vm_busy: bool,
    pub(crate) log_task: Option<(Task<()>, tokio::task::AbortHandle)>,
    pub(crate) notice_task: Option<Task<()>>,
    /// FerrumChat beside the cards (see `agent`).
    pub(crate) agent: ferrumagent_ui::AgentPane,
    /// The engine the agent's tools use, set before each message.
    pub(crate) agent_engine: crate::agent::EngineSlot,
    focus_handle: FocusHandle,
    _poll: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl FerrumContainers {
    pub fn new(
        services: Services,
        docker_cli: Option<DockerCli>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Filter")
                .borderless()
        });
        let log_search = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Search logs")
                .borderless()
        });
        let pull = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("e.g. nginx:latest, ghcr.io/org/app:1.2")
                .monospace()
        });
        let tag = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("e.g. myapp:v2, registry.local:5000/team/app:1.0")
                .monospace()
        });
        let run = RunInputs::new(window, cx);
        let registry = RegistryInputs::new(window, cx);
        let files_path = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("/path/in/the/container")
                .monospace()
                .borderless()
        });
        let file_view = cx.new(|cx| {
            TextInput::new(window, cx)
                .monospace()
                .multiline(8)
                .read_only(true)
                .borderless()
        });
        let compose_editor = cx.new(|cx| {
            TextInput::new(window, cx)
                .monospace()
                .multiline(12)
                .borderless()
        });
        let pod_logs = cx.new(|cx| {
            TextInput::new(window, cx)
                .monospace()
                .multiline(20)
                .read_only(true)
                .borderless()
        });
        let context_name = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("name, e.g. build-box")
                .monospace()
        });
        let context_host = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("ssh://me@build-box")
                .monospace()
        });
        let project_bar = cx.new(|cx| ProjectBar::new(APP_BINARY, window, cx).inline(true));
        let subscriptions = vec![
            cx.subscribe(&filter, |app: &mut Self, input, event, cx| {
                if *event == TextInputEvent::Changed {
                    app.state.filter = input.read(cx).text().to_owned();
                    cx.notify();
                }
            }),
            cx.subscribe(&log_search, |app: &mut Self, input, event, cx| {
                if *event == TextInputEvent::Changed {
                    let text = input.read(cx).text().to_owned();
                    app.set_log_search(text, cx);
                }
            }),
            cx.subscribe(&pull, |app: &mut Self, _, event, cx| {
                if *event == TextInputEvent::Submit {
                    app.start_pull(cx);
                }
            }),
            cx.subscribe(&tag, |app: &mut Self, _, event, cx| {
                if *event == TextInputEvent::Submit {
                    app.submit_tag(cx);
                }
            }),
            cx.subscribe(&registry.secret, |app: &mut Self, _, event, cx| {
                if *event == TextInputEvent::Submit {
                    app.sign_in(cx);
                }
            }),
            cx.subscribe(&files_path, |app: &mut Self, _, event, cx| {
                if *event == TextInputEvent::Submit {
                    app.submit_files_path(cx);
                }
            }),
            cx.subscribe(&compose_editor, |app: &mut Self, _, event, cx| {
                if *event == TextInputEvent::Changed {
                    app.compose_text_changed(cx);
                }
            }),
            cx.subscribe(&context_host, |app: &mut Self, _, event, cx| {
                if *event == TextInputEvent::Submit {
                    app.add_ssh_context(cx);
                }
            }),
            cx.subscribe(&run.image, |app: &mut Self, _, event, cx| {
                if *event == TextInputEvent::Submit {
                    app.submit_run(cx);
                }
            }),
            cx.subscribe(&project_bar, |app: &mut Self, _, event, cx| match event {
                ProjectBarEvent::Open(path) => app.open_project(path.clone(), cx),
            }),
        ];
        let mut app = Self {
            state: AppState::default(),
            status: EngineStatus::Connecting,
            services,
            docker_cli,
            inputs: Inputs {
                filter,
                log_search,
                pull,
                run,
                tag,
                registry,
                context_name,
                context_host,
                files_path,
                file_view,
                compose_editor,
                pod_logs,
            },
            project_bar,
            log_scroll: UniformListScrollHandle::new(),
            detail_width: 560.,
            sidebar_open: true,
            detail_active: false,
            started: std::time::Instant::now(),
            connecting: false,
            idle_since: None,
            vm_busy: false,
            polling: false,
            log_task: None,
            notice_task: None,
            agent: ferrumagent_ui::AgentPane::new(),
            agent_engine: crate::agent::EngineSlot::default(),
            focus_handle: cx.focus_handle(),
            _poll: Self::start_polling(cx),
            _subscriptions: subscriptions,
        };
        window.focus(&app.focus_handle, cx);
        app.connect(false, cx);
        app
    }

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_open = !self.sidebar_open;
        cx.notify();
    }

    pub fn set_detail_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.detail_active != active {
            self.detail_active = active;
            cx.notify();
        }
    }

    /// The container detail card is drawn, right of the list (see
    /// `views::section`).
    pub fn detail_open(&self) -> bool {
        matches!(self.status, EngineStatus::Connected(_))
            && self.state.section == crate::state::Section::Containers
            && self.state.detail.is_some()
    }

    /// The sidebar is drawn (open, and an engine is connected).
    pub fn sidebar_shown(&self) -> bool {
        self.sidebar_open && matches!(self.status, EngineStatus::Connected(_))
    }

    fn on_refresh(&mut self, _: &Refresh, _: &mut Window, cx: &mut Context<Self>) {
        match self.status {
            EngineStatus::Connected(_) => self.refresh(self.state.section, cx),
            EngineStatus::Offline(_) => self.connect(false, cx),
            EngineStatus::Connecting => {}
        }
    }

    fn on_focus_filter(&mut self, _: &FocusFilter, window: &mut Window, cx: &mut Context<Self>) {
        let filter = self.inputs.filter.clone();
        filter.update(cx, |input, cx| input.focus(window, cx));
    }

    fn on_cancel(
        &mut self,
        _: &ferrum_ui::menu::Cancel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.dialog.take().is_some() {
            window.focus(&self.focus_handle, cx);
        } else if self.state.detail.is_some() {
            self.close_detail(cx);
        }
        cx.notify();
    }
}

impl Render for FerrumContainers {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let text = cx.theme().colors().text;
        let body = views::body(self, window, cx);
        let dialog = views::dialogs::render(self, window, cx);
        let notice = views::chrome::notice(self, cx);
        let sidebar = self
            .sidebar_shown()
            .then(|| views::chrome::sidebar(self, window, cx));
        // The cards start at the top: each carries its own header row.
        // The Agent pane, when open, is a card right of the others.
        let main = h_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .py(ferrum_ui::PANE_GAP)
            .pr(ferrum_ui::PANE_GAP)
            .when(sidebar.is_none(), |d| d.pl(ferrum_ui::PANE_GAP))
            .gap(ferrum_ui::PANE_GAP)
            .child(v_flex().flex_1().min_w_0().h_full().child(body))
            .children(self.agent.card(cx));
        // Backdrop (drawn by the root) shows through: no background here.
        h_flex()
            .id("ferrumdock")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_refresh))
            .on_action(cx.listener(Self::on_focus_filter))
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(|app, _: &ToggleSidebar, _, cx| app.toggle_sidebar(cx)))
            .on_action(cx.listener(|app, _: &ToggleProjectSwitcher, window, cx| {
                app.project_bar
                    .update(cx, |bar, cx| bar.toggle_switcher(window, cx));
            }))
            .relative()
            .size_full()
            .items_stretch()
            .text_color(text)
            .text_sm()
            .children(sidebar)
            .child(main)
            .children(dialog)
            .children(notice)
    }
}

impl gpui::Focusable for FerrumContainers {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
