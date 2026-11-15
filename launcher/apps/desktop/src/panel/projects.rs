//! The Projects tab: recent projects from `~/.ferrum/projects.json`, each
//! with the apps it was opened in. A click opens it in the first of them;
//! an app's logo opens it in that app.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use ferrum_ui::gpui::FontWeight;
use ferrum_ui::prelude::*;
use ferrum_ui::project::{self, FerrumApp, RecentProject};
use ferrumlauncher_core::AppId;

use super::LauncherPanel;
use super::layout::matches;

/// Recent projects, newest first (none when the file is missing).
pub(super) fn load() -> Vec<RecentProject> {
    project::projects_file()
        .map(|file| project::load(&file))
        .unwrap_or_default()
}

/// Opens `folder` in the app whose executable is `binary`: the Editor,
/// FerrumGIT and FerrumTerm take the folder; other apps start in it.
fn open_folder(binary: &str, name: &str, folder: &Path) -> Result<(), String> {
    if let Some(app) = FerrumApp::from_binary(binary) {
        return project::open_in(app, folder);
    }
    let home = project::home_dir();
    let exe = project::find_binary(binary, home.as_deref())
        .ok_or_else(|| format!("{name} is not installed. Install it in Ferrum Launcher."))?;
    Command::new(exe)
        .current_dir(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("Could not start {name}: {e}"))
}

impl LauncherPanel {
    pub(super) fn render_projects(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let query = self.query(cx);
        let home = project::home_dir();
        let projects: Vec<(RecentProject, String)> = self
            .projects
            .iter()
            .map(|p| (p.clone(), project::display_path(&p.path, home.as_deref())))
            .filter(|(p, shown)| matches(&query, &p.name()) || matches(&query, shown))
            .collect();
        if projects.is_empty() {
            let text = if self.projects.is_empty() {
                "Folders you open in FerrumIDE, FerrumGIT or FerrumTerm show up here.".to_string()
            } else {
                format!("No project matches “{query}”")
            };
            return vec![
                div()
                    .p_4()
                    .child(Label::new(text).size(LabelSize::Small).color(Color::Muted))
                    .into_any_element(),
            ];
        }
        projects
            .into_iter()
            .enumerate()
            .map(|(ix, (project, shown))| self.render_project(ix, project, shown, cx))
            .collect()
    }

    /// The installed-or-known apps `project` was opened in, by app id.
    fn project_apps(&self, project: &RecentProject, cx: &App) -> Vec<AppId> {
        let state = &self.launcher.read(cx).state;
        let apps = state.managed_apps();
        project
            .apps
            .iter()
            .filter_map(|binary| apps.iter().find(|info| &info.binary == binary))
            .map(|info| info.id)
            .collect()
    }

    fn render_project(
        &self,
        ix: usize,
        project: RecentProject,
        shown: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = ferrum_ui::hover_fill(cx);
        let apps = self.project_apps(&project, cx);
        let first = apps.first().copied();
        let path = project.path.clone();
        let logos: Vec<AnyElement> = apps
            .iter()
            .map(|&app| {
                let name = self.launcher.read(cx).state.info(app).name;
                let logo = self.launcher.read(cx).logo(app, px(18.));
                let path = path.clone();
                div()
                    .id(SharedString::from(format!("panel-project-{ix}-{app}")))
                    .p_0p5()
                    .rounded(px(5.))
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .tooltip(Tooltip::text(format!("Open in {name}")))
                    .child(logo)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.open_project(path.clone(), app, window, cx);
                    }))
                    .into_any_element()
            })
            .collect();
        h_flex()
            .id(("panel-project", ix))
            .px_2()
            .py_1p5()
            .gap_2p5()
            .rounded(px(8.))
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .child(
                Icon::new(IconName::Folder)
                    .size(IconSize::Medium)
                    .color(Color::Muted),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        Label::new(project.name())
                            .weight(FontWeight::SEMIBOLD)
                            .truncate(),
                    )
                    .child(
                        Label::new(shown)
                            .size(LabelSize::XSmall)
                            .color(Color::Muted)
                            .truncate(),
                    ),
            )
            .child(h_flex().flex_none().gap_0p5().children(logos))
            .on_click(cx.listener(move |this, _, window, cx| match first {
                Some(app) => this.open_project(path.clone(), app, window, cx),
                None => this.open_project(path.clone(), AppId::EDITOR, window, cx),
            }))
            .into_any_element()
    }

    fn open_project(
        &mut self,
        folder: PathBuf,
        app: AppId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let info = self.launcher.read(cx).state.info(app);
        match open_folder(&info.binary, &info.name, &folder) {
            Ok(()) => self.close(window, cx),
            Err(e) => self.launcher.update(cx, |l, cx| l.notify_user(e, true, cx)),
        }
    }
}
