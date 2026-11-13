//! Pull requests page, right panel: description, conversation, commits,
//! changed files (local diff against the merge base) and CI checks.

use super::commit::file_section;
use super::pull_list::{check_color, chip, dot, review_color, unix};
use crate::app::FerrumGit;
use crate::commands::pulls::open_url;
use crate::components::now_secs;
use crate::components::widgets::{empty, ghost_button, primary_button, small_button};
use crate::state::{Dialog, Load, PrTab};
use ferrum_ui::components::{
    Clickable, Color, Disableable, Label, LabelCommon, LabelSize, StyledTypography, h_flex, v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumgit_application::forge::markdown::{Block, blocks};
use ferrumgit_application::forge::{
    PrDetail, PrState, ReviewDecision, ReviewEvent, ReviewState, TimelineKind,
};
use ferrumgit_domain::text::relative_time;
use ferrumgit_domain::{ForgeKind, RepoId};
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};
use std::sync::Arc;

/// Basic Markdown as stacked text blocks.
fn markdown(md: &str, cx: &mut Context<FerrumGit>) -> AnyElement {
    let colors = cx.theme().colors().clone();
    let items: Vec<AnyElement> = blocks(md)
        .into_iter()
        .map(|b| match b {
            Block::Heading(level, t) => Label::new(t)
                .size(if level <= 2 {
                    LabelSize::Large
                } else {
                    LabelSize::Default
                })
                .weight(gpui::FontWeight::SEMIBOLD)
                .into_any_element(),
            Block::Paragraph(t) => div().text_ui_sm(cx).child(t).into_any_element(),
            Block::Bullet(indent, t) => h_flex()
                .items_start()
                .gap_1p5()
                .pl(px(12. * indent as f32))
                .text_ui_sm(cx)
                .child(if t.starts_with('☐') || t.starts_with('☑') {
                    ""
                } else {
                    "•"
                })
                .child(div().flex_1().min_w_0().child(t))
                .into_any_element(),
            Block::Quote(t) => div()
                .pl_3()
                .border_l_2()
                .border_color(colors.border)
                .text_ui_sm(cx)
                .text_color(colors.text_muted)
                .child(t)
                .into_any_element(),
            Block::Code(t) => div()
                .px_2()
                .py_1()
                .rounded_sm()
                .bg(colors.element_background)
                .font_buffer(cx)
                .text_ui_xs(cx)
                .child(t)
                .into_any_element(),
            Block::Rule => div()
                .h(px(1.))
                .w_full()
                .bg(colors.border)
                .into_any_element(),
        })
        .collect();
    v_flex()
        .gap_2()
        .min_w_0()
        .children(items)
        .into_any_element()
}

fn state_chip(d: &PrDetail) -> (String, Color) {
    match d.pr.state {
        PrState::Merged => ("Merged".into(), Color::Accent),
        PrState::Closed => ("Closed".into(), Color::Error),
        PrState::Open if d.pr.draft => ("Draft".into(), Color::Muted),
        PrState::Open => ("Open".into(), Color::Success),
    }
}

