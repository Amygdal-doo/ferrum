//! The full-window Git view (Editor | Git toggle): changes and recent
//! history on the left, the selected file's or commit's diff on the right.

use ferrum_ui::prelude::*;
use gpui::{FontWeight, ListSizingBehavior, ScrollHandle, UniformListScrollHandle};

use super::diff::{self, RowKind};
use super::{GitPanel, GitSelection, GitState};
use crate::settings::Settings;
use crate::theme_apply::ui;

pub struct GitView {
    state: Entity<GitState>,
    panel: Entity<GitPanel>,
    history_scroll: ScrollHandle,
    diff_scroll: UniformListScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl GitView {
    pub fn new(state: Entity<GitState>, panel: Entity<GitPanel>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&state, |_, _, cx| cx.notify());
        Self {
            state,
            panel,
            history_scroll: ScrollHandle::new(),
            diff_scroll: UniformListScrollHandle::new(),
            _subscriptions: vec![subscription],
        }
    }

    fn render_history(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let state = self.state.read(cx);
        let selected = match &state.selection {
            Some(GitSelection::Commit(oid)) => Some(oid.clone()),
            _ => None,
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let rows: Vec<AnyElement> = state
            .commits
            .iter()
            .map(|commit| {
                let oid = commit.oid.clone();
                let is_selected = selected.as_ref() == Some(&oid);
                v_flex()
                    .id(SharedString::from(format!("commit-{}", oid.as_str())))
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .when(is_selected, |d| d.bg(colors.element_selected))
                    .hover(|d| d.bg(colors.element_hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let oid = oid.clone();
                        this.state.update(cx, |s, cx| s.select_commit(oid, cx));
                    }))
                    .child(
                        div()
                            .text_sm()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(commit.summary.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(colors.text_muted)
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(format!(
                                "{} · {} · {}",
                                commit.oid.as_str().chars().take(7).collect::<String>(),
                                commit.author_name,
                                relative_time(now - commit.author_time)
                            )),
                    )
                    .into_any_element()
            })
            .collect();
        v_flex()
            .flex_1()
            .min_h_0()
            .border_t_1()
            .border_color(colors.border)
            .child(section_header("History"))
            .child(
                v_flex()
                    .id("git-history")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.history_scroll)
                    .children(rows),
            )
    }

    fn render_diff(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let settings = cx.global::<Settings>();
        let theme = settings.theme.clone();
        let font = SharedString::from(settings.config.font_family.clone());
        let font_size = px(settings.font_size());
        let row_height = (font_size * 1.5).round();
        let state = self.state.read(cx);
        let rows = state.diff.clone();
        let header: AnyElement = match (&state.selection, &state.commit) {
            (Some(GitSelection::Commit(_)), Some(details)) => v_flex()
                .px_3()
                .py_2()
                .gap_0p5()
                .border_b_1()
                .border_color(colors.border)
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(details.commit.summary.clone()),
                )
                .when(!details.body.trim().is_empty(), |d| {
                    d.child(
                        div()
                            .text_sm()
                            .text_color(colors.text_muted)
                            .child(details.body.trim().to_string()),
                    )
                })
                .child(div().text_xs().text_color(colors.text_muted).child(format!(
                    "{} <{}> · {}",
                    details.commit.author_name, details.commit.author_email, details.commit.oid
                )))
                .into_any_element(),
            (Some(GitSelection::File { path, staged }), _) => h_flex()
                .px_3()
                .h(px(32.))
                .gap_2()
                .border_b_1()
                .border_color(colors.border)
                .child(div().text_sm().child(path.clone()))
                .child(
                    div()
                        .text_xs()
                        .text_color(colors.text_muted)
                        .child(if *staged { "staged" } else { "working tree" }),
                )
                .into_any_element(),
            _ => div().into_any_element(),
        };
        if state.selection.is_none() {
            return v_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .justify_center()
                .text_color(colors.text_muted)
                .child("Select a changed file or a commit to see its diff.")
                .into_any_element();
        }
        let list = uniform_list("git-diff", rows.len(), move |range, _, _| {
            let theme = theme.clone();
            range
                .map(|i| {
                    let row = &rows[i];
                    let (bg, fg) = diff::colors(row.kind, &theme);
                    let number = |n: Option<u32>| n.map_or(String::new(), |n| n.to_string());
                    let muted = ui(&theme, "line_number");
                    let marker = match row.kind {
                        RowKind::Added => "+",
                        RowKind::Removed => "−",
                        _ => " ",
                    };
                    let line = matches!(
                        row.kind,
                        RowKind::Added | RowKind::Removed | RowKind::Context
                    );
                    h_flex()
                        .h(row_height)
                        .w_full()
                        .when_some(bg, |d, bg| d.bg(bg))
                        .text_color(fg)
                        .when(row.kind == RowKind::File, |d| {
                            d.font_weight(FontWeight::SEMIBOLD).mt_1()
                        })
                        .when(line, |d| {
                            d.child(
                                div()
                                    .w(px(44.))
                                    .flex_none()
                                    .text_right()
                                    .pr_1()
                                    .text_color(muted)
                                    .child(number(row.old)),
                            )
                            .child(
                                div()
                                    .w(px(44.))
                                    .flex_none()
                                    .text_right()
                                    .pr_1()
                                    .text_color(muted)
                                    .child(number(row.new)),
                            )
                            .child(div().w(px(16.)).flex_none().child(marker))
                        })
                        .when(!line, |d| d.pl_2())
                        .child(div().whitespace_nowrap().child(row.text.clone()))
                        .into_any_element()
                })
                .collect::<Vec<_>>()
        })
        .with_sizing_behavior(ListSizingBehavior::Auto)
        .flex_1()
        .font_family(font)
        .text_size(font_size)
        .track_scroll(&self.diff_scroll);
        v_flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(header)
            .child(list)
            .into_any_element()
    }
}

/// "3 min ago", "2 days ago".
fn relative_time(seconds: i64) -> String {
    let s = seconds.max(0);
    let (n, unit) = match s {
        0..60 => return "just now".to_string(),
        60..3600 => (s / 60, "min"),
        3600..86_400 => (s / 3600, "hour"),
        86_400..2_592_000 => (s / 86_400, "day"),
        2_592_000..31_536_000 => (s / 2_592_000, "month"),
        _ => (s / 31_536_000, "year"),
    };
    let plural = if n == 1 || unit == "min" { "" } else { "s" };
    format!("{n} {unit}{plural} ago")
}

impl Render for GitView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        h_flex()
            .size_full()
            .bg(colors.editor_background)
            .child(
                v_flex()
                    .w(px(340.))
                    .flex_none()
                    .h_full()
                    .border_r_1()
                    .border_color(colors.border)
                    .child(div().flex_1().min_h_0().child(self.panel.clone()))
                    .child(self.render_history(cx)),
            )
            .child(self.render_diff(cx))
    }
}

#[cfg(test)]
mod tests {
    use super::relative_time;

    #[test]
    fn relative_times() {
        assert_eq!(relative_time(5), "just now");
        assert_eq!(relative_time(120), "2 min ago");
        assert_eq!(relative_time(3600), "1 hour ago");
        assert_eq!(relative_time(86_400 * 3), "3 days ago");
    }
}
