//! The project strip at the top of every Ferrum app window:
//!
//! `ferrum ▾   ~ / Projects / ferrum   ⎇ main          Editor  Git  Term`
//!
//! The name opens the switcher (recent projects from every Ferrum app, plus
//! "Open Folder…"); path segments reveal that folder in Finder, right-click
//! copies the path; the buttons open the project in another Ferrum app.
//! Embed it with [`ProjectBar::new`], keep it current with
//! [`ProjectBar::set_project`], and handle [`ProjectBarEvent::Open`].

use std::path::{Path, PathBuf};

use crate::components::{
    Button, ButtonCommon, ButtonSize, ButtonStyle, Clickable, Color, FixedWidth, Icon, IconButton,
    IconName, IconSize, Label, LabelCommon, LabelSize, StyledTypography, Tooltip, h_flex, v_flex,
};
use crate::theme::ActiveTheme;
use gpui::{
    Anchor, AnyElement, App, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, KeyBinding, KeyDownEvent, MouseButton, ParentElement, PathPromptOptions, Render,
    SharedString, Subscription, Window, actions, anchored, deferred, div, point, prelude::*, px,
};

use crate::project::{self, FerrumApp, RecentProject};
use crate::{TextInput, TextInputEvent};

actions!(ferrum, [ToggleProjectSwitcher]);

/// ⌥⌘O opens the switcher in any app that shows a [`ProjectBar`].
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("cmd-alt-o", ToggleProjectSwitcher, None)]);
}

/// What the user asked the app to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectBarEvent {
    /// Switch to this folder (picked from the list or the folder dialog).
    Open(PathBuf),
}

pub struct ProjectBar {
    /// This app's binary name, recorded in `projects.json`.
    app_binary: &'static str,
    project: Option<PathBuf>,
    /// Shown instead of the path when set, e.g. a database connection.
    detail: Option<SharedString>,
    /// Shown as the name when there is no project folder.
    title: Option<SharedString>,
    branch: Option<String>,
    switchable: bool,
    /// Drawn inside a sidebar or title row: name and branch only, no path,
    /// icon-only app buttons, no background.
    inline: bool,
    recent: Vec<RecentProject>,
    switcher_open: bool,
    query: Entity<TextInput>,
    selected: usize,
    message: Option<(SharedString, bool)>,
    focus_handle: FocusHandle,
    /// What had focus before the switcher opened (e.g. a terminal), given
    /// back when it closes.
    restore_focus: Option<FocusHandle>,
    home: Option<PathBuf>,
    /// `~/.ferrum/projects.json`, or a test's own file.
    projects_file: Option<PathBuf>,
    _query_events: Subscription,
}

impl EventEmitter<ProjectBarEvent> for ProjectBar {}

impl Focusable for ProjectBar {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ProjectBar {
    /// `app_binary` is this app's executable name (e.g. `ferrumgit`).
    pub fn new(app_binary: &'static str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| TextInput::new(window, cx).placeholder("Search recent projects"));
        let query_events =
            cx.subscribe_in(&query, window, |this, _, event, window, cx| match event {
                TextInputEvent::Changed => {
                    this.selected = 0;
                    cx.notify();
                }
                TextInputEvent::Submit => this.confirm(window, cx),
                TextInputEvent::Blurred => {}
            });
        Self {
            app_binary,
            project: None,
            detail: None,
            title: None,
            branch: None,
            switchable: true,
            inline: false,
            recent: project::projects_file()
                .map(|file| project::load(&file))
                .unwrap_or_default(),
            switcher_open: false,
            query,
            selected: 0,
            message: None,
            focus_handle: cx.focus_handle(),
            restore_focus: None,
            home: project::home_dir(),
            projects_file: project::projects_file(),
            _query_events: query_events,
        }
    }

