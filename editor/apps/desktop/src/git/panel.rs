//! The Git sidebar panel: branch and sync buttons, the commit message,
//! and staged / unstaged files with stage, unstage and discard.

use ferrum_ui::components::{Button, ButtonStyle, Icon, IconButton, IconName, IconSize};
use ferrum_ui::prelude::*;
use ferrumgit_domain::{ChangeKind, FileStatus};
use gpui::{PromptLevel, ScrollHandle};

use super::{GitState, open_in_ferrumgit};
use crate::settings::Settings;
use crate::theme_apply::ui;

/// Files listed per section at most (FerrumGIT handles huge change sets).
const MAX_ROWS: usize = 400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitPanelEvent {
    /// A file row was clicked.
    Selected {
        path: String,
        staged: bool,
    },
    Notice(String),
}

pub struct GitPanel {
    pub state: Entity<GitState>,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<GitPanelEvent> for GitPanel {}

impl GitPanel {
    pub fn new(state: Entity<GitState>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&state, |_, _, cx| cx.notify());
        Self {
            state,
            scroll: ScrollHandle::new(),
            _subscriptions: vec![subscription],
        }
    }

    fn change_color(kind: ChangeKind, cx: &App) -> gpui::Hsla {
        let theme = &cx.global::<Settings>().theme;
        match kind {
            ChangeKind::Added | ChangeKind::Untracked | ChangeKind::Copied => {
                ui(theme, "git_added")
            }
            ChangeKind::Deleted | ChangeKind::Conflicted => ui(theme, "git_deleted"),
            _ => ui(theme, "git_modified"),
        }
    }

