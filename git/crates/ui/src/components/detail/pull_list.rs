//! Pull requests page, middle column: filters, onboarding, the open list.

use crate::app::GitView;
use crate::commands::pulls::open_url;
use crate::components::icons::Icon;
use crate::components::now_secs;
use crate::components::widgets::{card, ghost_button, hint, primary_button, small_button};
use crate::state::{Dialog, Load};
use ferrum_ui::components::{
    Clickable, Color, IconSize, Label, LabelCommon, LabelSize, Tooltip, h_flex, v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumgit_application::ForgeInfo;
use ferrumgit_application::forge::{CheckState, PrFilter, PullRequest, ReviewDecision};
use ferrumgit_domain::text::relative_time;
use ferrumgit_domain::{ForgeKind, RepoId};
use gpui::{AnyElement, App, Context, Hsla, SharedString, div, prelude::*, px};

/// RFC 3339 → Unix seconds (0 when unparsable).
#[must_use]
pub fn unix(rfc3339: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(rfc3339).map_or(0, |t| t.timestamp())
}

pub fn check_color(state: CheckState, cx: &App) -> Hsla {
    let s = cx.theme().status();
    match state {
        CheckState::Success => s.success,
        CheckState::Failure => s.error,
        CheckState::Pending => s.warning,
        CheckState::Neutral | CheckState::None => s.ignored,
    }
}

pub fn review_color(r: ReviewDecision) -> Color {
    match r {
        ReviewDecision::Approved => Color::Success,
        ReviewDecision::ChangesRequested => Color::Error,
        ReviewDecision::ReviewRequired => Color::Warning,
        ReviewDecision::None => Color::Muted,
    }
}

pub fn dot(color: Hsla) -> gpui::Div {
    div().flex_none().size(px(8.)).rounded_full().bg(color)
}

/// Small rounded tag.
pub fn chip(text: impl Into<SharedString>, color: Color) -> gpui::Div {
    div()
        .flex_none()
        .px_1p5()
        .rounded_sm()
        .border_1()
        .child(Label::new(text.into()).size(LabelSize::XSmall).color(color))
}

/// Centered message with optional actions.
fn message(text: String, actions: Vec<AnyElement>) -> AnyElement {
    v_flex()
        .flex_1()
        .size_full()
        .items_center()
        .justify_center()
        .gap_3()
        .p_8()
        .child(Label::new(text).color(Color::Muted))
        .child(h_flex().gap_2().children(actions))
        .into_any_element()
}

fn onboarding(id: RepoId, info: &ForgeInfo, cx: &mut Context<GitView>) -> AnyElement {
    let url = info.help.create_url.clone();
    let host = info.repo.host.clone();
    let github = info.repo.kind == ForgeKind::GitHub;
    v_flex()
        .p_4()
        .child(
            card(cx)
                .child(
                    Label::new(format!(
                        "Connect to {} ({})",
                        info.repo.kind.label(),
                        info.repo.host
                    ))
                    .weight(gpui::FontWeight::SEMIBOLD),
                )
                .child(hint(info.help.text.clone()))
                .child(
                    Label::new(format!("Required scope: {}", info.help.scopes))
                        .size(LabelSize::Small)
                        .color(Color::Accent),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .flex_wrap()
                        .child(
                            small_button("pr-onboard-create", "Create a token")
                                .on_click(move |_, _, _| open_url(&url)),
                        )
                        .child(primary_button("pr-onboard-enter", "Enter token…").on_click(
                            cx.listener(move |app, _, window, cx| {
                                app.open_dialog(Dialog::ForgeSettings(id), window, cx);
                            }),
                        ))
                        .when(github, |d| {
                            d.child(
                                small_button("pr-onboard-gh", "Use GitHub CLI login").on_click(
                                    cx.listener(move |app, _, _, cx| {
                                        app.use_gh_cli(id, host.clone(), cx)
                                    }),
                                ),
                            )
                        }),
                ),
        )
        .into_any_element()
}

fn row(
    id: RepoId,
    pr: &PullRequest,
    selected: bool,
    prefix: char,
    now: i64,
    cx: &mut Context<GitView>,
) -> AnyElement {
    let colors = cx.theme().colors().clone();
    let number = pr.number;
    let (selected_fill, selected_text, selected_muted) =
        crate::components::selected_colors(cx.theme());
    let (text, muted) = if selected {
        (Color::Custom(selected_text), Color::Custom(selected_muted))
    } else {
        (Color::Default, Color::Muted)
    };
    let checks = (pr.status_loaded && pr.checks != CheckState::None).then(|| {
        div()
            .id(SharedString::from(format!("pr-checks-{number}")))
            .tooltip(Tooltip::text(pr.checks.label()))
            .child(dot(check_color(pr.checks, cx)))
    });
    let review = (pr.status_loaded && pr.review != ReviewDecision::None).then(|| {
        Label::new(pr.review.label())
            .size(LabelSize::XSmall)
            .color(if selected {
                muted
            } else {
                review_color(pr.review)
            })
    });
    v_flex()
        .id(SharedString::from(format!("pr-row-{number}")))
        .gap_0p5()
        .px_3()
        .py_2()
        .rounded_lg()
        .cursor_pointer()
        .when(selected, |d| d.bg(selected_fill))
        .when(!selected, |d| d.hover(|s| s.bg(colors.element_hover)))
        .on_click(cx.listener(move |app, _, _, cx| app.select_pull_request(id, number, cx)))
        .child(
            h_flex()
                .gap_2()
                .children(checks)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Label::new(pr.title.clone()).color(text).truncate()),
                )
                .when(pr.draft, |d| d.child(chip("Draft", muted))),
        )
        .child(
            h_flex()
                .gap_2()
                .child(
                    div().flex_1().min_w_0().child(
                        Label::new(format!(
                            "{prefix}{number} · {} · {} → {}{}",
                            pr.author,
                            pr.head_branch,
                            pr.base_branch,
                            if pr.from_fork { " (fork)" } else { "" }
                        ))
                        .size(LabelSize::Small)
                        .color(muted)
                        .truncate(),
                    ),
                )
                .child(
                    Label::new(relative_time(unix(&pr.updated_at), now))
                        .size(LabelSize::XSmall)
                        .color(muted),
                ),
        )
        .when(review.is_some() || !pr.labels.is_empty(), |d| {
            d.child(
                h_flex()
                    .gap_1()
                    .children(review)
                    .children(pr.labels.iter().take(4).map(|l| chip(l.clone(), muted))),
            )
        })
        .into_any_element()
}