fn conversation(d: &PrDetail, cx: &mut Context<FerrumGit>) -> Vec<AnyElement> {
    let colors = cx.theme().colors().clone();
    let now = now_secs();
    let mut out = vec![
        v_flex()
            .gap_2()
            .px_5()
            .py_4()
            .border_b_1()
            .border_color(colors.border)
            .child(if d.body.trim().is_empty() {
                Label::new("No description.")
                    .color(Color::Muted)
                    .into_any_element()
            } else {
                markdown(&d.body, cx)
            })
            .into_any_element(),
    ];
    if d.timeline.is_empty() {
        out.push(
            div()
                .px_5()
                .py_3()
                .child(
                    Label::new("No comments or reviews yet.")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element(),
        );
    }
    for item in &d.timeline {
        let (what, color) = match &item.kind {
            TimelineKind::Comment => ("commented".to_owned(), Color::Muted),
            TimelineKind::Review(ReviewState::Approved) => ("approved".to_owned(), Color::Success),
            TimelineKind::Review(ReviewState::ChangesRequested) => {
                ("requested changes".to_owned(), Color::Error)
            }
            TimelineKind::Review(ReviewState::Dismissed) => {
                ("review dismissed".to_owned(), Color::Muted)
            }
            TimelineKind::Review(_) => ("reviewed".to_owned(), Color::Muted),
            TimelineKind::LineComment { path, line } => (
                match line {
                    Some(l) => format!("commented on {path}:{l}"),
                    None => format!("commented on {path}"),
                },
                Color::Muted,
            ),
            TimelineKind::Event => (String::new(), Color::Muted),
        };
        let event = item.kind == TimelineKind::Event;
        let header = h_flex()
            .gap_2()
            .child(
                Label::new(item.author.clone())
                    .size(LabelSize::Small)
                    .weight(gpui::FontWeight::SEMIBOLD),
            )
            .when(event, |d| {
                d.child(
                    Label::new(item.body.clone())
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .truncate(),
                )
            })
            .when(!what.is_empty(), |d| {
                d.child(Label::new(what).size(LabelSize::Small).color(color))
            })
            .child(div().flex_1())
            .child(
                Label::new(relative_time(unix(&item.created_at), now))
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            );
        out.push(
            v_flex()
                .gap_1p5()
                .px_5()
                .py_3()
                .border_b_1()
                .border_color(colors.border_variant)
                .child(header)
                .when(!event && !item.body.trim().is_empty(), |d| {
                    d.child(markdown(&item.body, cx))
                })
                .into_any_element(),
        );
    }
    out
}

fn commits(d: &PrDetail, cx: &mut Context<FerrumGit>) -> Vec<AnyElement> {
    let colors = cx.theme().colors().clone();
    let now = now_secs();
    if d.commits.is_empty() {
        return vec![empty("No commits")];
    }
    d.commits
        .iter()
        .map(|c| {
            h_flex()
                .gap_3()
                .px_5()
                .py_2()
                .border_b_1()
                .border_color(colors.border_variant)
                .child(
                    div()
                        .flex_none()
                        .font_buffer(cx)
                        .text_ui_xs(cx)
                        .text_color(colors.text_muted)
                        .child(c.sha.chars().take(8).collect::<String>()),
                )
                .child(
                    div().flex_1().min_w_0().child(
                        Label::new(c.summary.clone())
                            .size(LabelSize::Small)
                            .truncate(),
                    ),
                )
                .child(
                    Label::new(c.author.clone())
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
                .child(
                    Label::new(relative_time(unix(&c.date), now))
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
                .into_any_element()
        })
        .collect()
}

fn checks(d: &PrDetail, cx: &mut Context<FerrumGit>) -> Vec<AnyElement> {
    let colors = cx.theme().colors().clone();
    if d.checks.is_empty() {
        return vec![empty("No checks reported for the head commit")];
    }
    d.checks
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let url = c.url.clone();
            h_flex()
                .id(SharedString::from(format!("pr-check-{i}")))
                .gap_3()
                .px_5()
                .py_2()
                .border_b_1()
                .border_color(colors.border_variant)
                .when(url.is_some(), |d| {
                    d.cursor_pointer().hover(|s| s.bg(colors.element_hover))
                })
                .on_click(move |_, _, _| {
                    if let Some(u) = &url {
                        open_url(u);
                    }
                })
                .child(dot(check_color(c.state, cx)))
                .child(Label::new(c.name.clone()).size(LabelSize::Small))
                .child(
                    div().flex_1().min_w_0().child(
                        Label::new(c.detail.clone())
                            .size(LabelSize::XSmall)
                            .color(Color::Muted)
                            .truncate(),
                    ),
                )
                .child(
                    Label::new(c.state.label())
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
                .into_any_element()
        })
        .collect()
}

fn files(app: &FerrumGit, id: RepoId, cx: &mut Context<FerrumGit>) -> Vec<AnyElement> {
    let view = app.state.view(id);
    match &view.prs.changes {
        Load::Idle | Load::Loading => {
            vec![empty("Fetching the pull request and computing the diff…")]
        }
        Load::Failed(e) => vec![
            v_flex()
                .gap_2()
                .p_5()
                .child(Label::new(e.clone()).color(Color::Error))
                .child(
                    small_button("pr-files-retry", "Retry")
                        .on_click(cx.listener(move |app, _, _, cx| app.load_pr_changes(id, cx))),
                )
                .into_any_element(),
        ],
        Load::Ready(changes) => {
            let (adds, dels) = changes
                .files
                .iter()
                .fold((0, 0), |(a, d), f| (a + f.additions, d + f.deletions));
            let paths: Vec<String> = changes.files.iter().map(|f| f.path.clone()).collect();
            let all_open = !paths.is_empty() && paths.iter().all(|p| view.expanded.contains(p));
            let mut out = vec![
                h_flex()
                    .h(px(36.))
                    .gap_3()
                    .px_5()
                    .child(
                        ghost_button(
                            "pr-expand-all",
                            if all_open {
                                "Collapse All"
                            } else {
                                "Expand All"
                            },
                        )
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.state.update_view(id, |v| {
                                v.expanded = if all_open {
                                    Default::default()
                                } else {
                                    paths.iter().cloned().collect()
                                };
                            });
                            cx.notify();
                        })),
                    )
                    .child(div().flex_1())
                    .child(
                        Label::new(format!(
                            "{} files, +{adds} −{dels} · against merge base {}",
                            changes.files.len(),
                            changes.merge_base.chars().take(8).collect::<String>()
                        ))
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                    )
                    .into_any_element(),
            ];
            if changes.truncated {
                out.push(
                    div()
                        .px_5()
                        .pb_2()
                        .child(
                            Label::new("Large diff: only the first files are shown.")
                                .size(LabelSize::XSmall)
                                .color(Color::Warning),
                        )
                        .into_any_element(),
                );
            }
            for f in &changes.files {
                let open = view.expanded.contains(&f.path);
                out.push(file_section(app, id, Arc::new(f.clone()), open, None, cx));
            }
            out
        }
    }
}

