//! The Pull requests tab: open PRs with their combined checks; a click
//! shows the runs of the PR's branch.

use crate::ci::widgets::check_icon;
use crate::ci::{CiView, Tab};
use chrono::{DateTime, Utc};
use ferrum_ui::prelude::*;
use ferrumci_core::PullRequest;
use ferrumci_core::format::relative;

impl CiView {
    /// Runs tab, filtered to the PR's head branch.
    pub(crate) fn show_pr_runs(&mut self, pr: &PullRequest, cx: &mut Context<Self>) {
        self.tab = Tab::Runs;
        self.detail = None;
        self.filter.workflow_id = None;
        self.filter.event = None;
        self.filter.status = None;
        self.set_branch_filter(Some(pr.head_branch.clone()), cx);
    }
}

fn row(pr: &PullRequest, cx: &mut Context<CiView>) -> impl IntoElement {
    let now = Utc::now();
    let updated = pr
        .updated_at
        .parse::<DateTime<Utc>>()
        .map(|t| relative(t, now))
        .unwrap_or_default();
    let checks = if pr.status_loaded {
        pr.checks.label()
    } else {
        "Checks not loaded"
    };
    let mut meta = vec![
        format!("#{}", pr.number),
        pr.author.clone(),
        format!("{} → {}", pr.head_branch, pr.base_branch),
        checks.to_owned(),
    ];
    if pr.draft {
        meta.push("draft".into());
    }
    let url = pr.web_url.clone();
    let clicked = pr.clone();
    ListItem::new(SharedString::from(format!("pr-{}", pr.number)))
        .rounded()
        .tooltip(Tooltip::text("Show this branch's runs"))
        .on_click(cx.listener(move |this, _, _, cx| this.show_pr_runs(&clicked, cx)))
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .py_0p5()
                .child(check_icon(pr.checks))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(Label::new(pr.title.clone()).truncate())
                        .child(
                            Label::new(meta.join(" · "))
                                .size(LabelSize::Small)
                                .color(Color::Muted)
                                .truncate(),
                        ),
                )
                .child(
                    Label::new(updated)
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .child(
                    IconButton::new(
                        SharedString::from(format!("pr-open-{}", pr.number)),
                        IconName::ArrowUpRight,
                    )
                    .tooltip(Tooltip::text("Open on GitHub"))
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        cx.open_url(&url);
                    }),
                ),
        )
}

pub fn render(app: &mut CiView, cx: &mut Context<CiView>) -> AnyElement {
    let rows: Vec<_> = app
        .prs
        .iter()
        .map(|pr| row(pr, cx).into_any_element())
        .collect();
    let empty = rows.is_empty();
    let loading = app.prs_loading;
    v_flex()
        .size_full()
        .child(
            h_flex()
                .flex_none()
                .px_2()
                .py_1()
                .gap_2()
                .border_b_1()
                .border_color(cx.theme().colors().border)
                .child(
                    Label::new(format!("{} open pull requests", app.prs.len()))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .child(div().flex_1())
                .child(
                    IconButton::new("refresh-prs", IconName::RotateCw)
                        .tooltip(Tooltip::text("Refresh pull requests"))
                        .on_click(cx.listener(|this, _, _, cx| this.load_prs(cx))),
                ),
        )
        .child(
            div()
                .id("prs")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p_1()
                .when(empty, |el| {
                    el.child(if loading {
                        empty_state("Loading pull requests…", "")
                    } else {
                        empty_state("No open pull requests", "")
                    })
                })
                .children(rows),
        )
        .into_any_element()
}
