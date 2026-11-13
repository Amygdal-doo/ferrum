//! Side-by-side diff rows and the unified / split layout switch.

use crate::app::FerrumGit;
use crate::components::code::{line_text, toggle_line};
use crate::components::highlight::Token;
use ferrum_ui::components::{StyledTypography, h_flex};
use ferrum_ui::theme::ActiveTheme;
use ferrumgit_domain::{DiffLine, LineKind, RepoId};
use gpui::{AnyElement, Context, Hsla, SharedString, StyledText, div, prelude::*, px};

/// One half of a side-by-side row: the old (`left`) or new line, or an empty
/// filler when the other side has no counterpart. Lines are clipped, not wrapped.
#[allow(clippy::too_many_arguments)]
pub fn split_side(
    repo: Option<RepoId>,
    hunk: usize,
    line: Option<(usize, &DiffLine)>,
    toks: &[Token],
    selected: bool,
    selectable: bool,
    invisibles: bool,
    left: bool,
    id_base: &str,
    theme: &ferrum_ui::theme::Theme,
    cx: &mut Context<FerrumGit>,
) -> AnyElement {
    let colors = theme.colors();
    let status = theme.status();
    let half = div()
        .id(SharedString::from(format!(
            "{id_base}-{}",
            if left { "old" } else { "new" }
        )))
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .flex()
        .when(left, |d| d.border_r_1().border_color(colors.border_variant));
    let Some((index, line)) = line else {
        return half
            .bg(colors.element_background.opacity(0.35))
            .child(div().w(px(48.)).child("\u{a0}"))
            .into_any_element();
    };
    let (tone, gutter_bg, gutter_fg): (Option<Hsla>, Option<Hsla>, Hsla) = match line.kind {
        LineKind::Added => (
            Some(status.created.opacity(0.08)),
            Some(status.created.opacity(0.14)),
            status.created,
        ),
        LineKind::Removed => (
            Some(status.deleted.opacity(0.08)),
            Some(status.deleted.opacity(0.14)),
            status.deleted,
        ),
        LineKind::Context | LineKind::NoNewline => (None, None, colors.text_muted.opacity(0.6)),
    };
    let number = if left { line.old_no } else { line.new_no };
    let content: AnyElement = if line.kind == LineKind::NoNewline {
        div()
            .italic()
            .text_color(colors.text_muted)
            .child(line.content.clone())
            .into_any_element()
    } else {
        let (text, highlights) = line_text(toks, invisibles, colors.text_muted.opacity(0.35));
        StyledText::new(text)
            .with_highlights(highlights)
            .into_any_element()
    };
    let clickable =
        repo.is_some() && selectable && matches!(line.kind, LineKind::Added | LineKind::Removed);
    half.when_some(tone, |d, t| d.bg(t))
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
                .child(number.map(|n| n.to_string()).unwrap_or_default()),
        )
        .child(div().pl_3().pr_2().child(content))
        .into_any_element()
}

/// "Unified | Split" switch for the diff layout; remembered in the settings.
pub fn layout_toggle(app: &FerrumGit, cx: &mut Context<FerrumGit>) -> AnyElement {
    let colors = cx.theme().colors().clone();
    let split = app.state.settings.side_by_side;
    let segment = |label: &'static str, on: bool, cx: &mut Context<FerrumGit>| {
        let active = split == on;
        div()
            .id(SharedString::from(format!("diff-layout-{label}")))
            .px_2()
            .py_0p5()
            .cursor_pointer()
            .text_ui_xs(cx)
            .when(active, |d| {
                d.bg(colors.text_accent).text_color(colors.background)
            })
            .when(!active, |d| d.hover(|s| s.bg(colors.element_hover)))
            .child(label)
            .on_click(cx.listener(move |app, _, _, cx| {
                if app.state.settings.side_by_side != on {
                    app.state.settings.side_by_side = on;
                    app.save_settings(cx);
                    cx.notify();
                }
            }))
    };
    h_flex()
        .flex_none()
        .rounded_md()
        .border_1()
        .border_color(colors.border)
        .overflow_hidden()
        .child(segment("Unified", false, cx))
        .child(segment("Split", true, cx))
        .into_any_element()
}
