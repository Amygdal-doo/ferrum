//! The banner shown on every page while `git bisect` runs: what to do next,
//! the commit under test, Good / Bad / Skip for it, and Reset.

use crate::app::GitView;
use crate::components::widgets::{primary_button, small_button};
use ferrum_ui::components::{
    ButtonCommon, Clickable, Color, Disableable, Label, LabelCommon, LabelSize, Tooltip, h_flex,
    v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumgit_domain::{BisectMark, RepoId, RepoOp};
use gpui::{AnyElement, Context, SharedString, div, prelude::*};

pub fn bisect_banner(app: &GitView, id: RepoId, cx: &mut Context<GitView>) -> Option<AnyElement> {
    let data = app.state.view(id).data?;
    let status = &data.snapshot.status;
    let state = status.bisect.clone()?;
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let busy = app.state.is_busy(id);
    let describe = |oid: &ferrumgit_domain::Oid| {
        let summary = data
            .snapshot
            .commits
            .iter()
            .find(|c| &c.oid == oid)
            .map(|c| c.summary.clone())
            .unwrap_or_default();
        format!("{} {summary}", oid.short()).trim_end().to_owned()
    };
    let detail = match (&state.found, &status.branch.oid) {
        (Some(found), _) => Some(describe(found)),
        (None, Some(head)) => Some(format!("Testing {}", describe(head))),
        (None, None) => None,
    };
    let mark = |mark: BisectMark, tip: &'static str| {
        small_button(
            SharedString::from(format!("bisect-{}", mark.command())),
            mark.label(),
        )
        .tooltip(Tooltip::text(tip))
        .disabled(busy)
        .on_click(cx.listener(move |app, _, _, cx| {
            app.op(id, RepoOp::BisectMark { mark, rev: None }, cx);
        }))
    };
    let buttons: Vec<AnyElement> = match state.found.clone() {
        Some(found) => vec![
            small_button("bisect-show", "Show")
                .on_click(cx.listener(move |app, _, _, cx| app.reveal_commit(id, &found, cx)))
                .into_any_element(),
        ],
        None => vec![
            mark(BisectMark::Good, "The bug is not in this commit").into_any_element(),
            mark(BisectMark::Bad, "The bug is in this commit").into_any_element(),
            mark(BisectMark::Skip, "This commit cannot be tested").into_any_element(),
        ],
    };
    let reset_tip = format!("End bisecting and check out {}", state.start);
    Some(
        h_flex()
            .flex_none()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(colors.border)
            .bg(theme.status().info_background)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        Label::new(state.summary())
                            .size(LabelSize::Small)
                            .color(Color::Info),
                    )
                    .children(detail.map(|d| {
                        div().min_w_0().child(
                            Label::new(d)
                                .size(LabelSize::XSmall)
                                .color(Color::Muted)
                                .truncate(),
                        )
                    })),
            )
            .children(buttons)
            .child(
                primary_button("bisect-reset", "Reset")
                    .tooltip(Tooltip::text(reset_tip))
                    .disabled(busy)
                    .on_click(
                        cx.listener(move |app, _, _, cx| app.op(id, RepoOp::BisectReset, cx)),
                    ),
            )
            .into_any_element(),
    )
}
