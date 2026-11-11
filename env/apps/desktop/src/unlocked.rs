//! The unlocked vault: project strip, project sidebar, the selected
//! project's environments and the status bar. Everything decrypted lives
//! in [`Unlocked`], which is dropped (and wiped) when the app locks.

use std::collections::HashSet;
use std::path::Path;
use std::time::Instant;

use ferrum_ui::prelude::*;
use ferrum_ui::project;
use ferrumenv_core::Vault;
use ferrumenv_core::lock::AutoLock;
use ferrumenv_core::model::{Environment, Project};
use ferrumenv_core::store::Session;

use crate::app::{EnvApp, Lock, OpenSettings};
use crate::dialogs::Dialog;
use crate::widgets::{icon_button, muted, text_button};

pub(crate) struct Unlocked {
    pub session: Session,
    pub vault: Vault,
    pub bar: Entity<ProjectBar>,
    pub search: Entity<TextInput>,
    pub project: Option<usize>,
    pub env: usize,
    /// Keys whose values are shown in the current environment.
    pub revealed: HashSet<String>,
    pub reveal_all: bool,
    pub dialog: Option<Dialog>,
    pub dialog_error: Option<SharedString>,
    pub dialog_subscriptions: Vec<Subscription>,
    pub auto_lock: AutoLock,
    _subscriptions: Vec<Subscription>,
}

impl Unlocked {
    pub fn new(
        session: Session,
        vault: Vault,
        window: &mut Window,
        cx: &mut Context<EnvApp>,
    ) -> Self {
        let bar = cx.new(|cx| ProjectBar::new("ferrumenv", window, cx));
        let search = cx.new(|cx| TextInput::new(window, cx).placeholder("Search projects"));
        let subscriptions = vec![
            cx.subscribe_in(&bar, window, |this, _, event, window, cx| match event {
                ProjectBarEvent::Open(path) => this.open_folder(path, window, cx),
            }),
            cx.subscribe(&search, |_, _, event, cx| {
                if *event == TextInputEvent::Changed {
                    cx.notify();
                }
            }),
        ];
        let auto_lock = AutoLock::new(vault.settings.auto_lock_minutes, Instant::now());
        Self {
            session,
            vault,
            bar,
            search,
            project: None,
            env: 0,
            revealed: HashSet::new(),
            reveal_all: false,
            dialog: None,
            dialog_error: None,
            dialog_subscriptions: Vec::new(),
            auto_lock,
            _subscriptions: subscriptions,
        }
    }

    pub fn project(&self) -> Option<&Project> {
        self.project.and_then(|i| self.vault.projects.get(i))
    }

    pub fn project_mut(&mut self) -> Option<&mut Project> {
        self.project.and_then(|i| self.vault.projects.get_mut(i))
    }

    pub fn environment(&self) -> Option<&Environment> {
        self.project().and_then(|p| p.environments.get(self.env))
    }

    pub fn environment_mut(&mut self) -> Option<&mut Environment> {
        let env = self.env;
        self.project_mut().and_then(|p| p.environments.get_mut(env))
    }
}

