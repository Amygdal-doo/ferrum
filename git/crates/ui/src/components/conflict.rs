//! Three columns for a conflicted file — ours, theirs and the result — with
//! a side to take per conflict, the editor for anything else, and
//! "Mark resolved". Works the same during a merge, rebase, cherry-pick or
//! revert (only the column names change).

use crate::app::GitView;
use crate::components::widgets::{ghost_button, primary_button, small_button};
use crate::state::ConflictView;
use ferrum_ui::components::{
    ButtonCommon, Clickable, Color, Disableable, Label, LabelCommon, LabelSize, StyledTypography,
    Tooltip, h_flex, v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumgit_domain::{ConflictChoice, ConflictSegment, RepoId, RepoOperation};
use gpui::{AnyElement, Context, Hsla, SharedString, div, prelude::*, px};

/// Unchanged stretches longer than this show only their ends.
const FOLD: usize = 8;

/// Column titles: what "ours" and "theirs" mean in this operation.
#[must_use]
pub fn side_names(op: Option<RepoOperation>) -> (&'static str, &'static str) {
    match op {
        Some(RepoOperation::Merge) => ("Ours (HEAD)", "Theirs (merged branch)"),
        Some(RepoOperation::Rebase) => ("Ours (rebasing onto)", "Theirs (your commit)"),
        Some(RepoOperation::CherryPick) => ("Ours (HEAD)", "Theirs (picked commit)"),
        Some(RepoOperation::Revert) => ("Ours (HEAD)", "Theirs (reverting)"),
        None => ("Ours", "Theirs"),
    }
}

fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_owned).collect()
}