    /// Keeps the recent-projects list in `file` instead of
    /// `~/.ferrum/projects.json` (`None`: don't record). For tests.
    #[must_use]
    pub fn projects_file(mut self, file: Option<PathBuf>) -> Self {
        self.recent = file.as_deref().map(project::load).unwrap_or_default();
        self.projects_file = file;
        self
    }

    /// Hides the switcher (for apps whose project isn't a folder).
    #[must_use]
    pub fn switchable(mut self, switchable: bool) -> Self {
        self.switchable = switchable;
        self
    }

    /// Draws the bar inline (in a sidebar or title row): the project name
    /// and branch, icon-only buttons for the other apps, no path.
    #[must_use]
    pub fn inline(mut self, inline: bool) -> Self {
        self.inline = inline;
        self
    }

    /// The folder now open, recorded as the most recent project.
    pub fn set_project(&mut self, path: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.project == path {
            return;
        }
        self.branch = path.as_deref().and_then(project::git_branch);
        if let (Some(path), Some(file)) = (&path, self.projects_file.clone()) {
            match project::record(&file, path, self.app_binary) {
                Ok(list) => self.recent = list,
                Err(e) => tracing_warn(&format!("could not save recent projects: {e}")),
            }
        }
        self.project = path;
        cx.notify();
    }

    /// Text shown after the name instead of the path (e.g. `postgres · app_db`).
    pub fn set_detail(&mut self, detail: Option<SharedString>, cx: &mut Context<Self>) {
        if self.detail != detail {
            self.detail = detail;
            cx.notify();
        }
    }

    /// The name shown when no project folder is set (e.g. the active
    /// collection), instead of "No project".
    pub fn set_title(&mut self, title: Option<SharedString>, cx: &mut Context<Self>) {
        if self.title != title {
            self.title = title;
            cx.notify();
        }
    }

    #[must_use]
    pub fn detail(&self) -> Option<&SharedString> {
        self.detail.as_ref()
    }

    #[must_use]
    pub fn title(&self) -> Option<&SharedString> {
        self.title.as_ref()
    }

    /// Re-reads the branch, e.g. after a checkout.
    pub fn refresh_branch(&mut self, cx: &mut Context<Self>) {
        let branch = self.project.as_deref().and_then(project::git_branch);
        if branch != self.branch {
            self.branch = branch;
            cx.notify();
        }
    }

    #[must_use]
    pub fn project(&self) -> Option<&Path> {
        self.project.as_deref()
    }

    /// Opens or closes the switcher.
    pub fn toggle_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.switchable {
            return;
        }
        if self.switcher_open {
            self.close_switcher(window, cx);
            return;
        }
        self.recent = self
            .projects_file
            .clone()
            .map(|file| project::load(&file))
            .unwrap_or_default();
        self.restore_focus = window.focused(cx);
        self.switcher_open = true;
        self.selected = 0;
        self.query.update(cx, |q, cx| q.set_text("", cx));
        let query = self.query.clone();
        let handle = query.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
        cx.notify();
    }

