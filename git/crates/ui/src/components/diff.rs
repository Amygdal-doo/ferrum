//! Working-copy diff (right panel): file header with file-level actions and
//! the hunks with stage / unstage / discard per hunk or selected lines.

use crate::app::GitView;
use crate::components::change_word;
use crate::components::code::code_diff;
use crate::components::icons::Icon;
use crate::components::split::layout_toggle;
use crate::components::widgets::{ghost_button, primary_button, small_button};
use crate::state::{Dialog, FileTab};
use ferrum_ui::components::{
    ButtonCommon, Clickable, Color, Disableable, IconSize, Label, LabelCommon, LabelSize,
    StyledTypography, Tooltip, h_flex, v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumgit_domain::{DiffSource, RepoOp};
use gpui::{AnyElement, Context, div, prelude::*, px};

pub fn diff_pane(app: &GitView, cx: &mut Context<GitView>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let status = theme.status();
    let Some(id) = app.state.active else {
        return div().into_any_element();
    };
    let Some(d) = app.state.view(id).diff else {
        return div().into_any_element();
    };
    let file = d.file.clone();
    let source = d.source.clone();
    let busy = app.state.is_busy(id);
    let (word, word_color) = change_word(file.change, cx);
    let label = match source {
        DiffSource::Unstaged => "Unstaged",
        DiffSource::Staged => "Staged",
        DiffSource::Untracked => "Untracked",
        DiffSource::Commit => "Commit",
    };
    let shown = match &file.old_path {
        Some(old) => format!("{old} → {}", file.path),
        None => file.path.clone(),
    };
    let (p_discard, p_stage, p_unstage) = (file.path.clone(), file.path.clone(), file.path.clone());
    let actions: Vec<AnyElement> = match source {
        DiffSource::Unstaged | DiffSource::Untracked => vec![
            ghost_button("diff-discard", "Discard file")
                .color(Color::Error)
                .disabled(busy)
                .on_click(cx.listener(move |app, _, window, cx| {
                    app.open_dialog(
                        Dialog::Confirm {
                            title: "Discard changes?".into(),
                            body: format!("All unstaged changes to “{p_discard}” will be lost."),
                            confirm: "Discard file".into(),
                            destructive: true,
                            op: RepoOp::Discard(vec![p_discard.clone()]),
                        },
                        window,
                        cx,
                    );
                }))
                .into_any_element(),
            primary_button("diff-stage", "Stage file")
                .disabled(busy)
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.op(id, RepoOp::Stage(vec![p_stage.clone()]), cx);
                }))
                .into_any_element(),
        ],
        DiffSource::Staged => vec![
            small_button("diff-unstage", "Unstage file")
                .disabled(busy)
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.op(id, RepoOp::Unstage(vec![p_unstage.clone()]), cx);
                }))
                .into_any_element(),
        ],
        DiffSource::Commit => Vec::new(),
    };
    // Untracked files have no history yet; deleted ones cannot be blamed in the working tree.
    let tracked = source != DiffSource::Untracked;
    let blameable = tracked && file.change != ferrumgit_domain::ChangeKind::Deleted;
    let (p_history, p_blame) = (file.path.clone(), file.path.clone());
    let open_line = first_line(&file);
    let p_open = file.path.clone();
    let deleted = file.change == ferrumgit_domain::ChangeKind::Deleted;
    let file_buttons = h_flex()
        .gap_1()
        .when(app.options.open_files_in_host && !deleted, |d| {
            d.child(
                ghost_button("diff-open", "Open")
                    .tooltip(Tooltip::text("Open this file in the editor"))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.open_in_host(id, &p_open, open_line, cx);
                    })),
            )
        })
        .when(tracked, |d| {
            d.child(
                ghost_button("diff-history", "History")
                    .tooltip(Tooltip::text("Commits that changed this file"))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.open_file_view(id, p_history.clone(), None, FileTab::History, cx);
                    })),
            )
        })
        .when(blameable, |d| {
            d.child(
                ghost_button("diff-blame", "Blame")
                    .tooltip(Tooltip::text("Who last changed each line"))
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.open_file_view(id, p_blame.clone(), None, FileTab::Blame, cx);
                    })),
            )
        });
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
                .child(div().text_color(word_color).child(word))
                .child(
                    div()
                        .size(px(16.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .bg(word_color)
                        .text_color(theme.readable_text_on(word_color))
                        .font_buffer(cx)
                        .text_ui_xs(cx)
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(file.change.letter().to_string()),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(shown),
                )
                .child(
                    div()
                        .px_1p5()
                        .rounded_sm()
                        .bg(colors.element_background)
                        .child(
                            Label::new(label)
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        ),
                )
                .child(
                    div()
                        .font_buffer(cx)
                        .text_ui_xs(cx)
                        .text_color(status.created)
                        .child(format!("+{}", file.additions)),
                )
                .child(
                    div()
                        .font_buffer(cx)
                        .text_ui_xs(cx)
                        .text_color(status.deleted)
                        .child(format!("−{}", file.deletions)),
                )
                .child(div().flex_1())
                .child(file_buttons)
                .child(layout_toggle(app, cx))
                .children(actions)
                .child(
                    div()
                        .id("diff-close")
                        .ml_1()
                        .p_1()
                        .rounded_sm()
                        .cursor_pointer()
                        .hover(|s| s.bg(colors.element_hover))
                        .tooltip(Tooltip::text("Close (Esc)"))
                        .child(Icon::Close.icon().size(IconSize::Small).color(Color::Muted))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.state.update_view(id, |v| v.diff = None);
                            cx.notify();
                        })),
                ),
        )
        .child(
            div()
                .id("diff-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(code_diff(app, file, source, Some(id), cx)),
        )
        .into_any_element()
}

/// First changed (else first shown) line of the new file, 1-based.
fn first_line(file: &ferrumgit_domain::FileDiff) -> Option<u32> {
    let lines = || file.hunks.iter().flat_map(|h| &h.lines);
    lines()
        .find(|l| l.kind == ferrumgit_domain::LineKind::Added)
        .or_else(|| lines().find(|l| l.new_no.is_some()))
        .and_then(|l| l.new_no)
        .or_else(|| file.hunks.first().map(|h| h.new_start.max(1)))
}

#[cfg(test)]
mod tests {
    use super::first_line;
    use ferrumgit_domain::{DiffLine, FileDiff, Hunk, LineKind};

    fn line(kind: LineKind, old_no: Option<u32>, new_no: Option<u32>) -> DiffLine {
        DiffLine {
            kind,
            content: String::new(),
            old_no,
            new_no,
        }
    }

    #[test]
    fn opens_at_the_first_added_line() {
        let mut file = FileDiff::default();
        assert_eq!(first_line(&file), None);
        file.hunks.push(Hunk {
            new_start: 10,
            lines: vec![
                line(LineKind::Context, Some(10), Some(10)),
                line(LineKind::Removed, Some(11), None),
                line(LineKind::Added, None, Some(11)),
            ],
            ..Hunk::default()
        });
        assert_eq!(first_line(&file), Some(11));
        // Only removals: the first line still shown.
        file.hunks[0].lines.pop();
        assert_eq!(first_line(&file), Some(10));
        file.hunks[0].lines.clear();
        assert_eq!(first_line(&file), Some(10));
    }
}
