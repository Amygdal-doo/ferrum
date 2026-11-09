//! File history and blame (right panel). Clicking a commit or an annotated
//! line selects that commit in the graph.

use crate::app::FerrumGit;
use crate::components::code::{MAX_LINES, line_text};
use crate::components::highlight::highlight;
use crate::components::icons::Icon;
use crate::components::widgets::empty;
use crate::components::{avatar_color, change_color, now_secs};
use crate::state::{FileTab, FileView, Selection};
use ferrumgit_domain::text::{full_time, initials, relative_time};
use ferrumgit_domain::{Blame, ChangeKind, FileCommit, RepoId};
use gpui::{
    AnyElement, Context, ListSizingBehavior, SharedString, StyledText, div, prelude::*, px,
    uniform_list,
};
use std::ops::Range;
use std::sync::Arc;
use theme::ActiveTheme;
use ui::{
    Color, IconSize, Label, LabelCommon, LabelSize, StyledTypography, Tooltip, h_flex, v_flex,
};

const HISTORY_ROW_H: f32 = 48.0;
/// Width of the blame annotation column.
const ANNOTATION_W: f32 = 240.0;

pub fn file_panel(
    app: &FerrumGit,
    id: RepoId,
    file: FileView,
    cx: &mut Context<FerrumGit>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let at = file
        .rev
        .as_ref()
        .map_or_else(|| "HEAD".to_owned(), |r| r.short().to_owned());
    let at_hint = match (&file.rev, file.tab) {
        (Some(_), _) => "Revision shown",
        (None, FileTab::Blame) => "Working tree (uncommitted lines included)",
        (None, FileTab::History) => "History up to HEAD",
    };
    let segment = |label: &'static str, tab: FileTab, cx: &mut Context<FerrumGit>| {
        let active = file.tab == tab;
        div()
            .id(label)
            .px_2p5()
            .py_0p5()
            .cursor_pointer()
            .text_ui_sm(cx)
            .when(active, |d| {
                d.bg(colors.text_accent).text_color(colors.background)
            })
            .when(!active, |d| d.hover(|s| s.bg(colors.element_hover)))
            .child(label)
            .on_click(cx.listener(move |app, _, _, cx| app.set_file_tab(id, tab, cx)))
    };
    let header = h_flex()
        .flex_none()
        .h(px(36.))
        .gap_2()
        .px_4()
        .border_b_1()
        .border_color(colors.border)
        .child(Icon::File.icon().size(IconSize::Small).color(Color::Muted))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_ui_sm(cx)
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(file.path.clone()),
        )
        .child(
            div()
                .id("file-at")
                .flex_none()
                .px_1p5()
                .rounded_sm()
                .bg(colors.element_background)
                .tooltip(Tooltip::text(at_hint))
                .child(
                    Label::new(if file.rev.is_none() && file.tab == FileTab::Blame {
                        "Working tree".to_owned()
                    } else {
                        at
                    })
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
                ),
        )
        .child(div().flex_1())
        .child(
            h_flex()
                .flex_none()
                .rounded_md()
                .border_1()
                .border_color(colors.border)
                .overflow_hidden()
                .child(segment("History", FileTab::History, cx))
                .child(segment("Blame", FileTab::Blame, cx)),
        )
        .child(
            div()
                .id("file-close")
                .p_1()
                .rounded_sm()
                .cursor_pointer()
                .hover(|s| s.bg(colors.element_hover))
                .tooltip(Tooltip::text("Close (Esc)"))
                .child(Icon::Close.icon().size(IconSize::Small).color(Color::Muted))
                .on_click(cx.listener(move |app, _, _, cx| app.close_file_view(id, cx))),
        );

    let body = match (&file.error, file.tab) {
        (Some(e), _) => div()
            .p_6()
            .child(Label::new(e.clone()).color(Color::Error))
            .into_any_element(),
        (None, FileTab::History) => match file.history.clone() {
            Some(h) if h.is_empty() => empty("No commits touch this file"),
            Some(h) => history_list(app, id, h, cx),
            None => empty("Loading history…"),
        },
        (None, FileTab::Blame) => match file.blame.clone() {
            Some(b) if b.lines.is_empty() => empty("Empty file"),
            Some(b) => blame_list(id, b, cx),
            None => empty("Loading blame…"),
        },
    };
    v_flex()
        .size_full()
        .child(header)
        .child(v_flex().flex_1().min_h_0().child(body))
        .into_any_element()
}

