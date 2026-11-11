//! The Runs tab: filters, the run list and (when one is open) its detail.

use crate::ci::CiView;
use crate::ci::widgets::{select, state_icon};
use chrono::Utc;
use ferrum_ui::prelude::*;
use ferrum_ui::ui::{Icon, IconSize};
use ferrumci_core::format::{duration, relative};
use ferrumci_core::{EVENTS, StatusFilter, WorkflowRun};

fn filters(app: &CiView, window: &mut Window, cx: &mut Context<CiView>) -> impl IntoElement {
    let f = &app.filter;
    let workflow_label = f
        .workflow_id
        .and_then(|id| app.workflows.iter().find(|w| w.id == id))
        .map_or("All workflows".to_owned(), |w| w.name.clone());
    let mut workflow_options = vec![(SharedString::from("All workflows"), None)];
    workflow_options.extend(
        app.workflows
            .iter()
            .map(|w| (SharedString::from(w.name.clone()), Some(w.id))),
    );
    let mut event_options = vec![(SharedString::from("Any event"), None)];
    event_options.extend(
        EVENTS
            .iter()
            .map(|e| (SharedString::from(*e), Some((*e).to_owned()))),
    );
    let mut status_options = vec![(SharedString::from("Any status"), None)];
    status_options.extend(
        StatusFilter::ALL
            .iter()
            .map(|s| (SharedString::from(s.label()), Some(*s))),
    );
    let local = app.local_branch.clone();
    let dispatch_options: Vec<(SharedString, u64)> = app
        .workflows
        .iter()
        .filter(|w| w.state == "active")
        .map(|w| (SharedString::from(w.name.clone()), w.id))
        .collect();
    h_flex()
        .flex_none()
        .flex_wrap()
        .gap_1()
        .px_2()
        .py_1()
        .border_b_1()
        .border_color(cx.theme().colors().border)
        .child(select(
            "filter-workflow",
            workflow_label,
            workflow_options,
            |this, id, cx| {
                this.filter.workflow_id = id;
                this.reload_runs(cx);
            },
            window,
            cx,
        ))
        .child(
            Icon::new(IconName::GitBranch)
                .size(IconSize::Small)
                .color(Color::Muted),
        )
        .child(div().w(px(150.)).child(app.branch_input.clone()))
        .when_some(local, |el, branch| {
            el.child(
                Button::new("branch-current", "Current")
                    .style(ButtonStyle::Subtle)
                    .tooltip(Tooltip::text(format!("Show runs of {branch}")))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_branch_filter(Some(branch.clone()), cx);
                    })),
            )
        })
        .child(
            Button::new("branch-all", "All")
                .style(ButtonStyle::Subtle)
                .tooltip(Tooltip::text("Runs of every branch"))
                .on_click(cx.listener(|this, _, _, cx| this.set_branch_filter(None, cx))),
        )
        .child(select(
            "filter-event",
            f.event.clone().unwrap_or_else(|| "Any event".into()),
            event_options,
            |this, event, cx| {
                this.filter.event = event;
                this.reload_runs(cx);
            },
            window,
            cx,
        ))
        .child(select(
            "filter-status",
            f.status.map_or("Any status", StatusFilter::label),
            status_options,
            |this, status, cx| {
                this.filter.status = status;
                this.reload_runs(cx);
            },
            window,
            cx,
        ))
        .child(div().flex_1())
        .when(!dispatch_options.is_empty(), |el| {
            el.child(select(
                "run-workflow",
                "Run workflow…",
                dispatch_options,
                |this, id, cx| this.open_dispatch(id, cx),
                window,
                cx,
            ))
        })
        .child(
            IconButton::new("refresh", IconName::RotateCw)
                .tooltip(Tooltip::text("Refresh (⌘R)"))
                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
        )
}

fn run_row(
    run: &WorkflowRun,
    selected: bool,
    watched: bool,
    compact: bool,
    cx: &mut Context<CiView>,
) -> impl IntoElement {
    let now = Utc::now();
    let state = run.state();
    let mut meta = vec![
        format!("{} #{}", run.workflow_name(), run.run_number),
        run.event.clone(),
        run.branch().to_owned(),
    ];
    if !compact {
        meta.push(run.actor().to_owned());
    }
    meta.retain(|m| !m.is_empty());
    let time = if state.is_active() {
        format!("{} {}", state.label(), duration(run.duration_secs(now)))
    } else {
        duration(run.duration_secs(now))
    };
    let clicked = run.clone();
    ListItem::new(SharedString::from(format!("run-{}", run.id)))
        .rounded()
        .toggle_state(selected)
        .on_click(cx.listener(move |this, _, _, cx| this.open_run(clicked.clone(), cx)))
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .py_0p5()
                .child(state_icon(state))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(Label::new(run.title()).truncate())
                        .child(
                            Label::new(meta.join(" · "))
                                .size(LabelSize::Small)
                                .color(Color::Muted)
                                .truncate(),
                        ),
                )
                .when(watched, |el| {
                    el.child(
                        Icon::new(IconName::Bell)
                            .size(IconSize::XSmall)
                            .color(Color::Accent),
                    )
                })
                .child(
                    v_flex()
                        .flex_none()
                        .items_end()
                        .child(Label::new(time).size(LabelSize::Small).color(Color::Muted))
                        .child(
                            Label::new(relative(run.created_at, now))
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        ),
                ),
        )
}

pub fn render(app: &mut CiView, window: &mut Window, cx: &mut Context<CiView>) -> AnyElement {
    let open = app.detail.as_ref().map(|d| d.run.id);
    let compact = open.is_some();
    let rows: Vec<_> = app
        .runs
        .iter()
        .map(|r| {
            run_row(
                r,
                open == Some(r.id),
                app.watcher.is_watching(r.id),
                compact,
                cx,
            )
            .into_any_element()
        })
        .collect();
    let empty = rows.is_empty();
    let summary = if empty {
        String::new()
    } else {
        format!("{} of {} runs", app.runs.len(), app.runs_total)
    };
    let list = v_flex().size_full().child(filters(app, window, cx)).child(
        div()
            .id("runs")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_1()
            .when(empty, |el| {
                let (title, detail) = if app.runs_loading || app.last_refresh.is_none() {
                    ("Loading runs…", "")
                } else {
                    ("No runs", "Nothing matches these filters.")
                };
                el.child(empty_state(title, detail))
            })
            .children(rows)
            .when(!summary.is_empty(), |el| {
                el.child(
                    div().p_2().child(
                        Label::new(summary)
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
                )
            }),
    );
    if open.is_none() {
        return list.into_any_element();
    }
    h_flex()
        .size_full()
        .child(
            div()
                .w(px(420.))
                .flex_none()
                .h_full()
                .border_r_1()
                .border_color(cx.theme().colors().border)
                .child(list),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(crate::ci::detail::render(app, cx)),
        )
        .into_any_element()
}