    fn close_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.switcher_open = false;
        let restore = self
            .restore_focus
            .take()
            .unwrap_or_else(|| self.focus_handle.clone());
        window.focus(&restore, cx);
        cx.notify();
    }

    /// Recent projects matching the query, the current one left out.
    fn matches(&self, cx: &App) -> Vec<RecentProject> {
        let query = self.query.read(cx).text().trim().to_lowercase();
        self.recent
            .iter()
            .filter(|p| Some(p.path.as_path()) != self.project.as_deref())
            .filter(|p| {
                query.is_empty() || p.path.to_string_lossy().to_lowercase().contains(&query)
            })
            .cloned()
            .collect()
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let matches = self.matches(cx);
        if let Some(pick) = matches.get(self.selected) {
            self.pick(pick.path.clone(), window, cx);
        } else {
            // Nothing matches: a typed path is opened as is.
            let typed = self.query.read(cx).text().trim().to_string();
            let path = match typed.strip_prefix("~/") {
                Some(rest) => self.home.as_ref().map(|h| h.join(rest)),
                None if !typed.is_empty() => Some(PathBuf::from(typed)),
                None => None,
            };
            if let Some(path) = path.filter(|p| p.is_dir()) {
                self.pick(path, window, cx);
            }
        }
    }

    fn pick(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.close_switcher(window, cx);
        if !path.is_dir() {
            self.show_message(format!("{} no longer exists", path.display()), true, cx);
            return;
        }
        cx.emit(ProjectBarEvent::Open(path));
    }

    fn forget(&mut self, path: &Path, cx: &mut Context<Self>) {
        if let Some(file) = self.projects_file.clone()
            && let Ok(list) = project::forget(&file, path)
        {
            self.recent = list;
            self.selected = 0;
            cx.notify();
        }
    }

    /// The system folder dialog; the folder chosen is opened.
    pub fn open_folder_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.switcher_open {
            self.close_switcher(window, cx);
        }
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            if let Some(path) = paths.into_iter().next() {
                this.update(cx, |_, cx| cx.emit(ProjectBarEvent::Open(path)))
                    .ok();
            }
        })
        .detach();
        cx.notify();
    }

    fn open_in(&mut self, app: FerrumApp, cx: &mut Context<Self>) {
        let Some(path) = self.project.clone() else {
            return;
        };
        match project::open_in(app, &path) {
            Ok(()) => self.show_message(format!("Opening in {}…", app.name()), false, cx),
            Err(e) => self.show_message(e, true, cx),
        }
    }

    fn show_message(&mut self, text: impl Into<SharedString>, error: bool, cx: &mut Context<Self>) {
        self.message = Some((text.into(), error));
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(4))
                .await;
            this.update(cx, |this, cx| {
                this.message = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn on_switcher_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = self.matches(cx).len();
        match event.keystroke.key.as_str() {
            "escape" => self.close_switcher(window, cx),
            "down" if count > 0 => self.selected = (self.selected + 1) % count,
            "up" if count > 0 => self.selected = (self.selected + count - 1) % count,
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn render_name(&self, cx: &mut Context<Self>) -> AnyElement {
        let name: SharedString = match (&self.project, &self.title) {
            (Some(p), _) => project::project_name(p).into(),
            (None, Some(title)) => title.clone(),
            (None, None) => "No project".into(),
        };
        let named = self.project.is_some() || self.title.is_some();
        let colors = cx.theme().colors();
        let mut name_el = h_flex()
            .id("project-name")
            .when(self.inline, |el| el.flex_shrink(1.).min_w_0())
            .when(!self.inline, |el| el.flex_none())
            .gap_1()
            .px_1p5()
            .h(px(22.))
            .rounded_sm()
            .child(
                Icon::new(IconName::FolderOpen)
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_ui_sm(cx)
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(if named {
                        colors.text
                    } else {
                        colors.text_muted
                    })
                    .child(name),
            );
        if self.switchable {
            name_el = name_el
                .cursor_pointer()
                .hover(|s| s.bg(colors.element_hover))
                .child(
                    Icon::new(IconName::ChevronDown)
                        .size(IconSize::XSmall)
                        .color(Color::Muted),
                )
                .tooltip(|_, cx| Tooltip::for_action("Switch Project", &ToggleProjectSwitcher, cx))
                .on_click(cx.listener(|this, _, window, cx| this.toggle_switcher(window, cx)));
        }
        name_el.into_any_element()
    }

    /// `~ / Projects / ferrum`: each segment reveals its folder.
    fn render_path(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if let Some(detail) = &self.detail {
            return Some(
                Label::new(detail.clone())
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .truncate()
                    .into_any_element(),
            );
        }
        let path = self.project.clone()?;
        let colors = cx.theme().colors();
        let shown = project::display_path(&path, self.home.as_deref());
        let tilde = shown.starts_with('~');
        let mut cumulative = if tilde {
            self.home.clone().unwrap_or_default()
        } else {
            PathBuf::from("/")
        };
        let mut row = h_flex()
            .id("project-path")
            .min_w_0()
            .overflow_hidden()
            .gap_0p5()
            .text_ui_sm(cx)
            .text_color(colors.text_muted)
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _, _, cx| {
                    if let Some(p) = &this.project {
                        cx.write_to_clipboard(ClipboardItem::new_string(p.display().to_string()));
                        this.show_message("Path copied", false, cx);
                    }
                }),
            )
            .tooltip(|_, cx| {
                Tooltip::simple(
                    "Click a folder to show it in Finder · right-click copies the path",
                    cx,
                )
            });
        let segments: Vec<String> = shown
            .split('/')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        for (index, segment) in segments.iter().enumerate() {
            if index == 0 && tilde {
                // `~` is the home folder itself.
            } else {
                cumulative.push(segment);
            }
            if index > 0 {
                row = row.child(div().flex_none().opacity(0.5).child("/"));
            }
            let target = cumulative.clone();
            let last = index + 1 == segments.len();
            row = row.child(
                div()
                    .id(SharedString::from(format!("project-path-{index}")))
                    .flex_none()
                    .px_0p5()
                    .rounded_sm()
                    .cursor_pointer()
                    .when(last, |d| d.text_color(colors.text))
                    .hover(|s| s.bg(colors.element_hover).text_color(colors.text))
                    .on_click(cx.listener(move |_, _, _, cx| cx.reveal_path(&target)))
                    .child(segment.clone()),
            );
        }
        Some(row.into_any_element())
    }

    fn render_switcher(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors();
        let matches = self.matches(cx);
        let mut list = v_flex()
            .id("project-switcher-list")
            .max_h(px(360.))
            .overflow_y_scroll();
        if matches.is_empty() {
            list = list.child(
                div().px_3().py_2().child(
                    Label::new(if self.recent.len() <= 1 {
                        "Projects you open in any Ferrum app appear here"
                    } else {
                        "No recent project matches. Enter a folder path to open it."
                    })
                    .size(LabelSize::Small)
                    .color(Color::Muted),
                ),
            );
        }
        for (index, item) in matches.iter().enumerate() {
            let path = item.path.clone();
            let forget_path = item.path.clone();
            let selected = index == self.selected;
            list = list.child(
                h_flex()
                    .id(SharedString::from(format!("recent-{index}")))
                    .group("recent-row")
                    .px_2()
                    .py_1()
                    .gap_2()
                    .rounded_sm()
                    .cursor_pointer()
                    .when(selected, |d| d.bg(colors.element_selected))
                    .hover(|s| s.bg(colors.element_hover))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.pick(path.clone(), window, cx);
                    }))
                    .child(
                        Icon::new(IconName::Folder)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(Label::new(item.name()).size(LabelSize::Small))
                            .child(
                                Label::new(project::display_path(&item.path, self.home.as_deref()))
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted)
                                    .truncate(),
                            ),
                    )
                    .child(
                        div()
                            .invisible()
                            .group_hover("recent-row", |s| s.visible())
                            .child(
                                IconButton::new(
                                    SharedString::from(format!("forget-{index}")),
                                    IconName::Close,
                                )
                                .icon_size(IconSize::XSmall)
                                .tooltip(Tooltip::text("Remove from recent projects"))
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.forget(&forget_path, cx);
                                    },
                                )),
                            ),
                    ),
            );
        }
        let body = v_flex()
            .id("project-switcher")
            .occlude()
            .w(px(440.))
            .p_1()
            .gap_1()
            .rounded_md()
            .border_1()
            .border_color(colors.border)
            .bg(colors.elevated_surface_background)
            .shadow_lg()
            .capture_key_down(cx.listener(Self::on_switcher_key))
            .on_mouse_down_out(cx.listener(|this, _, window, cx| this.close_switcher(window, cx)))
            .child(div().p_1().child(self.query.clone()))
            .child(list)
            .child(
                h_flex()
                    .border_t_1()
                    .border_color(colors.border_variant)
                    .pt_1()
                    .child(
                        Button::new("open-folder", "Open Folder…")
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .start_icon(
                                Icon::new(IconName::Plus)
                                    .size(IconSize::XSmall)
                                    .color(Color::Muted),
                            )
                            .full_width()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_folder_dialog(window, cx);
                            })),
                    ),
            );
        let _ = window;
        deferred(
            anchored()
                .anchor(Anchor::TopLeft)
                .position(point(px(8.), px(30.)))
                .snap_to_window_with_margin(px(8.))
                .child(body),
        )
        .with_priority(2)
        .into_any_element()
    }
}

