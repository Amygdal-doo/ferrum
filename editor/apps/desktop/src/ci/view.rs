//! The CI page's frame: Runs / Pull requests tabs, the body for the
//! current screen, the status line, the dispatch form and notices.

use chrono::{DateTime, Utc};
use ferrum_ui::prelude::*;

use super::{CiView, KEY_CONTEXT, Screen, Tab};

impl CiView {
    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = |id: &'static str, label: &'static str, which: Tab| {
            Button::new(id, label)
                .style(ButtonStyle::Subtle)
                .toggle_state(self.tab == which)
                .on_click(cx.listener(move |this, _, _, cx| this.set_tab(which, cx)))
        };
        toolbar(cx)
            .child(Label::new("CI"))
            .child(tab("tab-runs", "Runs", Tab::Runs))
            .child(tab("tab-prs", "Pull requests", Tab::PullRequests))
    }

    fn render_status(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Utc::now();
        let mut left: Vec<String> = Vec::new();
        if let Some(repo) = self.repo() {
            left.push(repo.path());
        }
        if let Some(b) = self.filter.branch.as_ref().or(self.local_branch.as_ref()) {
            left.push(format!("⎇ {b}"));
        }
        if let Some(origin) = self.token_origin {
            left.push(format!("Token: {}", origin.label()));
        }
        let mut right: Vec<String> = Vec::new();
        if let Some(rate) = self.client.as_ref().and_then(|c| c.actions.rate()) {
            let reset = rate
                .reset_at
                .and_then(|t| DateTime::<Utc>::from_timestamp(t, 0))
                .map(|t| {
                    let local: DateTime<chrono::Local> = t.into();
                    format!(" · resets {}", local.format("%H:%M"))
                })
                .unwrap_or_default();
            right.push(format!("API {} left{reset}", rate.remaining));
        }
        if self.runs_loading {
            right.push("Refreshing…".into());
        } else if let Some(t) = self.last_refresh {
            right.push(format!(
                "Updated {}",
                ferrumci_core::format::relative(t, now)
            ));
        }
        status_bar(cx)
            .child(Label::new(left.join("   ")).size(LabelSize::Small))
            .child(
                h_flex()
                    .gap_3()
                    .when_some(self.error.clone(), |el, e| {
                        el.child(
                            Label::new(e)
                                .size(LabelSize::Small)
                                .color(Color::Error)
                                .truncate(),
                        )
                    })
                    .child(Label::new(right.join("   ")).size(LabelSize::Small)),
            )
    }

    fn render_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match &self.screen {
            Screen::Empty { title, detail } => {
                super::signin::render_empty(title.clone(), detail.clone(), self, cx)
            }
            Screen::GitLab(repo) => empty_state(
                format!("{} is on GitLab", repo.path()),
                "GitLab pipelines are not supported yet; the CI page shows GitHub Actions only.",
            )
            .into_any_element(),
            Screen::Connecting(repo) => empty_state(
                format!("Connecting to {}…", repo.path()),
                "Looking for a GitHub token",
            )
            .into_any_element(),
            Screen::SignIn { .. } => super::signin::render_sign_in(self, cx),
            Screen::Ready => match self.tab {
                Tab::Runs => super::runs::render(self, window, cx),
                Tab::PullRequests => super::pulls::render(self, cx),
            },
        }
    }
}

impl Render for CiView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.frames += 1;
        }
        let body = self.render_body(window, cx);
        let modal = self
            .dispatch
            .is_some()
            .then(|| super::dispatch::render(self, window, cx));
        let colors = cx.theme().colors();
        let (background, text) = (colors.background, colors.text);
        v_flex()
            .relative()
            .size_full()
            .bg(background)
            .text_color(text)
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .child(self.render_tabs(cx))
            .child(div().flex_1().min_h_0().child(body))
            .child(self.render_status(cx))
            .children(modal)
            .when_some(self.notice.as_ref(), |el, notice| {
                el.child(render_notice(
                    notice,
                    cx.listener(|this, _, _, cx| {
                        this.notice = None;
                        cx.notify();
                    }),
                    cx,
                ))
            })
    }
}
