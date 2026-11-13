//! Quick Open (switch repository) and the branch picker for Merge / Rebase.

use super::{DialogState, modal};
use crate::app::FerrumGit;
use crate::components::icons::Icon;
use ferrum_ui::TextInput;
use ferrum_ui::components::{Color, IconSize, Label, LabelCommon, LabelSize, h_flex, v_flex};
use ferrum_ui::theme::ActiveTheme;
use ferrumgit_domain::{GitRef, RefKind, RepoEntry, RepoOp};
use gpui::{AnyElement, Context, Entity, SharedString, Window, div, prelude::*, px};

pub struct Picker {
    pub query: Entity<TextInput>,
    pub cursor: usize,
}

impl Picker {
    pub fn new(
        app: &mut FerrumGit,
        placeholder: &str,
        window: &mut Window,
        cx: &mut Context<FerrumGit>,
    ) -> Self {
        let query = app.input(placeholder, window, cx);
        let subscription =
            cx.subscribe_in(&query, window, |app, _, event, window, cx| match event {
                ferrum_ui::TextInputEvent::Changed => {
                    if let Some(
                        DialogState::QuickOpen(p) | DialogState::PickBranch { picker: p, .. },
                    ) = &mut app.dialog
                    {
                        p.cursor = 0;
                    }
                    cx.notify();
                }
                ferrum_ui::TextInputEvent::Submit => app.submit_dialog(window, cx),
                ferrum_ui::TextInputEvent::Blurred => {}
            });
        app.dialog_subscriptions.push(subscription);
        Self { query, cursor: 0 }
    }
}

fn quick_open_matches(app: &FerrumGit, cx: &Context<FerrumGit>) -> Vec<RepoEntry> {
    let Some(DialogState::QuickOpen(picker)) = &app.dialog else {
        return Vec::new();
    };
    let q = picker.query.read(cx).text().to_lowercase();
    app.state
        .repos
        .iter()
        .filter(|r| {
            q.is_empty() || r.name.to_lowercase().contains(&q) || r.path.to_lowercase().contains(&q)
        })
        .cloned()
        .collect()
}

