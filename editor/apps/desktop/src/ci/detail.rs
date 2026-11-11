//! Run detail: header with actions, jobs → steps, and the selected job's log.

use crate::ci::CiView;
use crate::ci::widgets::{state_color, state_icon};
use chrono::Utc;
use ferrum_ui::prelude::*;
use ferrum_ui::ui::{Icon, IconSize};
use ferrumci_core::format::{duration, relative, short_sha};
use ferrumci_core::logs::JobLog;
use ferrumci_core::{Job, RunState, WorkflowRun};
use gpui::{ScrollStrategy, UniformListScrollHandle};
use std::collections::HashSet;
use std::sync::Arc;

pub struct RunDetail {
    pub run: WorkflowRun,
    pub jobs: Vec<Job>,
    pub jobs_loaded: bool,
    pub job: Option<u64>,
    pub log: Option<LogView>,
}

impl RunDetail {
    pub fn new(run: WorkflowRun) -> Self {
        Self {
            run,
            jobs: Vec::new(),
            jobs_loaded: false,
            job: None,
            log: None,
        }
    }

    pub fn set_jobs(&mut self, jobs: Vec<Job>) {
        self.jobs = jobs;
        self.jobs_loaded = true;
    }

    /// The first failed job, else the first one.
    pub fn default_job(&self) -> Option<u64> {
        self.jobs
            .iter()
            .find(|j| j.state() == RunState::Failure)
            .or_else(|| self.jobs.first())
            .map(|j| j.id)
    }
}

pub struct LogView {
    pub job_id: u64,
    pub log: Option<Arc<JobLog>>,
    pub loading: bool,
    pub error: Option<String>,
    pub collapsed: HashSet<usize>,
    /// Line indices matching the search.
    pub matches: Vec<usize>,
    pub current: usize,
    pub scroll: UniformListScrollHandle,
}

impl LogView {
    pub fn loading(job_id: u64) -> Self {
        Self {
            job_id,
            log: None,
            loading: true,
            error: None,
            collapsed: HashSet::new(),
            matches: Vec::new(),
            current: 0,
            scroll: UniformListScrollHandle::new(),
        }
    }

    /// Folds quiet groups and scrolls to the first error.
    pub fn loaded(&mut self, log: Arc<JobLog>) {
        self.loading = false;
        self.collapsed = log.default_collapsed();
        let first_error = log.errors().first().copied();
        self.log = Some(log);
        if let Some(line) = first_error {
            self.scroll_to_line(line);
        }
    }

    /// Lines drawn right now.
    pub fn visible(&self) -> Vec<usize> {
        self.log
            .as_ref()
            .map(|l| l.visible(&self.collapsed))
            .unwrap_or_default()
    }

    fn scroll_to_line(&mut self, line: usize) {
        if let Some(log) = &self.log {
            log.reveal(line, &mut self.collapsed);
        }
        if let Some(row) = self.visible().iter().position(|l| *l == line) {
            self.scroll.scroll_to_item(row, ScrollStrategy::Center);
        }
    }

    pub fn search(&mut self, query: &str) {
        self.matches = self
            .log
            .as_ref()
            .map(|l| l.search(query))
            .unwrap_or_default();
        self.current = 0;
        if let Some(log) = &self.log {
            for line in &self.matches {
                log.reveal(*line, &mut self.collapsed);
            }
        }
        if let Some(first) = self.matches.first().copied() {
            self.scroll_to_line(first);
        }
    }

    pub fn step_match(&mut self, step: isize) {
        if self.matches.is_empty() {
            return;
        }
        let n = self.matches.len() as isize;
        self.current = (self.current as isize + step).rem_euclid(n) as usize;
        self.scroll_to_line(self.matches[self.current]);
    }

    pub fn toggle_group(&mut self, group: usize) {
        if !self.collapsed.remove(&group) {
            self.collapsed.insert(group);
        }
    }
}