impl Render for ProjectBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let this_app = FerrumApp::from_binary(self.app_binary);
        // A detail (e.g. a database file) may describe something inside the
        // folder: the folder can still be opened in the other apps.
        let has_project = self.project.is_some();
        let mut right = h_flex().flex_none().gap_0p5();
        if let Some((message, error)) = &self.message {
            right = right.child(
                div().pr_2().child(
                    Label::new(message.clone())
                        .size(LabelSize::XSmall)
                        .color(if *error { Color::Error } else { Color::Muted }),
                ),
            );
        }
        if has_project {
            for app in FerrumApp::ALL.into_iter().filter(|a| Some(*a) != this_app) {
                let icon = match app {
                    FerrumApp::Editor => IconName::Code,
                    FerrumApp::Git => IconName::GitBranch,
                    FerrumApp::Term => IconName::Terminal,
                };
                if self.inline {
                    right = right.child(
                        IconButton::new(
                            SharedString::from(format!("open-in-{}", app.binary())),
                            icon,
                        )
                        .icon_size(IconSize::Small)
                        .icon_color(Color::Muted)
                        .tooltip(Tooltip::text(format!(
                            "Open this project in {}",
                            app.name()
                        )))
                        .on_click(cx.listener(move |this, _, _, cx| this.open_in(app, cx))),
                    );
                    continue;
                }
                right = right.child(
                    Button::new(
                        SharedString::from(format!("open-in-{}", app.binary())),
                        app.label(),
                    )
                    .style(ButtonStyle::Subtle)
                    .size(ButtonSize::Compact)
                    .label_size(LabelSize::XSmall)
                    .start_icon(Icon::new(icon).size(IconSize::XSmall).color(Color::Muted))
                    .tooltip(Tooltip::text(format!(
                        "Open this project in {}",
                        app.name()
                    )))
                    .on_click(cx.listener(move |this, _, _, cx| this.open_in(app, cx))),
                );
            }
        }
        h_flex()
            .id("project-bar")
            .key_context("ProjectBar")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &ToggleProjectSwitcher, window, cx| {
                this.toggle_switcher(window, cx);
            }))
            .flex_none()
            .w_full()
            .h(px(30.))
            .px_2()
            .gap_2()
            .when(!self.inline, |bar| {
                bar.bg(colors.title_bar_background)
                    .border_b_1()
                    .border_color(colors.border_variant)
            })
            .child(self.render_name(cx))
            .children((!self.inline).then(|| self.render_path(cx)).flatten())
            .when_some(self.branch.clone(), |row, branch| {
                row.child(
                    h_flex()
                        .when(self.inline, |el| {
                            el.flex_shrink(1.).min_w_0().overflow_hidden()
                        })
                        .when(!self.inline, |el| el.flex_none())
                        .gap_1()
                        .child(
                            Icon::new(IconName::GitBranch)
                                .size(IconSize::XSmall)
                                .color(Color::Muted),
                        )
                        .child(
                            Label::new(branch)
                                .size(LabelSize::XSmall)
                                .color(Color::Muted)
                                .truncate(),
                        ),
                )
            })
            .child(div().flex_1())
            .child(right)
            .when(self.switcher_open, |row| {
                row.child(self.render_switcher(window, cx))
            })
    }
}

fn tracing_warn(message: &str) {
    eprintln!("ferrum-ui: {message}");
}