impl EnvApp {
    /// Applies `change` to the unlocked vault and saves it. Errors (from
    /// the change or the save) are shown in the open dialog or as a notice. Returns the change's value.
    pub(crate) fn change<R>(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Unlocked) -> Result<R, String>,
    ) -> Option<R> {
        let unlocked = self.vault_mut()?;
        let result = change(unlocked).and_then(|value| {
            if let Some(project) = unlocked.project_mut() {
                project.touch();
            }
            unlocked
                .session
                .save(&unlocked.vault)
                .map(|()| value)
                .map_err(|e| format!("Could not save the vault: {e}"))
        });
        cx.notify();
        match result {
            Ok(value) => Some(value),
            Err(message) => {
                // Inside a dialog the error shows there; otherwise as a notice.
                if self.vault().is_some_and(|u| u.dialog.is_some()) {
                    self.set_dialog_error(message, cx);
                } else {
                    self.show_error(message, cx);
                }
                None
            }
        }
    }

    pub(crate) fn select_project(&mut self, index: usize, _: &mut Window, cx: &mut Context<Self>) {
        let record = self.config.record_recent;
        let Some(unlocked) = self.vault_mut() else {
            return;
        };
        let Some(project) = unlocked.vault.projects.get(index) else {
            return;
        };
        let path = project.path.clone();
        let name = project.name.clone();
        unlocked.project = Some(index);
        unlocked.env = 0;
        unlocked.revealed.clear();
        unlocked.reveal_all = false;
        unlocked.bar.update(cx, |bar, cx| match path {
            Some(path) if record => {
                bar.set_project(Some(path), cx);
                bar.set_detail(None, cx);
            }
            Some(path) => {
                let home = project::home_dir();
                bar.set_detail(
                    Some(project::display_path(&path, home.as_deref()).into()),
                    cx,
                );
            }
            None => {
                bar.set_project(None, cx);
                bar.set_detail(Some(format!("{name} · not linked to a folder").into()), cx);
            }
        });
        cx.notify();
    }

    pub(crate) fn select_environment(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(unlocked) = self.vault_mut() {
            unlocked.env = index;
            unlocked.revealed.clear();
            unlocked.reveal_all = false;
            cx.notify();
        }
    }

    /// A folder picked in the project strip: its project, created if new.
    pub(crate) fn open_folder(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let path = path.to_path_buf();
        if let Some(index) = self.change(cx, |u| Ok(u.vault.project_for_folder(&path))) {
            self.select_project(index, window, cx);
        }
    }

    pub(crate) fn render_vault(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(unlocked) = self.vault() else {
            return div().into_any_element();
        };
        let bar = unlocked.bar.clone();
        v_flex()
            .size_full()
            .child(bar)
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_sidebar(cx))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(self.render_project(window, cx)),
                    ),
            )
            .child(self.render_status(cx))
            .into_any_element()
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let mut list = div()
            .id("project-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_1();
        let mut sidebar = v_flex()
            .w(px(240.))
            .flex_none()
            .h_full()
            .bg(colors.panel_background)
            .border_r_1()
            .border_color(colors.border);
        if let Some(unlocked) = self.vault() {
            let query = unlocked.search.read(cx).text().to_string();
            let matches = unlocked.vault.search(&query);
            let home = project::home_dir();
            if matches.is_empty() {
                list = list.child(if unlocked.vault.projects.is_empty() {
                    empty_state("No projects yet", "Create one with +")
                } else {
                    empty_state("No projects match", "Try another word")
                });
            }
            for index in matches {
                let p = &unlocked.vault.projects[index];
                let detail = p.path.as_deref().map_or_else(
                    || "free project".to_string(),
                    |path| project::display_path(path, home.as_deref()),
                );
                list = list.child(
                    ListItem::new(SharedString::from(format!("project-{index}")))
                        .rounded()
                        .toggle_state(unlocked.project == Some(index))
                        .child(
                            v_flex()
                                .child(Label::new(p.name.clone()))
                                .child(muted(detail).size(LabelSize::XSmall)),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_project(index, window, cx);
                        })),
                );
            }
            sidebar = sidebar.child(
                h_flex()
                    .p_2()
                    .gap_1()
                    .child(div().flex_1().child(unlocked.search.clone()))
                    .child(icon_button(
                        "new-project",
                        IconName::Plus,
                        "New project",
                        cx,
                        |this, window, cx| {
                            this.open_new_project(window, cx);
                        },
                    )),
            );
        }
        sidebar.child(section_header("Projects")).child(list).child(
            v_flex()
                .p_2()
                .border_t_1()
                .border_color(colors.border_variant)
                .child(
                    text_button(
                        "import-bundle",
                        "Import bundle…",
                        cx,
                        |this, window, cx| {
                            this.open_import_bundle(window, cx);
                        },
                    )
                    .full_width()
                    .start_icon(Icon::new(IconName::Download).size(IconSize::Small)),
                )
                .child(
                    text_button("settings", "Settings", cx, |this, window, cx| {
                        this.open_settings(window, cx);
                    })
                    .full_width()
                    .start_icon(Icon::new(IconName::Settings).size(IconSize::Small))
                    .tooltip(|_, cx| Tooltip::for_action("Settings", &OpenSettings, cx)),
                ),
        )
    }

    fn render_status(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (summary, lock_label) = self.vault().map_or_else(Default::default, |u| {
            let variables: usize = u
                .vault
                .projects
                .iter()
                .flat_map(|p| &p.environments)
                .map(|e| e.variables.len())
                .sum();
            (
                format!(
                    "{} projects · {variables} variables · encrypted",
                    u.vault.projects.len()
                ),
                u.auto_lock.label(Instant::now()),
            )
        });
        status_bar(cx)
            .child(Label::new(summary).size(LabelSize::Small))
            .child(
                h_flex()
                    .gap_2()
                    .child(Label::new(lock_label).size(LabelSize::Small))
                    .child(
                        Button::new("lock-now", "Lock")
                            .size(ButtonSize::Compact)
                            .label_size(LabelSize::Small)
                            .start_icon(Icon::new(IconName::Lock).size(IconSize::XSmall))
                            .tooltip(|_, cx| Tooltip::for_action("Lock now", &Lock, cx))
                            .on_click(cx.listener(|this, _, window, cx| this.lock(window, cx))),
                    ),
            )
    }
}
