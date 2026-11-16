//! Commit list rows: month header and two-line commit rows (Tower style).

use super::lanes::{paint_pass_through, paint_row};
use super::{MONTH_H, ROW_H};
use crate::app::GitView;
use crate::components::actions;
use crate::components::{avatar_color, selected_colors};
use crate::state::Selection;
use ferrum_ui::components::{StyledTypography, Tooltip, h_flex, v_flex};
use ferrum_ui::theme::{Theme, contrast};
use ferrumgit_application::GraphRow;
use ferrumgit_domain::text::{full_time, initials};
use ferrumgit_domain::{Commit, GitRef, RefKind, RepoId};
use gpui::{
    AnyElement, App, Hsla, MouseButton, MouseDownEvent, SharedString, WeakEntity, canvas, div,
    prelude::*, px,
};
use std::sync::Arc;

pub fn month_row(
    label: String,
    graph_px: f32,
    rails: Vec<(usize, usize)>,
    theme: &Theme,
    cx: &App,
) -> AnyElement {
    h_flex()
        .relative()
        .w_full()
        .h(px(MONTH_H))
        .items_end()
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(8.))
                .w(px(graph_px))
                .child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, (), window, _| {
                            paint_pass_through(bounds, &rails, window);
                        },
                    )
                    .size_full(),
                ),
        )
        .child(
            div()
                .pb_1p5()
                .pl(px(graph_px + 18.0))
                .text_ui_xs(cx)
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.colors().text_muted)
                .child(label),
        )
        .into_any_element()
}

/// `5. 11. 2026.` (local time).
fn short_date(unix: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(unix, 0)
        .single()
        .map_or_else(String::new, |t| t.format("%-d. %-m. %Y.").to_string())
}

pub struct CommitRowProps {
    pub repo: RepoId,
    pub commit: Commit,
    pub graph: Option<GraphRow>,
    pub refs: Vec<GitRef>,
    pub is_head: bool,
    pub selected: bool,
    /// While searching: whether this commit matches (others are dimmed).
    pub matched: Option<bool>,
    pub graph_px: f32,
    pub lanes: usize,
}

pub fn commit_row(
    props: CommitRowProps,
    view: WeakEntity<GitView>,
    theme: Arc<Theme>,
    cx: &App,
) -> AnyElement {
    let CommitRowProps {
        repo,
        commit,
        graph,
        refs,
        is_head,
        selected,
        matched,
        graph_px,
        lanes,
    } = props;
    let colors = theme.colors();
    let accent = colors.text_accent;
    let (card_bg, text, muted) = if selected {
        let (bg, text, muted) = selected_colors(&theme);
        (Some(bg), text, muted)
    } else {
        (None, colors.text, colors.text_muted)
    };
    let oid = commit.oid.clone();
    let menu_oid = commit.oid.clone();
    let foreground = colors.text;
    let background = colors.background;
    let hover = colors.element_hover;
    // Opaque, so the initials read the same on the selected card.
    let avatar_bg = contrast::composite(avatar_color(&commit.author_email), background);
    let author_title = format!("{} <{}>", commit.author_name, commit.author_email);
    let open_view = view.clone();

    h_flex()
        .id(SharedString::from(format!("commit-{}", commit.oid)))
        .w_full()
        .h(px(ROW_H))
        .items_stretch()
        .on_mouse_down(MouseButton::Right, move |event: &MouseDownEvent, _, cx| {
            let position = event.position;
            if let Err(error) = view.update(cx, |app, cx| {
                let items = actions::for_commit(&menu_oid, app.bisecting());
                app.open_menu(position, items, cx);
            }) {
                tracing::debug!(error = %error, "view closed");
            }
        })
        .child(
            div()
                .flex_none()
                .pl_2()
                .w(px(graph_px + 8.0))
                .children(graph.map(|g| {
                    canvas(
                        |_, _, _| (),
                        move |bounds, (), window, _| {
                            paint_row(bounds, &g, lanes, is_head, foreground, background, window);
                        },
                    )
                    .w(px(graph_px))
                    .h_full()
                })),
        )
        .child(
            h_flex()
                .id(SharedString::from(format!("commit-card-{}", commit.oid)))
                .mr_2()
                .flex_1()
                .min_w_0()
                .gap_2p5()
                .px_2()
                .rounded_lg()
                .text_color(text)
                .when_some(card_bg, |d, bg| d.bg(bg))
                .when(matched == Some(true) && !selected, |d| {
                    d.bg(accent.opacity(0.14))
                })
                .when(matched == Some(false) && !selected, |d| d.opacity(0.4))
                .when(!selected, |d| d.hover(move |s| s.bg(hover)))
                .on_click(move |_, _, cx| {
                    if let Err(error) = open_view.update(cx, |app, cx| {
                        app.navigate(repo, Selection::Commit(oid.clone()), cx);
                    }) {
                        tracing::debug!(error = %error, "view closed");
                    }
                })
                .child(
                    div()
                        .id(SharedString::from(format!("avatar-{}", commit.oid)))
                        .flex_none()
                        .size(px(28.))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(avatar_bg)
                        .text_color(contrast::readable_text_on(avatar_bg))
                        .text_ui_xs(cx)
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .tooltip(Tooltip::text(author_title))
                        .child(initials(&commit.author_name)),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .text_ui_sm(cx)
                        .child(
                            h_flex()
                                .min_w_0()
                                .gap_1p5()
                                .child(
                                    div()
                                        .flex_none()
                                        .max_w(gpui::relative(0.45))
                                        .truncate()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child(commit.author_name.clone()),
                                )
                                .child(
                                    h_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .gap_1()
                                        .overflow_hidden()
                                        .children(
                                            refs.iter().map(|r| ref_badge(r, selected, &theme, cx)),
                                        ),
                                )
                                .child(
                                    div()
                                        .id(SharedString::from(format!("date-{}", commit.oid)))
                                        .flex_none()
                                        .pl_2()
                                        .text_ui_xs(cx)
                                        .text_color(muted)
                                        .tooltip(Tooltip::text(full_time(commit.author_time)))
                                        .child(short_date(commit.author_time)),
                                ),
                        )
                        .child(
                            h_flex()
                                .min_w_0()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_none()
                                        .font_buffer(cx)
                                        .text_ui_xs(cx)
                                        .text_color(muted)
                                        .child(commit.oid.short().to_owned()),
                                )
                                .child(div().min_w_0().truncate().child(commit.summary.clone())),
                        ),
                ),
        )
        .into_any_element()
}