fn history_list(
    app: &FerrumGit,
    id: RepoId,
    history: Arc<Vec<FileCommit>>,
    cx: &mut Context<FerrumGit>,
) -> AnyElement {
    let selected = match app.state.view(id).selection {
        Selection::Commit(oid) => Some(oid),
        _ => None,
    };
    let count = history.len();
    uniform_list(
        "file-history",
        count,
        cx.processor(move |_, range: Range<usize>, _, cx| {
            range
                .filter_map(|i| history.get(i))
                .map(|f| history_row(id, f, selected.as_ref() == Some(&f.commit.oid), cx))
                .collect::<Vec<_>>()
        }),
    )
    .with_sizing_behavior(ListSizingBehavior::Infer)
    .flex_1()
    .into_any_element()
}

fn history_row(
    id: RepoId,
    f: &FileCommit,
    selected: bool,
    cx: &mut Context<FerrumGit>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let c = &f.commit;
    let (oid, blame_oid, blame_path) = (c.oid.clone(), c.oid.clone(), f.path.clone());
    let group = SharedString::from(format!("fh-{}", c.oid));
    let change = (f.change != ChangeKind::Unmodified).then(|| {
        div()
            .flex_none()
            .w(px(12.))
            .font_buffer(cx)
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(change_color(f.change, cx))
            .child(f.change.letter().to_string())
    });
    let renamed = f.old_path.as_ref().map(|old| format!("{old} → {}", f.path));
    let muted = if selected {
        gpui::white().opacity(0.75)
    } else {
        colors.text_muted
    };
    h_flex()
        .id(SharedString::from(format!("file-commit-{}", c.oid)))
        .group(group.clone())
        .h(px(HISTORY_ROW_H))
        .mx_2()
        .gap_2p5()
        .px_2()
        .rounded_lg()
        .cursor_pointer()
        .when(selected, |d| {
            d.bg(colors.text_accent).text_color(gpui::white())
        })
        .when(!selected, |d| d.hover(|s| s.bg(colors.element_hover)))
        .on_click(cx.listener(move |app, _, _, cx| app.reveal_commit(id, &oid, cx)))
        .child(
            div()
                .flex_none()
                .size(px(26.))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(avatar_color(&c.author_email))
                .text_ui_xs(cx)
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(initials(&c.author_name)),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .text_ui_sm(cx)
                .child(div().min_w_0().truncate().child(c.summary.clone()))
                .child(
                    h_flex()
                        .min_w_0()
                        .gap_2()
                        .text_ui_xs(cx)
                        .text_color(muted)
                        .children(change)
                        .child(
                            div()
                                .flex_none()
                                .font_buffer(cx)
                                .child(c.oid.short().to_owned()),
                        )
                        .child(div().flex_none().child(c.author_name.clone()))
                        .child(
                            div()
                                .id(SharedString::from(format!("fh-date-{}", c.oid)))
                                .flex_none()
                                .tooltip(Tooltip::text(full_time(c.author_time)))
                                .child(relative_time(c.author_time, now_secs())),
                        )
                        .children(renamed.map(|r| div().min_w_0().truncate().child(r))),
                ),
        )
        .child(
            div()
                .id(SharedString::from(format!("fh-blame-{}", c.oid)))
                .flex_none()
                .px_1p5()
                .rounded_sm()
                .border_1()
                .border_color(colors.border)
                .bg(colors.editor_background)
                .text_color(colors.text)
                .invisible()
                .group_hover(group, |s| s.visible())
                .hover(|s| s.bg(colors.element_hover))
                .tooltip(Tooltip::text("Blame the file as of this commit"))
                .child(Label::new("Blame").size(LabelSize::XSmall))
                .on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.open_file_view(
                        id,
                        blame_path.clone(),
                        Some(blame_oid.clone()),
                        FileTab::Blame,
                        cx,
                    );
                })),
        )
        .into_any_element()
}

