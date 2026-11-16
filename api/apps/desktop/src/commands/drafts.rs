//! "Draft tests with agent": build the review, draft each request with the
//! shared agent models, save what the user accepts.

use super::background;
use crate::app::FerrumApi;
use crate::state::Dialog;
use crate::state::TabResponse;
use crate::state::drafts::{DraftRow, DraftState, DraftStatus, merge_tests};
use ferrumapi_application::agent_tests::{DraftInput, TestDrafter};
use ferrumapi_domain::{CollectionId, ItemId, RequestKind, RequestOrigin, TabId};
use gpui::Context;

/// Most requests drafted for one collection / folder.
const MAX_ROWS: usize = 25;

impl FerrumApi {
    /// Opens the review for one tab (its last response is included).
    pub fn draft_tests_for_tab(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        let Some(tab) = self.state.tab(tab_id).cloned() else {
            return;
        };
        let description = tab
            .origin
            .and_then(|o| self.state.collection(o.collection)?.find_request(o.item))
            .map(|s| s.description.clone())
            .unwrap_or_default();
        let response = match self.state.responses.get(&tab_id) {
            Some(TabResponse::Done(r)) => Some((**r).clone()),
            _ => None,
        };
        let row = DraftRow {
            name: crate::state::tabs::display_title(&tab),
            tab: Some(tab_id),
            origin: None,
            input: DraftInput {
                name: crate::state::tabs::display_title(&tab),
                request: tab.request,
                description,
                response,
            },
            status: DraftStatus::Waiting,
            selected: true,
        };
        self.open_drafts(vec![row], cx);
    }

    /// Opens the review for every request of a collection or folder.
    pub fn draft_tests_for_saved(
        &mut self,
        cid: CollectionId,
        folder: Option<ItemId>,
        cx: &mut Context<Self>,
    ) {
        let Some(c) = self.state.collection(cid) else {
            return;
        };
        let rows: Vec<DraftRow> = c
            .requests_under(folder)
            .into_iter()
            .filter(|s| s.request.kind != RequestKind::WebSocket)
            .take(MAX_ROWS)
            .map(|s| DraftRow {
                name: s.name.clone(),
                tab: None,
                origin: Some(RequestOrigin {
                    collection: cid,
                    item: s.id,
                }),
                input: DraftInput {
                    name: s.name.clone(),
                    request: s.request.clone(),
                    description: s.description.clone(),
                    response: None,
                },
                status: DraftStatus::Waiting,
                selected: true,
            })
            .collect();
        if rows.is_empty() {
            self.notify("There are no requests to draft tests for.", true, cx);
            return;
        }
        self.open_drafts(rows, cx);
    }

    fn open_drafts(&mut self, rows: Vec<DraftRow>, cx: &mut Context<Self>) {
        let drafter = self.drafter();
        let models = drafter.models();
        let model = drafter.default_model();
        self.state.drafts = Some(DraftState::new(rows, models, model));
        self.state.dialog = Some(Dialog::DraftTests);
        cx.notify();
    }

    /// The shared models, loaded on first use.
    fn drafter(&mut self) -> TestDrafter {
        self.drafter.get_or_insert_with(TestDrafter::load).clone()
    }

    /// Drafts every row in turn with the chosen model.
    pub fn start_drafting(&mut self, cx: &mut Context<Self>) {
        let drafter = self.drafter();
        let Some(state) = self.state.drafts.as_mut() else {
            return;
        };
        let Some(model) = state.model.clone() else {
            return;
        };
        state.running = true;
        state.run += 1;
        let run = state.run;
        for row in &mut state.rows {
            row.status = DraftStatus::Waiting;
        }
        let inputs: Vec<DraftInput> = state.rows.iter().map(|r| r.input.clone()).collect();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            for (i, input) in inputs.into_iter().enumerate() {
                let current = this.update(cx, |this, cx| {
                    let Some(s) = this.state.drafts.as_mut().filter(|s| s.run == run) else {
                        return false;
                    };
                    if let Some(row) = s.rows.get_mut(i) {
                        row.status = DraftStatus::Drafting;
                    }
                    cx.notify();
                    true
                });
                if !matches!(current, Ok(true)) {
                    return;
                }
                let (d, m) = (drafter.clone(), model.clone());
                let result = background(&services, async move { d.draft(&m, &input).await })
                    .await
                    .and_then(|r| r);
                let _ = this.update(cx, |this, cx| {
                    if let Some(row) = this
                        .state
                        .drafts
                        .as_mut()
                        .filter(|s| s.run == run)
                        .and_then(|s| s.rows.get_mut(i))
                    {
                        row.status = match result {
                            Ok(d) => DraftStatus::Ready {
                                script: d.script,
                                problems: d.problems,
                            },
                            Err(e) => DraftStatus::Failed(e),
                        };
                        cx.notify();
                    }
                });
            }
            let _ = this.update(cx, |this, cx| {
                if let Some(s) = this.state.drafts.as_mut().filter(|s| s.run == run) {
                    s.running = false;
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Saves the ticked, ready scripts into their tabs / saved requests.
    pub fn save_drafts(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.state.drafts.take() else {
            return;
        };
        let mut saved = 0;
        for row in state.accepted() {
            let DraftStatus::Ready { script, .. } = &row.status else {
                continue;
            };
            if let Some(tab) = row.tab {
                self.state.edit_request(tab, |r| {
                    r.scripts.tests = merge_tests(&r.scripts.tests, script)
                });
                saved += 1;
            }
            if let Some(o) = row.origin {
                self.mutate_collection(o.collection, cx, |c| {
                    if let Some(s) = c.find_request_mut(o.item) {
                        s.request.scripts.tests = merge_tests(&s.request.scripts.tests, script);
                    }
                });
                // An open, unedited tab of the request shows the new tests too.
                for tab in self
                    .state
                    .tabs
                    .iter_mut()
                    .filter(|t| t.origin == Some(o) && !t.dirty)
                {
                    tab.request.scripts.tests = merge_tests(&tab.request.scripts.tests, script);
                }
                self.state.touch();
                saved += 1;
            }
        }
        self.state.dialog = None;
        self.notify(
            format!("Saved drafted tests for {saved} request(s)."),
            false,
            cx,
        );
    }
}