pub fn pull_detail(app: &FerrumGit, id: RepoId, cx: &mut Context<FerrumGit>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let view = app.state.view(id);
    let Some(info) = view.prs.info.ready().cloned() else {
        return empty("");
    };
    let detail = match &view.prs.detail {
        Load::Ready(d) => d.clone(),
        Load::Loading => return empty("Loading…"),
        Load::Failed(e) => {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .p_8()
                .child(Label::new(e.clone()).color(Color::Error))
                .into_any_element();
        }
        Load::Idle => {
            return empty(match info.repo.kind {
                ForgeKind::GitHub => "Select a pull request",
                ForgeKind::GitLab => "Select a merge request",
            });
        }
    };
    let pr = &detail.pr;
    let number = pr.number;
    let busy = app.state.is_busy(id);
    let open = pr.state == PrState::Open;
    let (state_text, state_color) = state_chip(&detail);
    let web = pr.web_url.clone();
    let tab = view.prs.tab;
    let mergeable_note = match (open, detail.mergeable) {
        (true, Some(false)) => Some(("Not mergeable (conflicts or blocked)", Color::Warning)),
        (true, Some(true)) => Some(("Mergeable", Color::Success)),
        _ => None,
    };

    let tabs: Vec<AnyElement> = PrTab::ALL
        .into_iter()
        .map(|t| {
            let active = t == tab;
            let count = match t {
                PrTab::Conversation => detail.timeline.len(),
                PrTab::Commits => detail.commits.len(),
                PrTab::Checks => detail.checks.len(),
                PrTab::Files => view.prs.changes.ready().map_or(0, |c| c.files.len()),
            };
            div()
                .id(SharedString::from(format!("pr-tab-{t:?}")))
                .px_3()
                .py_1p5()
                .cursor_pointer()
                .border_b_2()
                .border_color(if active {
                    colors.text_accent
                } else {
                    gpui::transparent_black()
                })
                .text_ui_sm(cx)
                .when(!active, |d| {
                    d.text_color(colors.text_muted)
                        .hover(|s| s.text_color(colors.text))
                })
                .child(if count > 0 {
                    format!("{} {count}", t.label())
                } else {
                    t.label().to_owned()
                })
                .on_click(cx.listener(move |app, _, _, cx| app.set_pr_tab(id, t, cx)))
                .into_any_element()
        })
        .collect();

    let body: Vec<AnyElement> = match tab {
        PrTab::Conversation => conversation(&detail, cx),
        PrTab::Commits => commits(&detail, cx),
        PrTab::Files => files(app, id, cx),
        PrTab::Checks => checks(&detail, cx),
    };

    let actions = h_flex()
        .gap_2()
        .flex_wrap()
        .child(
            small_button("pr-open-web", "Open in browser").on_click(move |_, _, _| open_url(&web)),
        )
        .child(
            small_button("pr-checkout", "Check out")
                .disabled(busy || !open)
                .on_click(cx.listener(move |app, _, _, cx| app.checkout_pull_request(id, cx))),
        )
        .child(
            small_button("pr-comment", "Comment…")
                .disabled(busy)
                .on_click(cx.listener(move |app, _, window, cx| {
                    app.open_dialog(
                        Dialog::PrReview {
                            number,
                            event: ReviewEvent::Comment,
                        },
                        window,
                        cx,
                    );
                })),
        )
        .child(
            small_button("pr-review", "Review…")
                .disabled(busy || !open)
                .on_click(cx.listener(move |app, _, window, cx| {
                    app.open_dialog(
                        Dialog::PrReview {
                            number,
                            event: ReviewEvent::Approve,
                        },
                        window,
                        cx,
                    );
                })),
        )
        .when(open && !detail.merge_methods.is_empty(), |d| {
            d.child(
                primary_button("pr-merge", "Merge…")
                    .disabled(busy || pr.draft || detail.mergeable == Some(false))
                    .on_click(cx.listener(move |app, _, window, cx| {
                        app.open_dialog(Dialog::PrMerge { number }, window, cx);
                    })),
            )
        });

    let now = now_secs();
    v_flex()
        .size_full()
        .child(
            v_flex()
                .flex_none()
                .gap_2()
                .px_5()
                .pt_4()
                .pb_3()
                .border_b_1()
                .border_color(colors.border)
                .child(
                    h_flex()
                        .gap_2()
                        .items_start()
                        .child(
                            div().flex_1().min_w_0().child(
                                Label::new(format!(
                                    "{} {}{number}",
                                    pr.title,
                                    info.repo.kind.number_prefix()
                                ))
                                .size(LabelSize::Large)
                                .weight(gpui::FontWeight::SEMIBOLD),
                            ),
                        )
                        .child(chip(state_text, state_color)),
                )
                .child(
                    h_flex().gap_2().flex_wrap().child(
                        Label::new(format!(
                            "{} wants to merge {} into {}{} · updated {}",
                            pr.author,
                            pr.head_branch,
                            pr.base_branch,
                            if pr.from_fork { " (from a fork)" } else { "" },
                            relative_time(unix(&pr.updated_at), now)
                        ))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                    ),
                )
                .child(
                    h_flex()
                        .gap_3()
                        .child(
                            h_flex()
                                .gap_1p5()
                                .child(dot(check_color(pr.checks, cx)))
                                .child(
                                    Label::new(pr.checks.label())
                                        .size(LabelSize::XSmall)
                                        .color(Color::Muted),
                                ),
                        )
                        .when(pr.review != ReviewDecision::None, |d| {
                            d.child(
                                Label::new(pr.review.label())
                                    .size(LabelSize::XSmall)
                                    .color(review_color(pr.review)),
                            )
                        })
                        .children(
                            mergeable_note
                                .map(|(t, c)| Label::new(t).size(LabelSize::XSmall).color(c)),
                        ),
                )
                .child(actions),
        )
        .child(
            h_flex()
                .flex_none()
                .px_3()
                .border_b_1()
                .border_color(colors.border)
                .children(tabs),
        )
        .child(
            v_flex()
                .id("pr-detail-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(body),
        )
        .into_any_element()
}