fn render_header(app: &CiView, cx: &mut Context<CiView>) -> impl IntoElement {
    let Some(d) = app.detail.as_ref() else {
        return div().into_any_element();
    };
    let run = &d.run;
    let state = run.state();
    let now = Utc::now();
    let failed = state == RunState::Failure;
    let watching = app.watcher.is_watching(run.id);
    let url = run.html_url.clone();
    let mut meta = vec![
        format!("{} #{}", run.workflow_name(), run.run_number),
        run.event.clone(),
        run.branch().to_owned(),
        short_sha(&run.head_sha).to_owned(),
        run.actor().to_owned(),
        duration(run.duration_secs(now)),
        relative(run.created_at, now),
    ];
    if run.run_attempt > 1 {
        meta.insert(1, format!("attempt {}", run.run_attempt));
    }
    meta.retain(|m| !m.is_empty());
    v_flex()
        .flex_none()
        .px_3()
        .py_2()
        .gap_1()
        .border_b_1()
        .border_color(cx.theme().colors().border)
        .child(
            h_flex()
                .gap_2()
                .child(
                    IconButton::new("close-run", IconName::Close)
                        .tooltip(Tooltip::text("Back to runs"))
                        .on_click(cx.listener(|this, _, _, cx| this.close_run(cx))),
                )
                .child(state_icon(state))
                .child(
                    Label::new(run.title())
                        .weight(gpui::FontWeight::SEMIBOLD)
                        .truncate(),
                )
                .child(
                    Label::new(state.label())
                        .size(LabelSize::Small)
                        .color(state_color(state)),
                ),
        )
        .child(
            Label::new(meta.join(" · "))
                .size(LabelSize::Small)
                .color(Color::Muted),
        )
        .child(
            h_flex()
                .gap_1()
                .flex_wrap()
                .when(!state.is_active(), |el| {
                    el.child(
                        Button::new("rerun-all", "Re-run all jobs")
                            .start_icon(Icon::new(IconName::RotateCw).size(IconSize::Small))
                            .style(ButtonStyle::Outlined)
                            .on_click(cx.listener(|this, _, _, cx| this.rerun(false, cx))),
                    )
                })
                .when(failed, |el| {
                    el.child(
                        Button::new("rerun-failed", "Re-run failed jobs")
                            .style(ButtonStyle::Outlined)
                            .on_click(cx.listener(|this, _, _, cx| this.rerun(true, cx))),
                    )
                })
                .when(state.is_active(), |el| {
                    el.child(
                        Button::new("cancel-run", "Cancel run")
                            .start_icon(Icon::new(IconName::Stop).size(IconSize::Small))
                            .style(ButtonStyle::Outlined)
                            .on_click(cx.listener(|this, _, _, cx| this.cancel(cx))),
                    )
                })
                .child(
                    Button::new(
                        "watch-run",
                        if watching {
                            "Watching"
                        } else {
                            "Notify when done"
                        },
                    )
                    .start_icon(Icon::new(IconName::Bell).size(IconSize::Small))
                    .style(ButtonStyle::Subtle)
                    .toggle_state(watching)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_watch(cx))),
                )
                .child(
                    Button::new("open-run", "Open on GitHub")
                        .start_icon(Icon::new(IconName::ArrowUpRight).size(IconSize::Small))
                        .style(ButtonStyle::Subtle)
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                ),
        )
        .into_any_element()
}

fn render_jobs(app: &CiView, cx: &mut Context<CiView>) -> impl IntoElement {
    let colors = cx.theme().colors();
    let now = Utc::now();
    let Some(d) = app.detail.as_ref() else {
        return div().into_any_element();
    };
    let mut list = v_flex().gap_0p5();
    if !d.jobs_loaded {
        list = list.child(Label::new("Loading jobs…").color(Color::Muted));
    } else if d.jobs.is_empty() {
        list = list.child(Label::new("No jobs").color(Color::Muted));
    }
    for job in &d.jobs {
        let id = job.id;
        let selected = d.job == Some(id);
        let time = job.duration_secs(now).map(duration).unwrap_or_default();
        list = list.child(
            ListItem::new(SharedString::from(format!("job-{id}")))
                .rounded()
                .toggle_state(selected)
                .on_click(cx.listener(move |this, _, _, cx| this.select_job(id, cx)))
                .child(
                    h_flex()
                        .w_full()
                        .gap_2()
                        .child(state_icon(job.state()))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(Label::new(job.name.clone()).truncate()),
                        )
                        .child(Label::new(time).size(LabelSize::Small).color(Color::Muted)),
                ),
        );
        if selected {
            for step in &job.steps {
                let time = step.duration_secs(now).map(duration).unwrap_or_default();
                list = list.child(
                    h_flex()
                        .pl_6()
                        .pr_2()
                        .gap_2()
                        .child(state_icon(step.state()))
                        .child(
                            div().flex_1().min_w_0().child(
                                Label::new(format!("{}. {}", step.number, step.name))
                                    .size(LabelSize::Small)
                                    .truncate(),
                            ),
                        )
                        .child(Label::new(time).size(LabelSize::XSmall).color(Color::Muted)),
                );
            }
        }
    }
    div()
        .id("jobs")
        .w(px(300.))
        .flex_none()
        .h_full()
        .overflow_y_scroll()
        .p_1()
        .border_r_1()
        .border_color(colors.border)
        .child(list)
        .into_any_element()
}

pub fn render(app: &mut CiView, cx: &mut Context<CiView>) -> AnyElement {
    v_flex()
        .size_full()
        .child(render_header(app, cx))
        .child(
            h_flex()
                .flex_1()
                .min_h_0()
                .items_start()
                .child(render_jobs(app, cx))
                .child(crate::ci::logview::render(app, cx)),
        )
        .into_any_element()
}