/// Monospace lines in a column.
fn code(text: &str, empty: &'static str, cx: &mut Context<GitView>) -> AnyElement {
    let rows = lines(text);
    if rows.is_empty() {
        return div()
            .px_2()
            .py_1()
            .child(
                Label::new(empty)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
            .into_any_element();
    }
    v_flex()
        .px_2()
        .py_1()
        .min_w_0()
        .overflow_hidden()
        .font_buffer(cx)
        .text_buffer(cx)
        .children(rows.into_iter().map(|l| {
            div()
                .whitespace_nowrap()
                .child(if l.is_empty() { " ".into() } else { l })
        }))
        .into_any_element()
}

fn common_block(index: usize, text: &str, cx: &mut Context<GitView>) -> AnyElement {
    let colors = cx.theme().colors();
    let all = lines(text);
    let shown = if all.len() > FOLD {
        let hidden = all.len() - 6;
        let mut v = all[..3].to_vec();
        v.push(format!("⋯ {hidden} unchanged lines ⋯"));
        v.extend_from_slice(&all[all.len() - 3..]);
        v.join("\n")
    } else {
        all.join("\n")
    };
    div()
        .id(("conflict-common", index))
        .text_color(colors.text_muted)
        .child(code(&shown, "", cx))
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn side_column(
    id: RepoId,
    n: usize,
    text: &str,
    label: &'static str,
    choice: ConflictChoice,
    chosen: bool,
    tint: Hsla,
    cx: &mut Context<GitView>,
) -> AnyElement {
    let colors = cx.theme().colors();
    v_flex()
        .flex_1()
        .min_w_0()
        .rounded_md()
        .border_1()
        .border_color(if chosen {
            colors.text_accent
        } else {
            colors.border_variant
        })
        .bg(tint)
        .child(code(text, "(no lines)", cx))
        .child(
            h_flex().p_1().justify_end().child(
                small_button(SharedString::from(format!("conflict-{n}-{label}")), label)
                    .disabled(chosen)
                    .on_click(cx.listener(move |app, _, _, cx| app.choose(id, n, choice, cx))),
            ),
        )
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn conflict_row(
    id: RepoId,
    n: usize,
    ours: &str,
    base: Option<&str>,
    theirs: &str,
    choice: ConflictChoice,
    cx: &mut Context<GitView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let status = theme.status();
    let result = match choice {
        ConflictChoice::Unresolved => None,
        ConflictChoice::Ours => Some(ours.to_owned()),
        ConflictChoice::Theirs => Some(theirs.to_owned()),
        ConflictChoice::Both => Some(format!("{ours}{theirs}")),
        ConflictChoice::BothTheirsFirst => Some(format!("{theirs}{ours}")),
        ConflictChoice::Base => Some(base.unwrap_or_default().to_owned()),
    };
    let content = match &result {
        Some(text) => code(text, "(nothing: the lines are removed)", cx),
        None => div()
            .px_2()
            .py_1()
            .child(
                Label::new("Unresolved: take a side, or both")
                    .size(LabelSize::XSmall)
                    .color(Color::Warning),
            )
            .into_any_element(),
    };
    let button = |key: &str, label: &'static str, tip: &'static str, pick: ConflictChoice| {
        ghost_button(SharedString::from(format!("conflict-{n}-{key}")), label)
            .tooltip(Tooltip::text(tip))
            .disabled(choice == pick)
            .on_click(cx.listener(move |app, _, _, cx| app.choose(id, n, pick, cx)))
    };
    let result_column = v_flex()
        .flex_1()
        .min_w_0()
        .rounded_md()
        .border_1()
        .border_color(if result.is_some() {
            colors.border_variant
        } else {
            status.warning
        })
        .child(content)
        .child(
            h_flex()
                .p_1()
                .gap_0p5()
                .justify_end()
                .flex_wrap()
                .child(button(
                    "both",
                    "Both",
                    "Our lines, then theirs",
                    ConflictChoice::Both,
                ))
                .child(button(
                    "both-rev",
                    "Both ↑↓",
                    "Their lines, then ours",
                    ConflictChoice::BothTheirsFirst,
                ))
                .when(base.is_some(), |d| {
                    d.child(button(
                        "base",
                        "Base",
                        "The common ancestor's lines (neither change)",
                        ConflictChoice::Base,
                    ))
                })
                .when(result.is_some(), |d| {
                    d.child(button(
                        "reset",
                        "Reset",
                        "Undo this choice",
                        ConflictChoice::Unresolved,
                    ))
                }),
        );
    h_flex()
        .id(("conflict", n))
        .items_start()
        .gap_2()
        .py_1()
        .child(side_column(
            id,
            n,
            ours,
            "Use ours",
            ConflictChoice::Ours,
            choice == ConflictChoice::Ours,
            status.created_background,
            cx,
        ))
        .child(side_column(
            id,
            n,
            theirs,
            "Use theirs",
            ConflictChoice::Theirs,
            choice == ConflictChoice::Theirs,
            status.info_background,
            cx,
        ))
        .child(result_column)
        .into_any_element()
}

pub fn conflict_pane(
    app: &GitView,
    id: RepoId,
    c: ConflictView,
    cx: &mut Context<GitView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let busy = app.state.is_busy(id);
    let op = app
        .state
        .view(id)
        .data
        .and_then(|d| d.snapshot.status.operation);
    let (ours_name, theirs_name) = side_names(op);
    let total = c.choices.len();
    let done = c.resolved();
    let complete = done == total;
    let edit_label = if app.options.open_files_in_host {
        "Open in editor"
    } else {
        "Edit result"
    };

    let mut n = 0;
    let mut rows: Vec<AnyElement> = Vec::new();
    for (i, segment) in c.file.segments.iter().enumerate() {
        match segment {
            ConflictSegment::Common(text) => rows.push(common_block(i, text, cx)),
            ConflictSegment::Conflict { ours, base, theirs } => {
                let choice = c.choices.get(n).copied().unwrap_or_default();
                rows.push(conflict_row(
                    id,
                    n,
                    ours,
                    base.as_deref(),
                    theirs,
                    choice,
                    cx,
                ));
                n += 1;
            }
        }
    }
    let column_title = |text: &'static str| {
        div().flex_1().min_w_0().child(
            Label::new(text)
                .size(LabelSize::XSmall)
                .color(Color::Muted)
                .truncate(),
        )
    };

    v_flex()
        .size_full()
        .child(
            h_flex()
                .flex_none()
                .h(px(36.))
                .gap_2()
                .px_4()
                .border_b_1()
                .border_color(colors.border)
                .text_ui_sm(cx)
                .child(div().text_color(theme.status().warning).child("Conflict"))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(c.file.path.clone()),
                )
                .child(
                    Label::new(format!("{done} of {total} resolved"))
                        .size(LabelSize::XSmall)
                        .color(if complete { Color::Success } else { Color::Muted }),
                )
                .child(
                    ghost_button("conflict-edit", edit_label)
                        .tooltip(Tooltip::text(
                            "Writes the choices so far to the file (open conflicts keep their markers) and opens it",
                        ))
                        .disabled(busy)
                        .on_click(cx.listener(move |app, _, window, cx| {
                            app.edit_conflict(id, window, cx);
                        })),
                )
                .child(
                    primary_button("conflict-resolved", "Mark resolved")
                        .disabled(busy || !complete)
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.mark_conflict_resolved(id, cx);
                        })),
                )
                .child(
                    ghost_button("conflict-close", "Close")
                        .on_click(cx.listener(move |app, _, _, cx| app.close_conflict(id, cx))),
                ),
        )
        .child(
            h_flex()
                .flex_none()
                .gap_2()
                .px_4()
                .py_1()
                .border_b_1()
                .border_color(colors.border_variant)
                .child(column_title(ours_name))
                .child(column_title(theirs_name))
                .child(column_title("Result")),
        )
        .child(
            v_flex()
                .id("conflict-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px_4()
                .py_2()
                .children(rows),
        )
        .into_any_element()
}
