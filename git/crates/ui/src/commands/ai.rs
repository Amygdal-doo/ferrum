//! AI drafts: a commit message from the staged diff, a pull request title
//! and description from the branch's commits. They only fill the inputs;
//! committing or creating the pull request stays with the user.

use crate::app::GitView;
use crate::components::actions::Action;
use crate::components::dialogs::DialogState;
use ferrumgit_domain::RepoId;
use gpui::{Context, Pixels, Point};

impl GitView {
    /// Drafts the commit message of repository `id` into the commit box.
    pub fn draft_commit_message(&mut self, id: RepoId, cx: &mut Context<Self>) {
        let Some(entry) = self.state.entry(id) else {
            return;
        };
        if self.state.view(id).drafting {
            return;
        }
        self.state.update_view(id, |v| v.drafting = true);
        let services = self.services.clone();
        let model = self.ai_model.clone();
        self.spawn_run(
            cx,
            async move { services.draft_commit_message(&entry.path, model).await },
            move |app, result, cx| {
                app.state.update_view(id, |v| v.drafting = false);
                match result {
                    Ok(draft) => {
                        app.state.update_view(id, |v| {
                            v.summary = draft.title;
                            v.description = draft.body;
                            v.draft_rev += 1;
                        });
                        app.notify("Draft ready: edit it, then commit", false, cx);
                    }
                    Err(e) => app.notify(e, true, cx),
                }
            },
        );
        cx.notify();
    }

    /// Drafts the open "New pull request" dialog's title and description.
    pub fn draft_pull_request(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.state.active_entry() else {
            return;
        };
        let Some(DialogState::CreatePr(form)) = &mut self.dialog else {
            return;
        };
        if form.drafting {
            return;
        }
        let base = form.base.read(cx).text().trim().to_owned();
        if base.is_empty() {
            self.notify("Enter the base branch first", true, cx);
            return;
        }
        form.drafting = true;
        let remote = self
            .state
            .view(entry.id)
            .prs
            .info
            .ready()
            .map_or_else(|| "origin".to_owned(), |i| i.repo.remote.clone());
        let services = self.services.clone();
        let model = self.ai_model.clone();
        self.spawn_run(
            cx,
            async move {
                services
                    .draft_pull_request(&entry.path, &remote, &base, model)
                    .await
            },
            move |app, result, cx| {
                let Some(DialogState::CreatePr(form)) = &mut app.dialog else {
                    return;
                };
                form.drafting = false;
                match result {
                    Ok(draft) => {
                        let (title, body) = (form.title.clone(), form.body.clone());
                        title.update(cx, |input, cx| input.set_text(draft.title, cx));
                        body.update(cx, |input, cx| input.set_text(draft.body, cx));
                        app.notify("Draft ready: edit it, then create", false, cx);
                    }
                    Err(e) => app.notify(e, true, cx),
                }
            },
        );
        cx.notify();
    }

    /// Lists the models drafts can use in a menu at `position`.
    pub fn open_ai_model_menu(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.ai_models().await },
            move |app, result, cx| match result {
                Ok(models) if models.is_empty() => app.notify(
                    "No AI model is set up: add a provider key in the agent's Models and API keys",
                    true,
                    cx,
                ),
                Ok(models) => {
                    let current = app.ai_model.clone();
                    let items = models
                        .into_iter()
                        .map(|m| Action::SetAiModel {
                            selected: current.as_deref() == Some(m.key.as_str()),
                            label: format!("{} · {}", m.label, m.group),
                            key: m.key,
                        })
                        .collect();
                    app.open_menu(position, items, cx);
                }
                Err(e) => app.notify(e, true, cx),
            },
        );
    }
}