fn branch_matches(app: &FerrumGit, cx: &Context<FerrumGit>) -> Vec<GitRef> {
    let (Some(DialogState::PickBranch { picker, .. }), Some(id)) = (&app.dialog, app.state.active)
    else {
        return Vec::new();
    };
    let q = picker.query.read(cx).text().to_lowercase();
    app.state
        .view(id)
        .data
        .as_ref()
        .map(|d| {
            d.snapshot
                .refs
                .iter()
                .filter(|r| {
                    (r.is(RefKind::LocalBranch) || r.is(RefKind::RemoteBranch)) && !r.is_head
                })
                .filter(|r| q.is_empty() || r.name.to_lowercase().contains(&q))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

impl FerrumGit {
    fn picker_cursor(&self) -> usize {
        match &self.dialog {
            Some(DialogState::QuickOpen(p) | DialogState::PickBranch { picker: p, .. }) => p.cursor,
            _ => 0,
        }
    }

    fn move_cursor(&mut self, delta: isize, len: usize, cx: &mut Context<Self>) {
        if let Some(DialogState::QuickOpen(p) | DialogState::PickBranch { picker: p, .. }) =
            &mut self.dialog
        {
            let max = len.saturating_sub(1);
            p.cursor = if delta < 0 {
                p.cursor.saturating_sub(1)
            } else {
                (p.cursor + 1).min(max)
            };
            cx.notify();
        }
    }

    /// Opens the repository at `index` (or the highlighted one).
    pub fn quick_open_pick(
        &mut self,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let repos = quick_open_matches(self, cx);
        let index =
            index.unwrap_or_else(|| self.picker_cursor().min(repos.len().saturating_sub(1)));
        if let Some(r) = repos.get(index) {
            let id = r.id;
            self.close_dialog(window, cx);
            self.activate(Some(id), cx);
        }
    }

    /// Merges / rebases onto the branch at `index` (or the highlighted one).
    pub fn pick_branch(
        &mut self,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(DialogState::PickBranch { rebase, .. }) = &self.dialog else {
            return;
        };
        let rebase = *rebase;
        let branches = branch_matches(self, cx);
        let index =
            index.unwrap_or_else(|| self.picker_cursor().min(branches.len().saturating_sub(1)));
        let (Some(b), Some(id)) = (branches.get(index), self.state.active) else {
            return;
        };
        let op = if rebase {
            RepoOp::Rebase {
                onto: b.name.clone(),
            }
        } else {
            RepoOp::Merge {
                branch: b.name.clone(),
                no_ff: false,
            }
        };
        self.close_dialog(window, cx);
        self.op(id, op, cx);
    }
}

fn row(
    id: SharedString,
    active: bool,
    icon: Icon,
    title: String,
    trailing: Option<String>,
    cx: &Context<FerrumGit>,
) -> gpui::Stateful<gpui::Div> {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    h_flex()
        .id(id)
        .w_full()
        .gap_2()
        .px_2p5()
        .py_1p5()
        .rounded_sm()
        .cursor_pointer()
        .when(active, |d| d.bg(colors.element_selected))
        .when(!active, |d| d.hover(|s| s.bg(colors.element_hover)))
        .child(icon.icon().size(IconSize::Small).color(Color::Muted))
        .child(Label::new(title).truncate())
        .children(trailing.map(|t| {
            div().flex_1().min_w_0().flex().justify_end().child(
                Label::new(t)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
                    .truncate(),
            )
        }))
}

fn list_box(rows: Vec<AnyElement>, empty: &'static str, cx: &Context<FerrumGit>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    v_flex()
        .id("picker-list")
        .max_h(px(320.))
        .overflow_y_scroll()
        .p_1()
        .rounded_md()
        .border_1()
        .border_color(colors.border)
        .when(rows.is_empty(), |d| {
            d.child(
                div()
                    .p_3()
                    .flex()
                    .justify_center()
                    .child(Label::new(empty).size(LabelSize::Small).color(Color::Muted)),
            )
        })
        .children(rows)
        .into_any_element()
}

pub fn render_quick_open(app: &FerrumGit, cx: &mut Context<FerrumGit>) -> AnyElement {
    let Some(DialogState::QuickOpen(picker)) = &app.dialog else {
        return div().into_any_element();
    };
    let query = picker.query.clone();
    let repos = quick_open_matches(app, cx);
    let len = repos.len();
    let cursor = picker.cursor.min(len.saturating_sub(1));
    let rows: Vec<AnyElement> = repos
        .into_iter()
        .enumerate()
        .map(|(i, r)| {
            row(
                SharedString::from(format!("qo-{}", r.id)),
                i == cursor,
                Icon::Folder,
                r.name.clone(),
                Some(r.path.clone()),
                cx,
            )
            .on_click(
                cx.listener(move |app, _, window, cx| app.quick_open_pick(Some(i), window, cx)),
            )
            .into_any_element()
        })
        .collect();
    modal("Quick Open", cx)
        .child(
            v_flex()
                .key_context("FerrumGitPicker")
                .on_action(
                    cx.listener(move |app, _: &ferrum_ui::menu::SelectNext, _, cx| {
                        app.move_cursor(1, len, cx)
                    }),
                )
                .on_action(
                    cx.listener(move |app, _: &ferrum_ui::menu::SelectPrevious, _, cx| {
                        app.move_cursor(-1, len, cx)
                    }),
                )
                .gap_3()
                .child(query)
                .child(list_box(rows, "No repositories match.", cx)),
        )
        .into_any_element()
}

pub fn render_pick_branch(app: &FerrumGit, cx: &mut Context<FerrumGit>) -> AnyElement {
    let Some(DialogState::PickBranch { rebase, picker }) = &app.dialog else {
        return div().into_any_element();
    };
    let Some(id) = app.state.active else {
        return div().into_any_element();
    };
    let current = app
        .state
        .view(id)
        .data
        .as_ref()
        .and_then(|d| d.snapshot.status.branch.head.clone())
        .unwrap_or_default();
    let title = if *rebase {
        format!("Rebase “{current}” onto…")
    } else {
        format!("Merge into “{current}”…")
    };
    let query = picker.query.clone();
    let branches = branch_matches(app, cx);
    let len = branches.len();
    let cursor = picker.cursor.min(len.saturating_sub(1));
    let rows: Vec<AnyElement> = branches
        .into_iter()
        .enumerate()
        .map(|(i, b)| {
            let icon = if b.is(RefKind::RemoteBranch) {
                Icon::Cloud
            } else {
                Icon::Branch
            };
            row(
                SharedString::from(format!("pb-{}", b.full_name)),
                i == cursor,
                icon,
                b.name.clone(),
                b.merged.then(|| "merged".to_owned()),
                cx,
            )
            .on_click(cx.listener(move |app, _, window, cx| app.pick_branch(Some(i), window, cx)))
            .into_any_element()
        })
        .collect();
    modal(title, cx)
        .child(
            v_flex()
                .key_context("FerrumGitPicker")
                .on_action(
                    cx.listener(move |app, _: &ferrum_ui::menu::SelectNext, _, cx| {
                        app.move_cursor(1, len, cx)
                    }),
                )
                .on_action(
                    cx.listener(move |app, _: &ferrum_ui::menu::SelectPrevious, _, cx| {
                        app.move_cursor(-1, len, cx)
                    }),
                )
                .gap_3()
                .child(query)
                .child(list_box(rows, "No other branches.", cx)),
        )
        .into_any_element()
}