    fn render_row(
        &self,
        file: &FileStatus,
        staged: bool,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let kind = if staged { file.index } else { file.worktree };
        let kind = if file.is_conflicted() {
            ChangeKind::Conflicted
        } else {
            kind
        };
        let color = Self::change_color(kind, cx);
        let (dir, name) = match file.path.rsplit_once('/') {
            Some((dir, name)) => (dir.to_string(), name.to_string()),
            None => (String::new(), file.path.clone()),
        };
        let path = file.path.clone();
        let id = SharedString::from(format!("git-{}-{}", if staged { "s" } else { "u" }, path));
        let colors = cx.theme().colors().clone();
        let state = self.state.clone();
        let mut actions = h_flex().gap_0p5().flex_none();
        if staged {
            let state = state.clone();
            let path = path.clone();
            actions = actions.child(
                IconButton::new(SharedString::from(format!("{id}-unstage")), IconName::Dash)
                    .icon_size(IconSize::XSmall)
                    .tooltip(Tooltip::text("Unstage"))
                    .on_click(move |_, _, cx| state.update(cx, |s, cx| s.unstage(&path, cx))),
            );
        } else {
            let discard_state = state.clone();
            let discard_path = path.clone();
            let untracked = file.worktree == ChangeKind::Untracked;
            actions = actions
                .child(
                    IconButton::new(SharedString::from(format!("{id}-discard")), IconName::Undo)
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text(if untracked {
                            "Delete File"
                        } else {
                            "Discard Changes"
                        }))
                        .on_click(move |_, window, cx| {
                            let detail = if untracked {
                                format!("{discard_path} is not tracked: it will be deleted.")
                            } else {
                                format!("Your changes to {discard_path} will be lost.")
                            };
                            let answer = window.prompt(
                                PromptLevel::Warning,
                                "Discard changes?",
                                Some(&detail),
                                &["Discard", "Cancel"],
                                cx,
                            );
                            let state = discard_state.clone();
                            let path = discard_path.clone();
                            cx.spawn(async move |cx| {
                                if answer.await == Ok(0) {
                                    state.update(cx, |s, cx| s.discard(&path, cx));
                                }
                            })
                            .detach();
                        }),
                )
                .child({
                    let state = state.clone();
                    let path = path.clone();
                    IconButton::new(SharedString::from(format!("{id}-stage")), IconName::Plus)
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text("Stage"))
                        .on_click(move |_, _, cx| state.update(cx, |s, cx| s.stage(&path, cx)))
                });
        }
        h_flex()
            .id(id)
            .px_2()
            .h(px(24.))
            .gap_1p5()
            .cursor_pointer()
            .when(selected, |d| d.bg(colors.element_selected))
            .hover(|d| d.bg(colors.element_hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.state
                    .update(cx, |s, cx| s.select_file(&path, staged, cx));
                cx.emit(GitPanelEvent::Selected {
                    path: path.clone(),
                    staged,
                });
            }))
            .child(
                div()
                    .w(px(12.))
                    .flex_none()
                    .text_color(color)
                    .text_xs()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(kind.letter()),
            )
            .child(
                div()
                    .flex_none()
                    .max_w(px(180.))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_sm()
                    .child(name),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_xs()
                    .text_color(colors.text_muted)
                    .child(dir),
            )
            .child(actions)
    }

    fn section(
        &self,
        title: &str,
        files: Vec<&FileStatus>,
        staged: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let state = self.state.read(cx);
        let selected = match &state.selection {
            Some(super::GitSelection::File { path, staged: s }) if *s == staged => {
                Some(path.clone())
            }
            _ => None,
        };
        let count = files.len();
        let all_state = self.state.clone();
        let header = h_flex()
            .justify_between()
            .pr_1()
            .child(section_header(format!("{title} ({count})")))
            .child(
                IconButton::new(
                    if staged {
                        "git-unstage-all"
                    } else {
                        "git-stage-all"
                    },
                    if staged {
                        IconName::Dash
                    } else {
                        IconName::Plus
                    },
                )
                .icon_size(IconSize::XSmall)
                .tooltip(Tooltip::text(if staged {
                    "Unstage All"
                } else {
                    "Stage All"
                }))
                .on_click(move |_, _, cx| {
                    all_state.update(cx, |s, cx| {
                        if staged {
                            s.unstage_all(cx)
                        } else {
                            s.stage_all(cx)
                        }
                    })
                }),
            );
        let rows: Vec<AnyElement> = files
            .iter()
            .take(MAX_ROWS)
            .map(|f| {
                let is_selected = selected.as_deref() == Some(f.path.as_str());
                self.render_row(f, staged, is_selected, cx)
                    .into_any_element()
            })
            .collect();
        v_flex()
            .child(header)
            .children(rows)
            .when(count > MAX_ROWS, |d| {
                d.child(
                    div()
                        .px_3()
                        .text_xs()
                        .text_color(cx.theme().colors().text_muted)
                        .child(format!(
                            "… and {} more (Open in FerrumGIT)",
                            count - MAX_ROWS
                        )),
                )
            })
    }
}

