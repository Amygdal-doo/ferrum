//! The window's state: which system is shown (the project's
//! `.architecture/`, a demo or nothing), the selection, the open tab and
//! dialog, zoom and pan, the flow being walked through, and live pod
//! status. The demo opens by itself only at the first start (see
//! [`crate::prefs`]); after that it is in File › Open Demo and Help.

use std::path::{Path, PathBuf};

use ferrum_ui::gpui::{KeyBinding, Point};
use ferrum_ui::prelude::*;
use ferrumsystem_core::history::History;
use ferrumsystem_core::runtime::{Container, Pod};
use ferrumsystem_core::{EdgeStyle, Provider, Side, System, demos, store};

use crate::prefs::{self, FirstView, UiPrefs};

actions!(
    ferrum_system,
    [
        ZoomIn,
        ZoomOut,
        ZoomFit,
        Escape,
        DeleteSelected,
        /// Undo the last edit of the diagram.
        Undo,
        /// Redo the edit last undone.
        Redo,
        /// Show or hide the left sidebar.
        ToggleSidebar,
        /// Show or hide the right panel (overview, APIs, data, …).
        TogglePanel,
        /// Choose a folder (or use the open project) and start its diagram.
        NewDiagram,
        /// Open a project folder.
        OpenProject,
        /// Generate the diagram from the project's docker-compose files,
        /// Kubernetes manifests or package manifests.
        GenerateFromProject
    ]
);

/// Export the diagram shown as a file of this format.
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(namespace = ferrum_system, no_json)]
pub struct ExportAs {
    pub format: crate::export::Format,
}

/// Show demo `index` (of [`demos::all`]).
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(namespace = ferrum_system, no_json)]
pub struct OpenDemo {
    pub index: usize,
}

pub const KEY_CONTEXT: &str = "FerrumSystem";

pub fn bind_keys(cx: &mut App) {
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("cmd-=", ZoomIn, context),
        KeyBinding::new("cmd--", ZoomOut, context),
        KeyBinding::new("cmd-0", ZoomFit, context),
        KeyBinding::new("escape", Escape, context),
        KeyBinding::new("backspace", DeleteSelected, context),
        KeyBinding::new("cmd-z", Undo, context),
        KeyBinding::new("cmd-shift-z", Redo, context),
        KeyBinding::new("cmd-b", ToggleSidebar, context),
        KeyBinding::new("alt-cmd-b", TogglePanel, context),
        KeyBinding::new("cmd-n", NewDiagram, context),
        KeyBinding::new("cmd-o", OpenProject, context),
    ]);
}

/// What the diagram shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// The project's `.architecture/system.json`.
    Project(PathBuf),
    Demo(usize),
    /// Nothing: no project diagram and the demo already seen.
    Empty,
}

pub struct Options {
    /// The project asked for (`ferrumsystem <folder>`); else the last one.
    pub project: Option<PathBuf>,
    /// `~/.ferrum/ferrumsystem/ui.json` (`None`: nothing saved).
    pub prefs_file: Option<PathBuf>,
    /// `~/.ferrum/projects.json` (`None`: not recorded).
    pub projects_file: Option<PathBuf>,
    /// Where the Agent pane's chat keeps its settings and threads, with
    /// keys in memory (tests); `None`: `~/.ferrum/agent` and the keychain.
    pub agent_home: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            project: None,
            prefs_file: prefs::default_file(),
            projects_file: ferrum_ui::project::projects_file(),
            agent_home: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Add,
    Overview,
    Component,
    Apis,
    Sockets,
    Data,
    Flows,
    Reliability,
    Deploy,
    Runtime,
}

impl Tab {
    pub const ALL: [Tab; 10] = [
        Tab::Add,
        Tab::Overview,
        Tab::Component,
        Tab::Apis,
        Tab::Sockets,
        Tab::Data,
        Tab::Flows,
        Tab::Reliability,
        Tab::Deploy,
        Tab::Runtime,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Add => "Add",
            Tab::Overview => "Overview",
            Tab::Component => "Component",
            Tab::Apis => "APIs",
            Tab::Sockets => "Sockets",
            Tab::Data => "Data",
            Tab::Flows => "Flows",
            Tab::Reliability => "Reliability",
            Tab::Deploy => "Deploy",
            Tab::Runtime => "Runtime",
        }
    }
}

