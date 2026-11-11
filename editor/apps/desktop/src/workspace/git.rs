//! Git in the workspace: the Editor | Git | CI | Logs switch at the top right, the
//! Files | Git tabs of the sidebar, `git:*` commands, and refreshing git
//! status after saves and when the window comes back.

use ferrum_ui::prelude::*;
use ui::{Button, ButtonStyle, IconButton, IconName, IconSize};

use super::Workspace;
use crate::git::{GitEvent, GitPanelEvent, GitState};

/// What fills the window below the top bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    Editor,
    Git,
    /// AI Diff Review of an agent session.
    Review,
    /// The CI page (GitHub Actions of the project, see `workspace/ci.rs`).
    Ci,
    /// The Logs page (live logs of the project, see `workspace/logs.rs`).
    Logs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarTab {
    #[default]
    Files,
    Git,
}

impl Workspace {
    pub(super) fn on_git_event(
        &mut self,
        _: Entity<GitState>,
        event: &GitEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            GitEvent::Notice(message) => self.info(message.clone(), cx),
            GitEvent::Error(message) => self.error(message.clone(), cx),
        }
    }

    pub(super) fn on_git_panel_event(
        &mut self,
        event: &GitPanelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            // In the editor, clicking a changed file opens it (its gutter
            // shows the changes); the Git view shows the diff instead.
            GitPanelEvent::Selected { path, .. } if self.mode == ViewMode::Editor => {
                if let Some(root) = self.git.read(cx).root().map(std::path::Path::to_path_buf) {
                    let full = root.join(path);
                    if full.is_file() {
                        self.open_file(&full, window, cx);
                    }
                }
            }
            GitPanelEvent::Notice(message) => self.info(message.clone(), cx),
            GitPanelEvent::Selected { .. } => {}
        }
    }

    pub(super) fn set_mode(&mut self, mode: ViewMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        if mode == ViewMode::Git {
            self.git.update(cx, |g, cx| g.refresh(false, cx));
        } else if mode == ViewMode::Ci {
            self.show_ci(window, cx);
        } else if mode == ViewMode::Logs {
            self.show_logs(window, cx);
        } else {
            self.focus_active(window, cx);
        }
        cx.notify();
    }

    pub(super) fn toggle_git_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = match self.mode {
            ViewMode::Editor | ViewMode::Ci | ViewMode::Logs => ViewMode::Git,
            ViewMode::Git | ViewMode::Review => ViewMode::Editor,
        };
        self.set_mode(next, window, cx);
    }

    /// Shows the Git tab of the sidebar (or goes back to files).
    pub(super) fn toggle_git_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.root.is_none() {
            self.info("Open a folder to see its Git changes (⌘⇧O)", cx);
            return;
        }
        self.set_mode(ViewMode::Editor, window, cx);
        if self.sidebar == SidebarTab::Git && self.tree_visible {
            self.sidebar = SidebarTab::Files;
        } else {
            self.sidebar = SidebarTab::Git;
            self.tree_visible = true;
            self.git.update(cx, |g, cx| g.refresh(false, cx));
        }
        cx.notify();
    }

    pub(super) fn refresh_git(&mut self, cx: &mut Context<Self>) {
        self.git.update(cx, |g, cx| g.refresh(true, cx));
    }

    /// The bar at the top: the shared project bar (name, path, branch,
    /// switcher) with the Editor | Git | CI | Logs switch at its right end.
    pub(super) fn render_top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let git = self.git.read(cx);
        let changes = git.change_count();
        let git_label = if changes > 0 {
            format!("Git {changes}")
        } else {
            "Git".to_string()
        };
        h_flex()
            .flex_none()
            .w_full()
            .child(div().flex_1().min_w_0().child(self.project_bar.clone()))
            .child(
                h_flex()
                    .flex_none()
                    .h(px(30.))
                    .px_2()
                    .bg(colors.title_bar_background)
                    .border_b_1()
                    .border_color(colors.border_variant)
                    .child(
                        h_flex()
                            .gap_0p5()
                            .p_0p5()
                            .rounded_md()
                            .bg(colors.element_background)
                            .child(
                                Button::new("mode-editor", "Editor")
                                    .style(ButtonStyle::Subtle)
                                    .toggle_state(self.mode == ViewMode::Editor)
                                    .tooltip(Tooltip::text("Editor"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.set_mode(ViewMode::Editor, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("mode-git", git_label)
                                    .style(ButtonStyle::Subtle)
                                    .toggle_state(self.mode == ViewMode::Git)
                                    .tooltip(Tooltip::text("Git: changes, diffs and history (⌥⌘G)"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.set_mode(ViewMode::Git, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("mode-ci", "CI")
                                    .style(ButtonStyle::Subtle)
                                    .toggle_state(self.mode == ViewMode::Ci)
                                    .tooltip(Tooltip::text(
                                        "CI: GitHub Actions runs, logs and PR checks (⇧⌘I)",
                                    ))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.set_mode(ViewMode::Ci, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("mode-logs", "Logs")
                                    .style(ButtonStyle::Subtle)
                                    .toggle_state(self.mode == ViewMode::Logs)
                                    .tooltip(Tooltip::text(
                                        "Logs: live logs of the project's files, commands and containers (⌥⌘L)",
                                    ))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.set_mode(ViewMode::Logs, window, cx)
                                    })),
                            ),
                    ),
            )
    }

    /// The left sidebar: Files | Git tabs over the tree view or Git panel.
    pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.tree_visible || self.root.is_none() {
            return None;
        }
        let colors = cx.theme().colors().clone();
        let changes = self.git.read(cx).change_count();
        let content: AnyElement = match (self.sidebar, self.tree.clone()) {
            (SidebarTab::Git, _) => self.git_panel.clone().into_any_element(),
            (SidebarTab::Files, Some(tree)) => tree.into_any_element(),
            (SidebarTab::Files, None) => div().into_any_element(),
        };
        let tab = |id: &'static str, icon: IconName, active: bool, tip: String| {
            IconButton::new(id, icon)
                .icon_size(IconSize::Small)
                .toggle_state(active)
                .when(active, |b| b.icon_color(Color::Accent))
                .tooltip(Tooltip::text(tip))
        };
        Some(
            v_flex()
                .size_full()
                .child(
                    h_flex()
                        .flex_none()
                        .h(px(28.))
                        .px_1()
                        .gap_1()
                        .bg(colors.panel_background)
                        .border_b_1()
                        .border_color(colors.border)
                        .child(
                            tab(
                                "sidebar-files",
                                IconName::Folder,
                                self.sidebar == SidebarTab::Files,
                                "Files".to_string(),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sidebar = SidebarTab::Files;
                                cx.notify();
                            })),
                        )
                        .child(
                            tab(
                                "sidebar-git",
                                IconName::GitBranch,
                                self.sidebar == SidebarTab::Git,
                                format!("Git changes ({changes}) ⌃⇧G"),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sidebar = SidebarTab::Git;
                                this.git.update(cx, |g, cx| g.refresh(false, cx));
                                cx.notify();
                            })),
                        )
                        .when(changes > 0, |d| {
                            d.child(
                                Label::new(changes.to_string())
                                    .size(LabelSize::XSmall)
                                    .color(Color::Accent),
                            )
                        }),
                )
                .child(div().flex_1().min_h_0().child(content))
                .into_any_element(),
        )
    }
}