impl Render for GitPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let state = self.state.read(cx);
        let root = state.root().map(std::path::Path::to_path_buf);
        let branch = state.branch_label();
        let busy = state.busy.clone();
        let not_a_repo = state.not_a_repo;
        let loaded = state.loaded;
        let message = state.message.clone();
        let staged_count = state.status.staged().len();
        let has_changes = !state.status.files.is_empty();
        let operation = state.status.operation.map(|o| o.label());
        let button = |id: &'static str, icon: IconName, tip: &'static str| {
            IconButton::new(id, icon)
                .icon_size(IconSize::Small)
                .tooltip(Tooltip::text(tip))
        };
        let s = self.state.clone();
        let header = h_flex()
            .px_2()
            .h(px(32.))
            .gap_1()
            .border_b_1()
            .border_color(colors.border)
            .child(
                Icon::new(IconName::GitBranch)
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_sm()
                    .child(branch.unwrap_or_else(|| "Git".to_string())),
            )
            .when(!not_a_repo && root.is_some(), |d| {
                let (fetch, pull, push, refresh) = (s.clone(), s.clone(), s.clone(), s.clone());
                d.child(
                    button("git-fetch", IconName::ArrowCircle, "Fetch")
                        .on_click(move |_, _, cx| fetch.update(cx, |s, cx| s.fetch(cx))),
                )
                .child(
                    button("git-pull", IconName::ArrowDown, "Pull (fast-forward)")
                        .on_click(move |_, _, cx| pull.update(cx, |s, cx| s.pull(cx))),
                )
                .child(
                    button("git-push", IconName::ArrowUp, "Push")
                        .on_click(move |_, _, cx| push.update(cx, |s, cx| s.push(cx))),
                )
                .child(
                    button("git-refresh", IconName::RotateCw, "Refresh")
                        .on_click(move |_, _, cx| refresh.update(cx, |s, cx| s.refresh(false, cx))),
                )
            })
            .when_some(root.clone(), |d, root| {
                d.child(
                    button(
                        "git-open-ferrumgit",
                        IconName::ArrowUpRight,
                        "Open in FerrumGIT (history, branches, pull requests…)",
                    )
                    .on_click(cx.listener(move |_, _, _, cx| {
                        let message = match open_in_ferrumgit(&root) {
                            Ok(()) => "Opened in FerrumGIT".to_string(),
                            Err(error) => error,
                        };
                        cx.emit(GitPanelEvent::Notice(message));
                    })),
                )
            });
        let body = if root.is_none() {
            div()
                .p_3()
                .text_sm()
                .text_color(colors.text_muted)
                .child("Open a folder to see its changes.")
                .into_any_element()
        } else if not_a_repo {
            div()
                .p_3()
                .text_sm()
                .text_color(colors.text_muted)
                .child("This folder is not a Git repository. Run `git init` in the terminal, or clone one with FerrumGIT.")
                .into_any_element()
        } else if !loaded {
            div()
                .p_3()
                .text_sm()
                .text_color(colors.text_muted)
                .child("Reading the repository…")
                .into_any_element()
        } else {
            let commit_state = self.state.clone();
            let commit_box = v_flex().p_2().gap_1p5().child(message).child(
                Button::new(
                    "git-commit",
                    if staged_count == 0 && has_changes {
                        "Commit All"
                    } else {
                        "Commit"
                    },
                )
                .style(ButtonStyle::Filled)
                .full_width()
                .disabled(!has_changes || busy.is_some())
                .on_click(move |_, _, cx| commit_state.update(cx, |s, cx| s.commit(cx))),
            );
            let (staged, unstaged): (Vec<FileStatus>, Vec<FileStatus>) = {
                let status = &self.state.read(cx).status;
                (
                    status.staged().into_iter().cloned().collect(),
                    status.unstaged().into_iter().cloned().collect(),
                )
            };
            let mut list = v_flex()
                .id("git-files")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.scroll);
            if !staged.is_empty() {
                list = list.child(self.section("Staged", staged.iter().collect(), true, cx));
            }
            list = list.child(self.section("Changes", unstaged.iter().collect(), false, cx));
            if !has_changes {
                list = list.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_sm()
                        .text_color(colors.text_muted)
                        .child("No changes."),
                );
            }
            v_flex()
                .flex_1()
                .min_h_0()
                .when_some(operation, |d, op| {
                    d.child(
                        div()
                            .mx_2()
                            .mt_2()
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .bg(colors.element_hover)
                            .text_xs()
                            .child(format!("{op} in progress: resolve conflicts, then commit. Use FerrumGIT to continue or abort.")),
                    )
                })
                .child(commit_box)
                .child(list)
                .into_any_element()
        };
        v_flex()
            .size_full()
            .bg(colors.panel_background)
            .child(header)
            .when_some(busy, |d, busy| {
                d.child(
                    div()
                        .px_3()
                        .py_1()
                        .text_xs()
                        .text_color(colors.text_muted)
                        .child(busy),
                )
            })
            .child(body)
    }
}