pub enum Dialog {
    /// A database's tables.
    Schema(String),
    /// A new layer: its name and colour.
    Layer(Entity<TextInput>, u32),
    /// Name a custom technology before choosing its icon.
    CustomTech(Entity<TextInput>),
    /// Export the diagram: PNG, SVG or PDF.
    Export,
}

/// Live status for the Runtime tab.
#[derive(Default)]
pub struct Live {
    pub loading: bool,
    pub pods: Option<Result<Vec<Pod>, String>>,
    pub containers: Option<Result<Vec<Container>, String>>,
}

/// A state to undo to: the system and, for a project, its `system.json`
/// text then (put back as it was when nothing else changed the file).
pub(crate) type Snapshot = (System, Option<String>);

pub struct FerrumSystem {
    pub(crate) project_bar: Entity<ProjectBar>,
    pub(crate) project: Option<PathBuf>,
    /// Whether the project has an `.architecture/` folder.
    pub(crate) project_has_system: bool,
    pub(crate) source: Source,
    pub(crate) system: System,
    /// The edits of the system shown, to undo (cleared when another is shown).
    pub(crate) history: History<Snapshot>,
    pub(crate) demos: Vec<System>,
    pub(crate) prefs: UiPrefs,
    prefs_file: Option<PathBuf>,
    pub(crate) selected: Option<String>,
    pub(crate) tab: Tab,
    pub(crate) dialog: Option<Dialog>,
    /// Connect mode: the first component clicked.
    pub(crate) connecting: Option<Option<String>>,
    pub(crate) edge_style: EdgeStyle,
    pub(crate) zoom: f32,
    pub(crate) pan: Point<f32>,
    pub(crate) drag_from: Option<Point<f32>>,
    /// Canvas bounds in the window at the last paint: x, y, width, height.
    pub(crate) canvas_bounds: std::rc::Rc<std::cell::Cell<(f32, f32, f32, f32)>>,
    /// Routes drawn at the last render, unscaled (for right-click hits).
    pub(crate) routes: Vec<(usize, Vec<(f32, f32)>)>,
    /// Pointer while something is dragged, in window coordinates.
    pub(crate) drag_pos: Option<Point<f32>>,
    /// The handle a connection is being drawn from.
    pub(crate) drawing: Option<crate::canvas::DraggedHandleState>,
    /// Side the connection being made leaves from.
    pub(crate) connect_side: Option<Side>,
    /// Right-click menu of connection `.0`, at `.1` (canvas coordinates).
    pub(crate) edge_menu: Option<(usize, Point<f32>)>,
    pub(crate) show_toolbar: bool,
    /// The Add tab's category and target layer.
    pub(crate) palette: (ferrumsystem_core::Category, String),
    /// Flow walkthrough: (flow, step).
    pub(crate) walk: Option<(usize, usize)>,
    /// Fit the diagram at the next render (a system was just shown).
    pub(crate) fit_pending: bool,
    /// The Component tab's text fields, for the selected component.
    pub(crate) fields: Option<crate::component::NodeFields>,
    pub(crate) live: Live,
    pub(crate) notice: Option<Notice>,
    /// FerrumChat beside the diagram (see `agent`).
    pub(crate) agent: ferrumagent_ui::AgentPane,
    /// The diagram the agent's tools read, set before each message.
    pub(crate) agent_system: crate::agent_tools::SystemSlot,
    pub(crate) agent_home: Option<PathBuf>,
    pub(crate) focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl FerrumSystem {
    pub fn new(options: Options, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let projects_file = options.projects_file;
        let project_bar = cx.new(|cx| {
            ProjectBar::new("ferrumsystem", window, cx)
                .inline(true)
                .projects_file(projects_file)
        });
        let prefs = prefs::load(options.prefs_file.as_deref());
        let project = prefs::start_project(options.project, &prefs);
        let subscriptions = vec![
            cx.subscribe(&project_bar, |this: &mut Self, bar, event, cx| {
                let ProjectBarEvent::Open(path) = event;
                let path = path.clone();
                bar.update(cx, |bar, cx| bar.set_project(Some(path.clone()), cx));
                this.open_project(Some(path), cx);
            }),
        ];
        let mut this = Self {
            project_bar,
            project: None,
            project_has_system: false,
            source: Source::Empty,
            system: System::default(),
            history: History::default(),
            demos: demos::all(),
            prefs,
            prefs_file: options.prefs_file,
            selected: None,
            tab: Tab::Overview,
            dialog: None,
            connecting: None,
            edge_style: EdgeStyle::Request,
            zoom: 0.9,
            pan: Point { x: 0., y: 0. },
            drag_from: None,
            canvas_bounds: Default::default(),
            routes: Vec::new(),
            drag_pos: None,
            drawing: None,
            connect_side: None,
            edge_menu: None,
            show_toolbar: true,
            palette: (ferrumsystem_core::Category::Backend, "services".into()),
            walk: None,
            fit_pending: true,
            fields: None,
            live: Live::default(),
            notice: None,
            agent: ferrumagent_ui::AgentPane::new(),
            agent_system: crate::agent_tools::SystemSlot::default(),
            agent_home: options.agent_home,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        };
        if let Some(path) = project.clone() {
            this.project_bar
                .update(cx, |bar, cx| bar.set_project(Some(path), cx));
        }
        this.open_project(project, cx);
        match prefs::first_view(&this.prefs, this.project_has_system) {
            FirstView::Demo => this.show_demo(0, cx),
            FirstView::Project | FirstView::Empty => {}
        }
        window.focus(&this.focus_handle, cx);
        this
    }

    pub fn sidebar_open(&self) -> bool {
        self.prefs.sidebar_open
    }

    pub fn panel_open(&self) -> bool {
        self.prefs.panel_open
    }

    pub(crate) fn save_prefs(&mut self, cx: &mut Context<Self>) {
        if let Err(e) = prefs::save(self.prefs_file.as_deref(), &self.prefs) {
            self.say(Notice::error(format!("Could not save settings: {e}")), cx);
        }
    }

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.prefs.sidebar_open = !self.prefs.sidebar_open;
        self.save_prefs(cx);
        cx.notify();
    }