fn blame_list(id: RepoId, blame: Arc<Blame>, cx: &mut Context<FerrumGit>) -> AnyElement {
    let count = blame.lines.len();
    // Highlight the whole file at once (syntect needs the preceding lines);
    // very long files stay plain.
    let tokens = (count <= MAX_LINES).then(|| {
        let texts: Vec<&str> = blame.lines.iter().map(|l| l.content.as_str()).collect();
        highlight(&blame.path, &texts, true)
    });
    let digits = count.to_string().len().max(3);
    div()
        .id("blame")
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .font_buffer(cx)
        .text_buffer(cx)
        .child(
            uniform_list(
                "blame-lines",
                count,
                cx.processor(move |_, range: Range<usize>, _, cx| {
                    range
                        .map(|i| blame_row(id, &blame, i, tokens.as_deref(), digits, cx))
                        .collect::<Vec<_>>()
                }),
            )
            .with_sizing_behavior(ListSizingBehavior::Infer)
            .flex_1(),
        )
        .into_any_element()
}

fn blame_row(
    id: RepoId,
    blame: &Blame,
    i: usize,
    tokens: Option<&Vec<Vec<crate::components::highlight::Token>>>,
    digits: usize,
    cx: &mut Context<FerrumGit>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let Some(line) = blame.lines.get(i) else {
        return div().into_any_element();
    };
    let start = blame.starts_group(i);
    let commit = blame.commit_of(line);
    let uncommitted = line.oid.is_zero();
    let annotation: AnyElement = match commit.filter(|_| start) {
        Some(c) => {
            let oid = line.oid.clone();
            let title = if uncommitted {
                "Not committed yet".to_owned()
            } else {
                format!(
                    "{} {}\n{} <{}>\n{}",
                    line.oid.short(),
                    c.summary,
                    c.author_name,
                    c.author_email,
                    full_time(c.author_time)
                )
            };
            h_flex()
                .id(SharedString::from(format!("blame-ann-{i}")))
                .size_full()
                .gap_2()
                .px_2()
                .text_ui_xs(cx)
                .text_color(colors.text_muted)
                .tooltip(Tooltip::text(title))
                .when(!uncommitted, |d| {
                    d.cursor_pointer()
                        .hover(|s| s.bg(colors.element_hover).text_color(colors.text))
                        .on_click(cx.listener(move |app, _, _, cx| app.reveal_commit(id, &oid, cx)))
                })
                .map(|d| {
                    if uncommitted {
                        d.child(div().italic().child("Not committed"))
                    } else {
                        d.child(
                            div()
                                .flex_none()
                                .text_color(colors.text_accent)
                                .child(line.oid.short().to_owned()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(c.author_name.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .child(relative_time(c.author_time, now_secs())),
                        )
                    }
                })
                .into_any_element()
        }
        None => div().into_any_element(),
    };
    let (text, highlights) = match tokens.and_then(|t| t.get(i)) {
        Some(toks) => line_text(toks, false, colors.text_muted),
        None => (line.content.clone(), Vec::new()),
    };
    // Rows must keep one height (uniform list), so the group rule is an overlay.
    let rule = (start && i > 0).then(|| {
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(1.))
            .bg(colors.border_variant)
    });
    h_flex()
        .relative()
        .w_full()
        .flex_none()
        .whitespace_nowrap()
        .children(rule)
        .when(uncommitted, |d| d.bg(theme.status().modified.opacity(0.08)))
        .child(
            div()
                .flex_none()
                .w(px(ANNOTATION_W))
                .h_full()
                .border_r_1()
                .border_color(colors.border_variant)
                .child(annotation),
        )
        .child(
            div()
                .flex_none()
                .w(px(8.0 * digits as f32 + 16.0))
                .pr_2()
                .flex()
                .justify_end()
                .text_color(colors.text_muted.opacity(0.6))
                .child(line.line_no.to_string()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .pl_3()
                .child(StyledText::new(text).with_highlights(highlights)),
        )
        .into_any_element()
}
