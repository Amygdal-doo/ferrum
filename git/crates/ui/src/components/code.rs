//! Hunks of a file diff with syntax highlighting, visible whitespace and —
//! for working-copy diffs — hunk / line staging.

use crate::app::GitView;
use crate::commands::repo::HunkAction;
use crate::components::highlight::{Token, highlight};
use crate::components::split::split_side;
use crate::components::widgets::small_button;
use ferrum_ui::components::{
    Clickable, Color, Disableable, Label, LabelCommon, LabelSize, StyledTypography, h_flex, v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumgit_domain::{DiffLine, DiffSource, FileDiff, Hunk, LineKind, RepoId};
use gpui::{
    AnyElement, Context, HighlightStyle, Hsla, SharedString, StyledText, div, prelude::*, px, rgb,
};
use std::ops::Range;
use std::sync::Arc;

/// Lines rendered per file at most.
pub const MAX_LINES: usize = 4000;

/// Line text with `·` for spaces and `→` for tabs (muted) when `invisibles`,
/// plus the syntax colour runs over it.
pub(crate) fn line_text(
    toks: &[Token],
    invisibles: bool,
    muted: Hsla,
) -> (String, Vec<(Range<usize>, HighlightStyle)>) {
    let mut text = String::new();
    let mut highlights = Vec::new();
    let colored = |color: Hsla| HighlightStyle {
        color: Some(color),
        ..Default::default()
    };
    for t in toks {
        let color = t.color.map(|c| Hsla::from(rgb(c)));
        if invisibles && t.text.contains([' ', '\t']) {
            for ch in t.text.chars() {
                let start = text.len();
                match ch {
                    ' ' => {
                        text.push('·');
                        highlights.push((start..text.len(), colored(muted)));
                    }
                    '\t' => {
                        text.push_str("→   ");
                        highlights.push((start..text.len(), colored(muted)));
                    }
                    other => {
                        text.push(other);
                        if let Some(c) = color {
                            highlights.push((start..text.len(), colored(c)));
                        }
                    }
                }
            }
        } else {
            let start = text.len();
            text.push_str(&t.text);
            if let Some(c) = color {
                highlights.push((start..text.len(), colored(c)));
            }
        }
    }
    if invisibles {
        let start = text.len();
        text.push('¬');
        highlights.push((start..text.len(), colored(muted)));
    }
    (text, highlights)
}

/// All hunks of `file`. `repo` + an editable source enables staging actions.
pub fn code_diff(
    app: &GitView,
    file: Arc<FileDiff>,
    source: DiffSource,
    repo: Option<RepoId>,
    cx: &mut Context<GitView>,
) -> AnyElement {
    let invisibles = !app.state.settings.hide_invisibles;
    let note = |text: &'static str| {
        div()
            .px_4()
            .py_3()
            .child(Label::new(text).size(LabelSize::Small).color(Color::Muted))
            .into_any_element()
    };
    if file.binary {
        return note("Binary file — no text diff.");
    }
    if file.hunks.is_empty() {
        return note("No textual changes (mode change, rename or empty file).");
    }
    let mut budget = MAX_LINES;
    let mut hunks: Vec<AnyElement> = Vec::new();
    for (i, h) in file.hunks.iter().enumerate() {
        let shown = h.lines.len().min(budget);
        budget -= shown;
        if shown > 0 {
            hunks.push(hunk_view(
                app, &file.path, i, h, shown, &source, repo, invisibles, cx,
            ));
        }
    }
    let hidden = file
        .hunks
        .iter()
        .map(|h| h.lines.len())
        .sum::<usize>()
        .saturating_sub(MAX_LINES);
    v_flex()
        .id(SharedString::from(format!("code-{}-{source:?}", file.path)))
        .w_full()
        .overflow_x_scroll()
        .font_buffer(cx)
        .text_buffer(cx)
        .children(hunks)
        .when(hidden > 0, |d| {
            d.child(
                div().px_4().py_2().child(
                    Label::new(format!("{hidden} more lines not shown."))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
            )
        })
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn hunk_view(
    app: &GitView,
    path: &str,
    index: usize,
    hunk: &Hunk,
    shown: usize,
    source: &DiffSource,
    repo: Option<RepoId>,
    invisibles: bool,
    cx: &mut Context<GitView>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let editable = repo.is_some() && matches!(source, DiffSource::Unstaged | DiffSource::Staged);
    let selected: Vec<usize> = repo
        .and_then(|id| app.state.view(id).diff)
        .map(|d| {
            d.lines
                .iter()
                .filter(|(h, _)| *h == index)
                .map(|(_, l)| *l)
                .collect()
        })
        .unwrap_or_default();
    let has_sel = !selected.is_empty();
    let busy = repo.is_some_and(|id| app.state.is_busy(id));
    let texts: Vec<&str> = hunk
        .lines
        .iter()
        .take(shown)
        .map(|l| l.content.as_str())
        .collect();
    let toks = highlight(path, &texts, true);
    let what = if has_sel { "Lines" } else { "Hunk" };
    let act = move |action: HunkAction| {
        move |app: &mut GitView,
              _: &gpui::ClickEvent,
              _: &mut gpui::Window,
              cx: &mut Context<GitView>| {
            if let Some(id) = repo {
                app.hunk(id, index, action, has_sel, cx);
            }
        }
    };
    let id_base = format!("{path}-{index}");
    let mut buttons: Vec<AnyElement> = Vec::new();
    if editable {
        if has_sel {
            buttons.push(
                Label::new(format!("{} selected", selected.len()))
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
                    .into_any_element(),
            );
        }
        if *source == DiffSource::Unstaged {
            buttons.push(
                small_button(
                    SharedString::from(format!("{id_base}-discard")),
                    format!("Discard {what}"),
                )
                .color(Color::Error)
                .disabled(busy)
                .on_click(cx.listener(act(HunkAction::Discard)))
                .into_any_element(),
            );
            buttons.push(
                small_button(
                    SharedString::from(format!("{id_base}-stage")),
                    format!("Stage {what}"),
                )
                .disabled(busy)
                .on_click(cx.listener(act(HunkAction::Stage)))
                .into_any_element(),
            );
        } else {
            buttons.push(
                small_button(
                    SharedString::from(format!("{id_base}-unstage")),
                    format!("Unstage {what}"),
                )
                .disabled(busy)
                .on_click(cx.listener(act(HunkAction::Unstage)))
                .into_any_element(),
            );
        }
    }
    let tokens_of = |li: usize| toks.get(li).map(Vec::as_slice).unwrap_or_default();
    let lines: Vec<AnyElement> = if app.state.settings.side_by_side {
        hunk.split_rows()
            .into_iter()
            .filter(|r| r.left.into_iter().chain(r.right).all(|li| li < shown))
            .enumerate()
            .map(|(ri, row)| {
                let mut side = |li: Option<usize>, left: bool| {
                    let line = li.and_then(|li| hunk.lines.get(li).map(|l| (li, l)));
                    split_side(
                        repo,
                        index,
                        line,
                        line.map(|(li, _)| tokens_of(li)).unwrap_or_default(),
                        li.is_some_and(|li| selected.contains(&li)),
                        editable,
                        invisibles,
                        left,
                        &format!("{id_base}-row-{ri}"),
                        &theme,
                        cx,
                    )
                };
                let (l, r) = (side(row.left, true), side(row.right, false));
                h_flex()
                    .w_full()
                    .items_stretch()
                    .child(l)
                    .child(r)
                    .into_any_element()
            })
            .collect()
    } else {
        hunk.lines
            .iter()
            .take(shown)
            .enumerate()
            .map(|(li, line)| {
                line_view(
                    repo,
                    index,
                    li,
                    line,
                    tokens_of(li),
                    selected.contains(&li),
                    editable,
                    invisibles,
                    &id_base,
                    &theme,
                    cx,
                )
            })
            .collect()
    };
    v_flex()
        .border_t_1()
        .border_color(colors.border_variant)
        .child(
            h_flex()
                .h(px(32.))
                .gap_2()
                .px_4()
                .bg(colors.element_background)
                .text_ui_xs(cx)
                .text_color(colors.text_muted)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(hunk.header.clone()),
                )
                .children(buttons),
        )
        .children(lines)
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn line_view(
    repo: Option<RepoId>,
    hunk: usize,
    index: usize,
    line: &DiffLine,
    toks: &[Token],
    selected: bool,
    selectable: bool,
    invisibles: bool,
    id_base: &str,
    theme: &ferrum_ui::theme::Theme,
    cx: &mut Context<GitView>,
) -> AnyElement {
    let colors = theme.colors();
    let status = theme.status();
    let (tone, gutter_bg, gutter_fg, sign): (Option<Hsla>, Option<Hsla>, Hsla, &str) =
        match line.kind {
            LineKind::Added => (
                Some(status.created.opacity(0.08)),
                Some(status.created.opacity(0.14)),
                status.created,
                "+",
            ),
            LineKind::Removed => (
                Some(status.deleted.opacity(0.08)),
                Some(status.deleted.opacity(0.14)),
                status.deleted,
                "-",
            ),
            LineKind::Context => (None, None, colors.text_muted.opacity(0.6), " "),
            LineKind::NoNewline => (None, None, colors.text_muted.opacity(0.6), ""),
        };
    let changed = matches!(line.kind, LineKind::Added | LineKind::Removed);
    let old = line.old_no.map(|n| n.to_string()).unwrap_or_default();
    let new = line
        .new_no
        .map(|n| format!("{sign}{n}"))
        .unwrap_or_default();
    let is_meta = line.kind == LineKind::NoNewline;
    let muted = colors.text_muted.opacity(0.35);
    let content: AnyElement = if is_meta {
        div()
            .italic()
            .text_color(colors.text_muted)
            .child(line.content.clone())
            .into_any_element()
    } else {
        let (text, highlights) = line_text(toks, invisibles, muted);
        StyledText::new(text)
            .with_highlights(highlights)
            .into_any_element()
    };
    let clickable = repo.is_some() && selectable && changed;
    h_flex()
        .id(SharedString::from(format!("{id_base}-line-{index}")))
        .min_w_full()
        .flex_none()
        .whitespace_nowrap()
        .when_some(tone, |d, t| d.bg(t))
        .when(selected, |d| {
            d.bg(status.info_background)
                .border_1()
                .border_color(status.info)
        })
        .when(clickable, |d| {
            d.cursor_pointer()
                .on_click(cx.listener(move |app, _, _, cx| toggle_line(app, repo, hunk, index, cx)))
        })
        .child(
            div()
                .flex_none()
                .w(px(48.))
                .pr_2()
                .flex()
                .justify_end()
                .when_some(gutter_bg, |d, b| d.bg(b))
                .text_color(gutter_fg)
                .child(old),
        )
        .child(
            div()
                .flex_none()
                .w(px(56.))
                .pr_2()
                .flex()
                .justify_end()
                .border_r_1()
                .border_color(colors.border_variant)
                .when_some(gutter_bg, |d, b| d.bg(b))
                .text_color(gutter_fg)
                .child(new),
        )
        .child(div().pl_3().pr_6().child(content))
        .into_any_element()
}

/// Adds / removes a changed line from the line-staging selection.
pub(crate) fn toggle_line(
    app: &mut GitView,
    repo: Option<RepoId>,
    hunk: usize,
    index: usize,
    cx: &mut Context<GitView>,
) {
    if let Some(id) = repo {
        app.state.update_view(id, |v| {
            if let Some(d) = v.diff.as_mut()
                && !d.lines.remove(&(hunk, index))
            {
                d.lines.insert((hunk, index));
            }
        });
        cx.notify();
    }
}