pub fn pull_list(app: &GitView, id: RepoId, cx: &mut Context<GitView>) -> AnyElement {
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let view = app.state.view(id);
    let prs = view.prs.clone();
    let filter = prs.filter;
    let info = prs.info.ready().cloned();
    let ready = info.as_ref().is_some_and(|i| i.has_token);

    let on_accent = theme.readable_text_on(colors.text_accent);
    let segment = |f: PrFilter, cx: &mut Context<GitView>| {
        let active = f == filter;
        div()
            .id(SharedString::from(format!("pr-filter-{f:?}")))
            .px_2()
            .py_0p5()
            .cursor_pointer()
            .text_xs()
            .when(active, |d| d.bg(colors.text_accent).text_color(on_accent))
            .when(!active, |d| d.hover(|s| s.bg(colors.element_hover)))
            .child(f.label())
            .on_click(cx.listener(move |app, _, _, cx| app.set_pr_filter(id, f, cx)))
            .into_any_element()
    };
    let segments: Vec<AnyElement> = PrFilter::ALL.into_iter().map(|f| segment(f, cx)).collect();
    let web = info.as_ref().map(|i| i.repo.prs_web_url());
    let icon_button = |name: &'static str, icon: Icon, tip: &'static str| {
        div()
            .id(name)
            .p_1()
            .rounded_sm()
            .cursor_pointer()
            .hover(|s| s.bg(colors.element_hover))
            .tooltip(Tooltip::text(tip))
            .child(icon.icon().size(IconSize::Small).color(Color::Muted))
    };
    let header = h_flex()
        .flex_none()
        .h(px(36.))
        .gap_2()
        .px_3()
        .border_b_1()
        .border_color(colors.border)
        .child(
            h_flex()
                .rounded_md()
                .border_1()
                .border_color(colors.border)
                .overflow_hidden()
                .children(segments),
        )
        .child(div().flex_1())
        .when(ready, |d| {
            d.child(
                icon_button(
                    "pr-new",
                    Icon::Plus,
                    "New pull request from the current branch",
                )
                .on_click(
                    cx.listener(|app, _, window, cx| app.open_dialog(Dialog::CreatePr, window, cx)),
                ),
            )
        })
        .child(
            icon_button("pr-refresh", Icon::Refresh, "Refresh").on_click(cx.listener(
                move |app, _, _, cx| {
                    if app
                        .state
                        .view(id)
                        .prs
                        .info
                        .ready()
                        .is_some_and(|i| i.has_token)
                    {
                        app.load_pull_requests(id, cx);
                    } else {
                        app.load_forge_info(id, cx);
                    }
                },
            )),
        )
        .children(web.map(|url| {
            icon_button("pr-web", Icon::Cloud, "Open in browser")
                .on_click(move |_, _, _| open_url(&url))
        }))
        .child(
            icon_button("pr-settings", Icon::Sliders, "Token & forge settings").on_click(
                cx.listener(move |app, _, window, cx| {
                    app.open_dialog(Dialog::ForgeSettings(id), window, cx)
                }),
            ),
        );

    let settings_button = |cx: &mut Context<GitView>| {
        ghost_button("pr-open-settings", "Token & forge settings…")
            .on_click(cx.listener(move |app, _, window, cx| {
                app.open_dialog(Dialog::ForgeSettings(id), window, cx)
            }))
            .into_any_element()
    };
    let body = match (&prs.info, info.as_ref()) {
        (Load::Failed(e), _) => message(e.clone(), vec![settings_button(cx)]),
        (_, Some(i)) if !i.has_token => onboarding(id, i, cx),
        (_, Some(i)) => match &prs.list {
            Load::Idle | Load::Loading => message("Loading…".into(), Vec::new()),
            Load::Failed(e) => message(
                e.clone(),
                vec![
                    small_button("pr-retry", "Retry")
                        .on_click(cx.listener(move |app, _, _, cx| app.load_pull_requests(id, cx)))
                        .into_any_element(),
                    settings_button(cx),
                ],
            ),
            Load::Ready(list) if list.prs.is_empty() => message(
                format!(
                    "No open {}s{}.",
                    i.repo.kind.pr_noun(),
                    match filter {
                        PrFilter::All => String::new(),
                        f => format!(" ({})", f.label().to_lowercase()),
                    }
                ),
                Vec::new(),
            ),
            Load::Ready(list) => {
                let now = now_secs();
                let prefix = i.repo.kind.number_prefix();
                let rows: Vec<AnyElement> = list
                    .prs
                    .iter()
                    .map(|p| row(id, p, prs.selected == Some(p.number), prefix, now, cx))
                    .collect();
                v_flex()
                    .id("pr-list-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_2()
                    .gap_0p5()
                    .children(rows)
                    .when(list.status_skipped, |d| {
                        d.child(div().px_3().py_2().child(hint(
                            "Review and CI status shown for the most recent entries only (saves API rate limit).",
                        )))
                    })
                    .into_any_element()
            }
        },
        _ => message("Detecting the forge…".into(), Vec::new()),
    };
    v_flex()
        .size_full()
        .child(header)
        .child(body)
        .into_any_element()
}
