//! Search: thread titles and message text across every recent project
//! (read off the UI thread as you type); a result opens its project and
//! thread.

use std::time::Duration;

use chrono::Utc;
use ferrum_ui::prelude::*;
use ferrum_ui::project;

use super::{field, frame, placeholder, row, two_lines};
use crate::app::FerrumAgentic;
use crate::nav::Page;
use crate::threads;

/// Results shown at most.
const MAX_HITS: usize = 60;

impl FerrumAgentic {
    /// Searches for the query, after a short pause in typing.
    pub(crate) fn run_search(&mut self, cx: &mut Context<Self>) {
        let query = self.pages.search.read(cx).text().to_string();
        let home = self.agent_home.clone();
        let projects = self.recent.clone();
        self.pages.searching = !query.trim().is_empty();
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;
            let hits = cx
                .background_spawn(async move {
                    let index = threads::ThreadIndex::load(&home, &projects);
                    threads::search(&home, &index, &query, MAX_HITS)
                })
                .await;
            this.update(cx, |this, cx| {
                this.pages.hits = hits;
                this.pages.searching = false;
                cx.notify();
            })
            .ok();
        });
        // A newer query replaces the one still running.
        self.pages.tasks.retain(|t| !t.is_ready());
        self.pages.search_task = Some(task);
        cx.notify();
    }

    pub(crate) fn search_page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let query = self.pages.search.read(cx).text().trim().to_string();
        let mut page = frame(
            Page::Search,
            "Thread titles and messages in every project you opened.",
            cx,
        )
        .child(h_flex().child(field(&self.pages.search, cx)));
        if query.is_empty() {
            return page.into_any_element();
        }
        if self.pages.hits.is_empty() {
            page = page.child(placeholder(
                if self.pages.searching {
                    "Searching…"
                } else {
                    "Nothing found."
                },
                cx,
            ));
        }
        let now = Utc::now();
        for hit in &self.pages.hits {
            let entry = &hit.entry;
            let mut detail = format!(
                "{} · {}",
                project::project_name(&entry.project),
                threads::ago(entry.thread.updated, now)
            );
            if !hit.snippet.is_empty() {
                detail = format!("{detail} · {}", hit.snippet);
            }
            let (project, id) = (entry.project.clone(), entry.thread.id.clone());
            page = page.child(
                row(
                    SharedString::from(format!("hit-{}-{}", entry.project.display(), id)),
                    cx,
                )
                .cursor_pointer()
                .child(Icon::new(IconName::MagnifyingGlass).color(Color::Muted))
                .child(two_lines(entry.thread.title.clone(), detail))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_thread(&project, &id, window, cx);
                })),
            );
        }
        page.into_any_element()
    }
}
