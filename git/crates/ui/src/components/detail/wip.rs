//! Working copy: operation banner, conflicts, unstaged / staged lists, commit box.

use super::files::{FileRow, RowAction, list_header};
use crate::app::GitView;
use crate::components::widgets::{ghost_button, primary_button, small_button};
use crate::state::Dialog;
use ferrum_ui::components::{
    ButtonCommon, Checkbox, Clickable, Color, Disableable, FixedWidth, Label, LabelCommon,
    LabelSize, ToggleState, Tooltip, h_flex, v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumgit_domain::{DiffSource, FileStatus, RepoId, RepoOp, RepoOperation};
use gpui::{AnyElement, Context, ElementId, Window, div, prelude::*, px};

const STAGE: RowAction = RowAction {
    label: "Stage",
    title: "Stage file",
    danger: false,
};
const DISCARD: RowAction = RowAction {
    label: "Discard",
    title: "Discard changes in this file",
    danger: true,
};
const UNSTAGE: RowAction = RowAction {
    label: "Unstage",
    title: "Unstage file",
    danger: false,
};
const OURS: RowAction = RowAction {
    label: "Ours",
    title: "Resolve with our version",
    danger: false,
};
const THEIRS: RowAction = RowAction {
    label: "Theirs",
    title: "Resolve with their version",
    danger: false,
};
const RESOLVED: RowAction = RowAction {
    label: "Resolved",
    title: "Mark as resolved (stage the file as edited)",
    danger: false,
};

fn discard_dialog(paths: Vec<String>) -> Dialog {
    let body = match paths.as_slice() {
        [one] => format!("Changes to “{one}” will be lost. Untracked files are deleted."),
        _ => format!(
            "Changes to {} files will be lost. Untracked files are deleted.",
            paths.len()
        ),
    };
    Dialog::Confirm {
        title: "Discard changes?".into(),
        body,
        confirm: "Discard".into(),
        destructive: true,
        op: RepoOp::Discard(paths),
    }
}

fn header_button(
    id: &'static str,
    label: &'static str,
    danger: bool,
    disabled: bool,
    on_click: impl Fn(&mut GitView, &gpui::ClickEvent, &mut Window, &mut Context<GitView>) + 'static,
    cx: &mut Context<GitView>,
) -> AnyElement {
    ghost_button(id, label)
        .color(if danger { Color::Error } else { Color::Default })
        .disabled(disabled)
        .on_click(cx.listener(on_click))
        .into_any_element()
}

fn placeholder(text: &'static str) -> AnyElement {
    div()
        .px_2()
        .py_3()
        .child(Label::new(text).size(LabelSize::Small).color(Color::Muted))
        .into_any_element()
}

pub fn wip_panel(
    app: &mut GitView,
    id: RepoId,
    _: &mut Window,
    cx: &mut Context<GitView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let status_colors = theme.status();
    let view = app.state.view(id);
    let Some(data) = view.data.clone() else {
        return div().into_any_element();
    };
    let status = data.snapshot.status.clone();
    let conflicted: Vec<FileStatus> = status.conflicted().into_iter().cloned().collect();
    let unstaged: Vec<FileStatus> = status
        .unstaged()
        .into_iter()
        .filter(|f| !f.is_conflicted())
        .cloned()
        .collect();
    let staged: Vec<FileStatus> = status.staged().into_iter().cloned().collect();
    let open_diff = view
        .diff
        .as_ref()
        .map(|d| (d.file.path.clone(), d.source.clone()));
    let is_open = |p: &str, staged: bool| {
        open_diff
            .as_ref()
            .is_some_and(|(path, src)| path == p && (*src == DiffSource::Staged) == staged)
    };
    let open_conflict = view.conflict.as_ref().map(|c| c.file.path.clone());
    let busy = app.state.is_busy(id);
    let unstaged_paths: Vec<String> = unstaged.iter().map(|f| f.path.clone()).collect();

    let mut body: Vec<AnyElement> = Vec::new();
    if !conflicted.is_empty() {
        body.push(list_header("Conflicts", conflicted.len(), Vec::new(), cx));
        let rows: Vec<AnyElement> = conflicted
            .iter()
            .map(|f| {
                let (p1, p2, p3) = (f.path.clone(), f.path.clone(), f.path.clone());
                let path = f.path.clone();
                FileRow::new(
                    format!("c-{}", f.path),
                    f.path.clone(),
                    f.index,
                    open_conflict.as_deref() == Some(f.path.as_str()),
                    move |app, _, _, cx| app.open_conflict(id, path.clone(), cx),
                )
                .action(OURS, move |app, _, _, cx| {
                    app.op(id, RepoOp::TakeOurs(vec![p1.clone()]), cx);
                })
                .action(THEIRS, move |app, _, _, cx| {
                    app.op(id, RepoOp::TakeTheirs(vec![p2.clone()]), cx);
                })
                .action(RESOLVED, move |app, _, _, cx| {
                    app.op(id, RepoOp::Stage(vec![p3.clone()]), cx);
                })
                .edit_action(app, id, f)
                .render(cx)
            })
            .collect();
        body.push(v_flex().p_1().children(rows).into_any_element());
    }

    let unstaged_buttons = if unstaged.is_empty() {
        Vec::new()
    } else {
        vec![
            header_button(
                "discard-all",
                "Discard all",
                true,
                busy,
                move |app, _, window, cx| {
                    app.open_dialog(discard_dialog(unstaged_paths.clone()), window, cx);
                },
                cx,
            ),
            header_button(
                "stage-all",
                "Stage all",
                false,
                busy,
                move |app, _, _, cx| app.op(id, RepoOp::StageAll, cx),
                cx,
            ),
        ]
    };
    body.push(list_header(
        "Unstaged",
        unstaged.len(),
        unstaged_buttons,
        cx,
    ));
    let mut rows: Vec<AnyElement> = Vec::new();
    if unstaged.is_empty() {
        rows.push(placeholder("No unstaged changes"));
    }
    for f in &unstaged {
        let (p1, p2) = (f.path.clone(), f.path.clone());
        let file = f.clone();
        rows.push(
            FileRow::new(
                format!("u-{}", f.path),
                f.path.clone(),
                f.worktree,
                is_open(&f.path, false),
                move |app, _, _, cx| app.open_file(id, &file, false, cx),
            )
            .action(DISCARD, move |app, _, window, cx| {
                app.open_dialog(discard_dialog(vec![p1.clone()]), window, cx);
            })
            .action(STAGE, move |app, _, _, cx| {
                app.op(id, RepoOp::Stage(vec![p2.clone()]), cx);
            })
            .edit_action(app, id, f)
            .render(cx),
        );
    }
    body.push(v_flex().p_1().children(rows).into_any_element());

    let staged_buttons = if staged.is_empty() {
        Vec::new()
    } else {
        vec![header_button(
            "unstage-all",
            "Unstage all",
            false,
            busy,
            move |app, _, _, cx| app.op(id, RepoOp::UnstageAll, cx),
            cx,
        )]
    };
    body.push(list_header("Staged", staged.len(), staged_buttons, cx));
    let mut rows: Vec<AnyElement> = Vec::new();
    if staged.is_empty() {
        rows.push(placeholder("Stage files or hunks to commit them"));
    }
    for f in &staged {
        let p = f.path.clone();
        let file = f.clone();
        rows.push(
            FileRow::new(
                format!("s-{}", f.path),
                f.path.clone(),
                f.index,
                is_open(&f.path, true),
                move |app, _, _, cx| app.open_file(id, &file, true, cx),
            )
            .action(UNSTAGE, move |app, _, _, cx| {
                app.op(id, RepoOp::Unstage(vec![p.clone()]), cx);
            })
            .edit_action(app, id, f)
            .render(cx),
        );
    }
    body.push(v_flex().p_1().children(rows).into_any_element());

    let banner = status.operation.map(|op| {
        let progress = status.rebase.as_ref().map_or_else(String::new, |p| {
            let step = if p.total > 0 {
                format!(" (step {} of {})", p.step.min(p.total), p.total)
            } else {
                String::new()
            };
            match (&p.current, p.editing) {
                (Some(line), true) => format!("{step}, stopped to edit “{}”", short_todo(line)),
                (Some(line), false) => format!("{step} at “{}”", short_todo(line)),
                (None, _) => step,
            }
        });
        let text = if conflicted.is_empty() {
            format!("{} in progress{progress} — ready to continue", op.label())
        } else {
            format!(
                "{} in progress{progress} — resolve {} conflict(s)",
                op.label(),
                conflicted.len()
            )
        };
        let can_skip = op != RepoOperation::Merge;
        h_flex()
            .flex_none()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(colors.border)
            .bg(status_colors.warning_background)
            .child(
                div().flex_1().child(
                    Label::new(text)
                        .size(LabelSize::Small)
                        .color(Color::Warning),
                ),
            )
            .when(can_skip, |d| {
                d.child(
                    small_button("op-skip", "Skip")
                        .disabled(busy)
                        .tooltip(Tooltip::text("Leave out the commit it stopped at"))
                        .on_click(cx.listener(move |app, _, _, cx| app.op(id, RepoOp::Skip, cx))),
                )
            })
            .child(
                small_button("op-abort", "Abort")
                    .disabled(busy)
                    .on_click(cx.listener(move |app, _, _, cx| app.op(id, RepoOp::Abort, cx))),
            )
            .child(
                primary_button("op-continue", "Continue")
                    .disabled(busy || !conflicted.is_empty())
                    .on_click(cx.listener(move |app, _, _, cx| app.op(id, RepoOp::Continue, cx))),
            )
    });

    v_flex()
        .flex_1()
        .min_h_0()
        .children(banner)
        .child(
            v_flex()
                .id("wip-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(body),
        )
        .child(commit_box(app, id, staged.len(), cx))
        .into_any_element()
}

/// `pick 1a2b3c4d… Subject` → `1a2b3c4 Subject` for the banner.
fn short_todo(line: &str) -> String {
    let mut parts = line.splitn(3, ' ');
    let (_, oid, rest) = (parts.next(), parts.next(), parts.next());
    match oid {
        Some(o) => format!("{} {}", o.get(..7).unwrap_or(o), rest.unwrap_or_default())
            .trim_end()
            .to_owned(),
        None => line.to_owned(),
    }
}

fn commit_box(app: &GitView, id: RepoId, staged: usize, cx: &mut Context<GitView>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let view = app.state.view(id);
    let entry = app.state.entry(id);
    let profile = entry.as_ref().and_then(|e| app.state.profile_of(e));
    let who = profile
        .as_ref()
        .and_then(|p| p.identity().map(|(n, e)| format!("{n} <{e}>")));
    let amend = view.amend;
    let drafting = view.drafting;
    let model_label = app
        .ai_model
        .as_deref()
        .map_or("default model ▾".to_owned(), |k| {
            format!("{} ▾", k.split_once('/').map_or(k, |(_, m)| m))
        });
    let summary_len = view.summary.chars().count();
    let can_commit = app.can_commit(id);
    let label = match (amend, staged) {
        (true, _) => "Amend previous commit".to_owned(),
        (false, 0) => "Stage changes to commit".to_owned(),
        (false, n) => format!("Commit {n} file{}", if n == 1 { "" } else { "s" }),
    };
    let identity = match who {
        Some(w) => format!("as {w} · ⌘↩ to commit"),
        None => "as your git config identity · ⌘↩ to commit".to_owned(),
    };

    v_flex()
        .flex_none()
        .gap_2()
        .p_3()
        .border_t_1()
        .border_color(colors.border)
        .bg(colors.panel_background)
        .child(
            h_flex()
                .gap_2()
                .child(
                    div().flex_1().child(
                        Label::new("Commit")
                            .size(LabelSize::Small)
                            .weight(gpui::FontWeight::SEMIBOLD),
                    ),
                )
                .child(
                    ghost_button(
                        "ai-draft",
                        if drafting {
                            "Drafting…"
                        } else {
                            "Draft with AI"
                        },
                    )
                    .disabled(drafting || staged == 0)
                    .tooltip(Tooltip::text(
                        "Write the message from the staged changes; you edit and commit",
                    ))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.draft_commit_message(id, cx);
                    })),
                )
                .child(
                    div()
                        .id("ai-model")
                        .cursor_pointer()
                        .tooltip(Tooltip::text("Model for AI drafts"))
                        .child(
                            Label::new(model_label)
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        )
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(|app, event: &gpui::MouseDownEvent, _, cx| {
                                app.open_ai_model_menu(event.position, cx);
                            }),
                        ),
                )
                .child(
                    Checkbox::new("amend", ToggleState::from(amend))
                        .label("Amend")
                        .label_size(LabelSize::Small)
                        .on_click(cx.listener(move |app, state: &ToggleState, _, cx| {
                            app.toggle_amend(id, state.selected(), cx);
                        })),
                ),
        )
        .child(
            div().relative().child(app.inputs.summary.clone()).child(
                div().absolute().right(px(8.)).top(px(6.)).child(
                    Label::new(summary_len.to_string())
                        .size(LabelSize::XSmall)
                        .color(if summary_len > 72 {
                            Color::Warning
                        } else {
                            Color::Muted
                        }),
                ),
            ),
        )
        .child(app.inputs.description.clone())
        .child(
            primary_button(ElementId::Name("commit".into()), label)
                .full_width()
                .disabled(!can_commit)
                .on_click(cx.listener(move |app, _, _, cx| {
                    if app.can_commit(id) {
                        app.commit(id, cx);
                    }
                })),
        )
        .child(
            div()
                .id("commit-identity")
                .tooltip(Tooltip::text("Set per repository via its profile"))
                .child(
                    Label::new(identity)
                        .size(LabelSize::XSmall)
                        .color(Color::Muted)
                        .truncate(),
                ),
        )
        .into_any_element()
}
