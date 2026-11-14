//! The window's state: which system is shown (the project's
//! `.architecture/` or a demo), the selection, the open tab and dialog,
//! zoom and pan, the flow being walked through, and live pod status.

use std::path::{Path, PathBuf};

use ferrum_ui::gpui::{KeyBinding, Point};
use ferrum_ui::prelude::*;
use ferrumsystem_core::runtime::{Container, Pod};
use ferrumsystem_core::{EdgeStyle, Provider, Side, System, demos, store};

actions!(
    ferrum_system,
    [ZoomIn, ZoomOut, ZoomFit, Escape, DeleteSelected]
);

pub const KEY_CONTEXT: &str = "FerrumSystem";

pub fn bind_keys(cx: &mut App) {
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("cmd-=", ZoomIn, context),
        KeyBinding::new("cmd--", ZoomOut, context),
        KeyBinding::new("cmd-0", ZoomFit, context),
        KeyBinding::new("escape", Escape, context),
        KeyBinding::new("backspace", DeleteSelected, context),
    ]);
}

/// What the diagram shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// The project's `.architecture/system.json`.
    Project(PathBuf),
    Demo(usize),
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
}

/// Live status for the Runtime tab.
#[derive(Default)]
pub struct Live {
    pub loading: bool,
    pub pods: Option<Result<Vec<Pod>, String>>,
    pub containers: Option<Result<Vec<Container>, String>>,
}

pub struct FerrumSystem {
    pub(crate) project_bar: Entity<ProjectBar>,
    pub(crate) project: Option<PathBuf>,
    /// Whether the project has an `.architecture/` folder.
    pub(crate) project_has_system: bool,
    pub(crate) source: Source,
    pub(crate) system: System,
    pub(crate) demos: Vec<System>,
    pub(crate) sidebar_open: bool,
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
    pub(crate) show_panel: bool,
    pub(crate) show_toolbar: bool,
    pub(crate) demos_open: bool,
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
    pub(crate) focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl FerrumSystem {
    pub fn new(project: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let project_bar = cx.new(|cx| ProjectBar::new("ferrumsystem", window, cx).inline(true));
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
            source: Source::Demo(0),
            system: demos::banking(),
            demos: demos::all(),
            sidebar_open: true,
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
            show_panel: true,
            show_toolbar: true,
            demos_open: true,
            palette: (ferrumsystem_core::Category::Backend, "services".into()),
            walk: None,
            fit_pending: true,
            fields: None,
            live: Live::default(),
            notice: None,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        };
        if let Some(path) = project.clone() {
            this.project_bar
                .update(cx, |bar, cx| bar.set_project(Some(path), cx));
        }
        this.open_project(project, cx);
        window.focus(&this.focus_handle, cx);
        this
    }

    /// Switches to `project` (its system if it has one, else the first demo).
    pub fn open_project(&mut self, project: Option<PathBuf>, cx: &mut Context<Self>) {
        self.project = project.clone();
        self.project_has_system = false;
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
        if !matches!(self.source, Source::Demo(_)) {
            self.show_demo(0, cx);
        }
        cx.notify();
    }

    pub fn show_demo(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(system) = self.demos.get(index).cloned() {
            self.show(Source::Demo(index), system, cx);
        }
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

    /// Applies an edit and saves it (demos change only in memory).
    pub fn edit(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut System)) {
        f(&mut self.system);
        match &self.source {
            Source::Project(path) => {
                if let Err(e) = store::save(path, &self.system) {
                    self.say(Notice::error(e.to_string()), cx);
                }
            }
            Source::Demo(i) => {
                if let Some(demo) = self.demos.get_mut(*i) {
                    *demo = self.system.clone();
                }
            }
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
            Source::Demo(_) => None,
        }
    }

    pub fn set_provider(&mut self, provider: Provider, cx: &mut Context<Self>) {
        self.edit(cx, |s| s.provider = provider);
    }

    pub fn select(&mut self, id: Option<String>, cx: &mut Context<Self>) {
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