/// (background, text) of a ref chip. Coloured chips are solid fills with
/// near-black or near-white text, whichever reads better; neutral chips use
/// regular text on an element fill. `on_card` is a chip inside the selected
/// commit card.
pub fn ref_badge_colors(r: &GitRef, on_card: bool, theme: &Theme) -> (Hsla, Hsla) {
    let colors = theme.colors();
    let status = theme.status();
    let filled = |fill: Hsla| (fill, theme.readable_text_on(fill));
    if on_card {
        // Push the chip away from the text colour so it stays readable.
        let (card, text, _) = selected_colors(theme);
        let away = contrast::readable_text_on(text);
        return (contrast::composite(away.opacity(0.18), card), text);
    }
    match r.kind {
        Some(RefKind::LocalBranch) if r.is_head => filled(colors.text_accent),
        Some(RefKind::LocalBranch) => filled(status.info),
        Some(RefKind::RemoteBranch) => (colors.element_active, colors.text),
        Some(RefKind::Tag) => filled(status.warning),
        None => (colors.element_background, colors.text),
    }
}

pub fn ref_badge(r: &GitRef, on_card: bool, theme: &Theme, cx: &App) -> AnyElement {
    let (bg, fg) = ref_badge_colors(r, on_card, theme);
    let label = if r.is_head {
        format!("HEAD → {}", r.name)
    } else {
        r.name.clone()
    };
    div()
        .id(SharedString::from(format!("badge-{}", r.full_name)))
        .flex_none()
        .max_w(px(160.))
        .truncate()
        .px_1p5()
        .rounded_sm()
        .bg(bg)
        .text_color(fg)
        .text_ui_xs(cx)
        .font_weight(gpui::FontWeight::MEDIUM)
        .tooltip(Tooltip::text(r.full_name.clone()))
        .child(label)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::avatar_color;
    use ferrum_ui::theme::contrast::{MIN_TEXT_CONTRAST, contrast_over};
    use ferrum_ui::theme::{dark, light};

    #[test]
    fn every_ref_chip_is_readable_in_both_themes() {
        let kinds = [
            (None, false),
            (Some(RefKind::LocalBranch), false),
            (Some(RefKind::LocalBranch), true),
            (Some(RefKind::RemoteBranch), false),
            (Some(RefKind::Tag), false),
        ];
        for theme in [dark(), light()] {
            let base = theme.colors().background;
            for (kind, is_head) in kinds {
                let r = GitRef {
                    name: "git-v0.4.0-alpha.1".into(),
                    kind,
                    is_head,
                    ..GitRef::default()
                };
                for on_card in [false, true] {
                    let (bg, fg) = ref_badge_colors(&r, on_card, &theme);
                    let ratio = contrast_over(fg, bg, base);
                    assert!(
                        ratio >= MIN_TEXT_CONTRAST,
                        "{}: {kind:?} head={is_head} card={on_card}: {ratio:.2}",
                        theme.name
                    );
                }
            }
        }
    }

    #[test]
    fn selected_card_and_avatars_are_readable() {
        for theme in [dark(), light()] {
            let base = theme.colors().background;
            let (fill, text, muted) = selected_colors(&theme);
            assert!(contrast_over(text, fill, base) >= MIN_TEXT_CONTRAST);
            let m = contrast_over(muted, fill, base);
            assert!(m >= MIN_TEXT_CONTRAST, "{}: muted {m:.2}", theme.name);
            for seed in ["a", "b", "c", "d", "e", "f", "g", "h"] {
                let avatar = avatar_color(seed);
                let shown = contrast::composite(avatar, base);
                let r = contrast_over(contrast::readable_text_on(shown), shown, base);
                assert!(
                    r >= MIN_TEXT_CONTRAST,
                    "{}: avatar {seed} {r:.2}",
                    theme.name
                );
                // Avatars elsewhere inherit the regular text colour.
                let r = contrast_over(theme.colors().text, shown, base);
                assert!(
                    r >= MIN_TEXT_CONTRAST,
                    "{}: avatar text {seed} {r:.2}",
                    theme.name
                );
            }
        }
    }
}