    pub fn set_panel_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.prefs.panel_open != open {
            self.prefs.panel_open = open;
            self.save_prefs(cx);
        }
        cx.notify();
    }

    pub fn toggle_panel(&mut self, cx: &mut Context<Self>) {
        let open = !self.prefs.panel_open;
        self.set_panel_open(open, cx);
    }

    /// Switches to `project`: its system if it has one, else nothing (the
    /// empty view offers to create one).
    pub fn open_project(&mut self, project: Option<PathBuf>, cx: &mut Context<Self>) {
        self.project = project.clone();
        self.project_has_system = false;
        if let Some(path) = &project
            && self.prefs.last_project.as_ref() != Some(path)
        {
            self.prefs.last_project = Some(path.clone());
            self.save_prefs(cx);
        }
        if let Some(path) = project {
            match store::load(&path) {
                Ok(Some(system)) => {
                    self.project_has_system = true;
                    self.show(Source::Project(path), system, cx);
                    return;
                }
                Ok(None) => {}
                Err(e) => self.notice = Some(Notice::error(e.to_string())),
            }
        }
        if !self.is_demo() {
            self.show(Source::Empty, System::default(), cx);
        }
        cx.notify();
    }

    /// Shows demo `index` and remembers that the demo has been seen.
    pub fn show_demo(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(system) = self.demos.get(index).cloned() {
            self.show(Source::Demo(index), system, cx);
            if !self.prefs.demo_seen {
                self.prefs.demo_seen = true;
                self.save_prefs(cx);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.source == Source::Empty
    }

    /// The open project's diagram (created if it has none), or a folder to
    /// start one in.
    pub fn new_diagram(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.project.is_some() {
            if self.project_has_system {
                self.show_project_system(cx);
            } else {
                self.create_architecture(None, cx);
            }
            return;
        }
        let receiver = cx.prompt_for_paths(ferrum_ui::gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Start diagram here".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update(cx, |this, cx| {
                this.project_bar
                    .update(cx, |bar, cx| bar.set_project(Some(path.clone()), cx));
                this.open_project(Some(path), cx);
                if !this.project_has_system {
                    this.create_architecture(None, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Asks for a project folder (the switcher's "Open folder…").
    pub fn open_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.project_bar
            .update(cx, |bar, cx| bar.open_folder_dialog(window, cx));
    }

    pub fn show_project_system(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.project.clone() else {
            return;
        };
        match store::load(&path) {
            Ok(Some(system)) => self.show(Source::Project(path), system, cx),
            Ok(None) => {}
            Err(e) => self.say(Notice::error(e.to_string()), cx),
        }
    }

    fn show(&mut self, source: Source, system: System, cx: &mut Context<Self>) {
        self.source = source;
        self.system = system;
        self.history.clear();
        self.selected = None;
        self.walk = None;
        self.connecting = None;
        self.connect_side = None;
        self.edge_menu = None;
        self.fields = None;
        self.live = Live::default();
        if self.tab == Tab::Component {
            self.tab = Tab::Overview;
        }
        self.fit_pending = true;
        cx.notify();
    }

    /// Creates the project's `.architecture/` from `template` (a demo) or
    /// the starter system.
    pub fn create_architecture(&mut self, template: Option<System>, cx: &mut Context<Self>) {
        let Some(path) = self.project.clone() else {
            self.say(Notice::info("Choose a project folder first."), cx);
            return;
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "System".into());
        let system = template.unwrap_or_else(|| demos::starter(&name));
        match store::save(&path, &system) {
            Ok(()) => {
                self.project_has_system = true;
                self.show(Source::Project(path.clone()), system, cx);
                self.say(
                    Notice::info(format!("Created {}", store::system_file(&path).display())),
                    cx,
                );
            }
            Err(e) => self.say(Notice::error(e.to_string()), cx),
        }
    }

    /// Applies an edit, saves it and lets it be undone (demos change only
    /// in memory; with nothing shown there is nothing to edit). An edit
    /// that changes nothing writes nothing.
    pub fn edit(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut System)) {
        self.edit_grouped(None, cx, f);
    }

    /// [`Self::edit`], where edits of one `group` in a row (typing into a
    /// field) undo as one.
    pub fn edit_grouped(
        &mut self,
        group: Option<&str>,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut System),
    ) {
        if self.is_empty() {
            return;
        }
        let before = self.system.clone();
        f(&mut self.system);
        if self.system == before {
            return;
        }
        let text = self.file_text();
        self.history.record((before, text), group);
        self.persist(None, cx);
    }

    /// The project's `system.json` as it is now (`None` for a demo).
    fn file_text(&self) -> Option<String> {
        match &self.source {
            Source::Project(path) => store::read_text(path),
            Source::Demo(_) | Source::Empty => None,
        }
    }

    /// Undoes the last edit (`redo` false) or redoes the one undone last.
    pub fn undo(&mut self, redo: bool, cx: &mut Context<Self>) {
        let current = (self.system.clone(), self.file_text());
        let restored = if redo {
            self.history.redo(current)
        } else {
            self.history.undo(current)
        };
        let Some((system, text)) = restored else {
            return;
        };
        self.system = system;
        // The Component tab's fields show the restored text.
        self.fields = None;
        if let Some(id) = &self.selected
            && self.system.node(id).is_none()
        {
            self.select(None, cx);
        }
        self.edge_menu = None;
        self.persist(text, cx);
    }

    /// Saves the system shown: into the project's `system.json` (keeping
    /// its formatting; `text`, the file as it was, when it says just that),
    /// or into the demo in memory.
    fn persist(&mut self, text: Option<String>, cx: &mut Context<Self>) {
        match &self.source {
            Source::Project(path) => {
                let same =
                    |t: &String| serde_json::from_str::<System>(t).is_ok_and(|s| s == self.system);
                let result = match text.filter(same) {
                    Some(text) => store::restore_text(path, &text),
                    None => store::save_edit(path, &self.system),
                };
                if let Err(e) = result {
                    self.say(Notice::error(e.to_string()), cx);
                }
            }
            Source::Demo(i) => {
                if let Some(demo) = self.demos.get_mut(*i) {
                    *demo = self.system.clone();
                }
            }
            Source::Empty => {}
        }
        cx.notify();
    }

    pub fn is_demo(&self) -> bool {
        matches!(self.source, Source::Demo(_))
    }

    /// The folder custom icons come from.
    pub fn icon_root(&self) -> Option<&Path> {
        match &self.source {
            Source::Project(path) => Some(path),
            Source::Demo(_) | Source::Empty => None,
        }
    }

    pub fn set_provider(&mut self, provider: Provider, cx: &mut Context<Self>) {
        self.edit(cx, |s| s.provider = provider);
    }

    pub fn select(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        self.history.seal();
        self.selected = id;
        self.fields = None;
        if self.selected.is_some() {
            self.tab = Tab::Component;
        } else if self.tab == Tab::Component {
            self.tab = Tab::Overview;
        }
        cx.notify();
    }

    /// A click on a component: connect, open its schema (double-click on a
    /// database) or select it.
    pub fn click_node(&mut self, id: &str, clicks: usize, cx: &mut Context<Self>) {
        if let Some(first) = self.connecting.clone() {
            match first {
                None => self.connecting = Some(Some(id.to_owned())),
                Some(from) => {
                    let style = self.edge_style;
                    match self.connect_side.take() {
                        // Started from a side handle: one connection, then done.
                        Some(side) => {
                            self.edit(cx, |s| s.connect(&from, id, (Some(side), None), style));
                            self.connecting = None;
                        }
                        None => {
                            self.edit(cx, |s| s.toggle_edge(&from, id, style));
                            self.connecting = Some(None);
                        }
                    }
                }
            }
            cx.notify();
            return;
        }
        let database = self.system.node(id).and_then(|n| n.database.clone());
        if clicks >= 2
            && let Some(db) = database
        {
            self.dialog = Some(Dialog::Schema(db));
            cx.notify();
            return;
        }
        self.select(Some(id.to_owned()), cx);
    }

    /// Copies component `id` next to itself (without its connections).
    pub fn duplicate(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(mut node) = self.system.node(id).cloned() else {
            return;
        };
        node.id = self.system.new_node_id(&node.name);
        node.name = format!("{} copy", node.name);
        let new_id = node.id.clone();
        let layer = node.layer.clone();
        let index = self
            .system
            .nodes_in(&layer)
            .iter()
            .position(|n| n.id == id)
            .map_or(0, |i| i + 1);
        self.edit(cx, |s| {
            s.nodes.push(node);
            s.place(&new_id, &layer, index);
        });
        self.select(Some(new_id), cx);
    }

    pub fn delete_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected.take() {
            self.edit(cx, |s| s.remove_node(&id));
            self.select(None, cx);
        }
    }

    pub fn zoom_by(&mut self, factor: f32, cx: &mut Context<Self>) {
        self.zoom = (self.zoom * factor).clamp(0.3, 2.5);
        cx.notify();
    }

    /// Fits the whole diagram in the canvas.
    pub fn fit(&mut self, cx: &mut Context<Self>) {
        let (_, _, w, h) = self.canvas_bounds.get();
        self.fit_to(w, h);
        cx.notify();
    }

    /// Zoom and pan so the whole diagram fits `w` × `h`.
    pub(crate) fn fit_to(&mut self, w: f32, h: f32) {
        if w <= 0. || h <= 0. {
            return;
        }
        let layout = crate::layout::compute(&self.system);
        self.zoom = (w / layout.width).min(h / layout.height).clamp(0.3, 1.4);
        self.pan = Point {
            x: (w - layout.width * self.zoom) / 2.,
            y: ((h - layout.height * self.zoom) / 2.).clamp(0., 12.),
        };
    }

    /// Flow walkthrough: start flow `i`, or step within it.
    pub fn walk_to(&mut self, walk: Option<(usize, usize)>, cx: &mut Context<Self>) {
        self.walk = walk.filter(|(f, s)| {
            self.system
                .flows
                .get(*f)
                .is_some_and(|flow| *s < flow.steps.len())
        });
        cx.notify();
    }

    /// The (from, to) of the walkthrough's current step.
    pub fn walk_step(&self) -> Option<(&str, &str)> {
        let (f, s) = self.walk?;
        let step = self.system.flows.get(f)?.steps.get(s)?;
        Some((step.from.as_str(), step.to.as_str()))
    }

    pub fn say(&mut self, notice: Notice, cx: &mut Context<Self>) {
        self.notice = Some(notice);
        cx.notify();
    }

    /// Reads pods (`kubectl`) and containers (`docker`) in the background.
    pub fn refresh_live(&mut self, cx: &mut Context<Self>) {
        if self.live.loading {
            return;
        }
        self.live.loading = true;
        let cluster = self.system.cluster.clone().unwrap_or_default();
        let kubectl = ferrum_ui::integrations::find_executable("kubectl");
        let docker = ferrum_ui::integrations::find_executable("docker");
        let task = cx.background_executor().spawn(async move {
            let pods = match kubectl {
                Some(exe) => ferrumsystem_core::runtime::pods(&exe, &cluster),
                None => Err("kubectl is not installed".into()),
            };
            let containers = match docker {
                Some(exe) => ferrumsystem_core::runtime::containers(&exe),
                None => Err("docker is not installed".into()),
            };
            (pods, containers)
        });
        cx.spawn(async move |this, cx| {
            let (pods, containers) = task.await;
            this.update(cx, |this, cx| {
                this.live = Live {
                    loading: false,
                    pods: Some(pods),
                    containers: Some(containers),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Asks for an image and stores it as technology `tech`'s icon.
    pub fn choose_icon(&mut self, tech: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.icon_root().map(Path::to_path_buf) else {
            self.say(
                Notice::info(
                    "Icons are saved in the project's .architecture folder: open a project first.",
                ),
                cx,
            );
            return;
        };
        let receiver = cx.prompt_for_paths(ferrum_ui::gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Use as icon".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let result = store::add_icon(&root, &tech, &path);
            this.update(cx, |this, cx| match result {
                Ok(_) => this.say(Notice::info("Icon added"), cx),
                Err(e) => this.say(Notice::error(e.to_string()), cx),
            })
            .ok();
        })
        .detach();
    }
}

/// A component being dragged (to another place or layer).
#[derive(Clone)]
pub struct DraggedNode(pub String);
/// A layer band being dragged by its label.
#[derive(Clone)]
pub struct DraggedLayer(pub usize);
/// A connection being drawn from a component's side handle.
#[derive(Clone)]
pub struct DraggedHandle(pub String, pub Side);

/// What follows the pointer while dragging.
pub struct DragLabel(pub SharedString);

impl Render for DragLabel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(colors.text_accent)
            .bg(colors.elevated_surface_background)
            .text_size(px(12.))
            .child(self.0.clone())
    }
}

impl FerrumSystem {
    /// A window point in diagram units.
    pub fn to_world(&self, p: Point<f32>) -> (f32, f32) {
        let (ox, oy, _, _) = self.canvas_bounds.get();
        (
            (p.x - ox - self.pan.x) / self.zoom,
            (p.y - oy - self.pan.y) / self.zoom,
        )
    }

    /// Drops component `id` into `layer` where the pointer is.
    pub fn drop_node(&mut self, id: &str, layer: &str, cx: &mut Context<Self>) {
        let (x, _) = self.drag_pos.map(|p| self.to_world(p)).unwrap_or((0., 0.));
        self.drag_pos = None;
        let l = crate::layout::compute(&self.system);
        let band_x = l.bands.first().map(|(_, r)| r.x).unwrap_or(0.);
        let step = crate::layout::NODE_W + crate::layout::GAP_X;
        let index = ((x - band_x - crate::layout::BAND_PAD) / step)
            .round()
            .max(0.) as usize;
        let (id, layer) = (id.to_owned(), layer.to_owned());
        self.edit(cx, |s| s.place(&id, &layer, index));
    }

    pub fn drop_layer(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        self.drag_pos = None;
        self.edit(cx, |s| s.move_layer(from, to));
    }

    /// A connection drawn from `from`'s `side` and dropped on `to`.
    pub fn drop_handle(&mut self, from: &str, side: Side, to: &str, cx: &mut Context<Self>) {
        let at = self.drag_pos.take().map(|p| self.to_world(p));
        let layout = crate::layout::compute(&self.system);
        let to_side = at.and_then(|(x, y)| layout.nodes.get(to).map(|r| r.nearest_side(x, y)));
        let style = self.edge_style;
        let (from, to) = (from.to_owned(), to.to_owned());
        self.edit(cx, |s| s.connect(&from, &to, (Some(side), to_side), style));
    }

    /// The connection under a window point, if any.
    pub fn edge_at(&self, p: Point<f32>) -> Option<usize> {
        let (x, y) = self.to_world(p);
        let tolerance = 6. / self.zoom;
        self.routes
            .iter()
            .map(|(i, r)| (*i, crate::layout::distance(r, x, y)))
            .filter(|(_, d)| *d <= tolerance)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    pub fn edit_edge(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut ferrumsystem_core::Edge),
    ) {
        self.edit(cx, |s| {
            if let Some(e) = s.edges.get_mut(index) {
                f(e);
            }
        });
    }
}
