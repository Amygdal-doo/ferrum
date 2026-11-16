//! Pull request dialogs: forge settings + token onboarding, create, review, merge.

use super::{DialogState, WIDE, modal, width};
use crate::app::GitView;
use crate::commands::pulls::open_url;
use crate::components::widgets::{
    caption, card, confirm_button, danger_button, field, ghost_button, hint, primary_button,
    radio_row, small_button,
};
use ferrum_ui::TextInput;
use ferrum_ui::components::{
    Checkbox, Clickable, Color, Disableable, Label, LabelCommon, LabelSize, ToggleState, h_flex,
    v_flex,
};
use ferrumgit_application::forge::{MergeMethod, NewPullRequest, ReviewEvent};
use ferrumgit_domain::{ForgeKind, RefKind, RepoForge, RepoId, TokenSource};
use gpui::{AnyElement, Context, Entity, SharedString, Window, prelude::*};

pub struct ForgeSettingsForm {
    pub id: RepoId,
    pub kind: Option<ForgeKind>,
    pub token: Entity<TextInput>,
    pub api_url: Entity<TextInput>,
    pub remote: Entity<TextInput>,
}

pub struct CreatePrForm {
    pub title: Entity<TextInput>,
    pub body: Entity<TextInput>,
    pub base: Entity<TextInput>,
    pub head: String,
    pub draft: bool,
    /// An AI draft of the title and description is being written.
    pub drafting: bool,
}

pub struct ReviewForm {
    pub number: u64,
    pub event: ReviewEvent,
    pub body: Entity<TextInput>,
}

pub struct MergeForm {
    pub number: u64,
    pub methods: Vec<MergeMethod>,
    pub method: Option<MergeMethod>,
}

fn text(
    placeholder: &str,
    value: String,
    window: &mut Window,
    cx: &mut Context<GitView>,
) -> Entity<TextInput> {
    let placeholder = placeholder.to_owned();
    cx.new(|cx| {
        TextInput::new(window, cx)
            .placeholder(placeholder)
            .with_text(value)
    })
}

impl ForgeSettingsForm {
    pub fn new(
        app: &mut GitView,
        id: RepoId,
        window: &mut Window,
        cx: &mut Context<GitView>,
    ) -> Self {
        let forge = app.state.entry(id).map(|e| e.forge).unwrap_or_default();
        let token = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Paste a personal access token")
                .masked(true)
        });
        let api_url = text(
            "Detected from the remote (e.g. https://git.example.com/api/v4)",
            forge.api_url,
            window,
            cx,
        );
        let remote = text("origin", forge.remote, window, cx);
        app.watch_input(&token, window, cx);
        Self {
            id,
            kind: forge.kind,
            token,
            api_url,
            remote,
        }
    }
}

impl CreatePrForm {
    pub fn new(app: &GitView, window: &mut Window, cx: &mut Context<GitView>) -> Self {
        let view = app
            .state
            .active
            .map(|id| app.state.view(id))
            .unwrap_or_default();
        let snap = view
            .data
            .as_ref()
            .map(|d| d.snapshot.clone())
            .unwrap_or_default();
        let head = snap.status.branch.head.clone().unwrap_or_default();
        let title = snap
            .status
            .branch
            .oid
            .as_ref()
            .and_then(|oid| snap.commits.iter().find(|c| &c.oid == oid))
            .map(|c| c.summary.clone())
            .unwrap_or_default();
        // Base: the forge remote's default-looking branch.
        let remote = view
            .prs
            .info
            .ready()
            .map_or_else(|| "origin".to_owned(), |i| i.repo.remote.clone());
        let remote_has = |b: &str| {
            snap.refs
                .iter()
                .any(|r| r.is(RefKind::RemoteBranch) && r.name == format!("{remote}/{b}"))
        };
        let base = ["main", "master", "develop", "trunk"]
            .into_iter()
            .find(|b| *b != head && remote_has(b))
            .unwrap_or("main")
            .to_owned();
        let body = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Description (Markdown)")
                .multiline(6)
        });
        Self {
            title: text("Title", title, window, cx),
            body,
            base: text("Base branch", base, window, cx),
            head,
            draft: false,
            drafting: false,
        }
    }

    pub fn build(&self, cx: &Context<GitView>) -> Option<NewPullRequest> {
        let title = self.title.read(cx).text().trim().to_owned();
        let base = self.base.read(cx).text().trim().to_owned();
        (!title.is_empty() && !base.is_empty() && base != self.head).then(|| NewPullRequest {
            title,
            body: self.body.read(cx).text().to_owned(),
            head: self.head.clone(),
            base,
            draft: self.draft,
        })
    }
}

impl ReviewForm {
    pub fn new(
        number: u64,
        event: ReviewEvent,
        window: &mut Window,
        cx: &mut Context<GitView>,
    ) -> Self {
        let body = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Leave a comment (Markdown)")
                .multiline(6)
        });
        Self {
            number,
            event,
            body,
        }
    }

    pub fn valid(&self, cx: &Context<GitView>) -> bool {
        self.event == ReviewEvent::Approve || !self.body.read(cx).text().trim().is_empty()
    }
}

impl MergeForm {
    pub fn new(app: &GitView, number: u64) -> Self {
        let methods = app
            .state
            .active
            .and_then(|id| {
                app.state
                    .view(id)
                    .prs
                    .detail
                    .ready()
                    .map(|d| d.merge_methods.clone())
            })
            .unwrap_or_default();
        Self {
            number,
            method: methods.first().copied(),
            methods,
        }
    }
}

fn form_mut<T>(
    app: &mut GitView,
    f: impl FnOnce(&mut DialogState) -> Option<&mut T>,
) -> Option<&mut T> {
    app.dialog.as_mut().and_then(f)
}

pub fn render_forge_settings(
    app: &GitView,
    form: &ForgeSettingsForm,
    cx: &mut Context<GitView>,
) -> AnyElement {
    let id = form.id;
    let info = app.state.view(id).prs.info.clone();
    let detected = info.ready().cloned();
    let selected_kind = form.kind;
    let kind_rows: Vec<AnyElement> = [
        (
            None,
            "Detect from the remote URL",
            "github.com, gitlab.com, and hosts named like gitlab.* / github.*",
        ),
        (
            Some(ForgeKind::GitHub),
            "GitHub Enterprise",
            "REST API at https://<host>/api/v3",
        ),
        (
            Some(ForgeKind::GitLab),
            "GitLab (self-managed)",
            "REST API at https://<host>/api/v4",
        ),
    ]
    .into_iter()
    .map(|(kind, title, sub)| {
        radio_row(
            SharedString::from(format!("forge-kind-{kind:?}")),
            selected_kind == kind,
            title,
            Some(sub.into()),
            cx.listener(move |app, _, _, cx| {
                if let Some(f) = form_mut(app, |d| match d {
                    DialogState::ForgeSettings(f) => Some(f),
                    _ => None,
                }) {
                    f.kind = kind;
                }
                cx.notify();
            }),
            cx,
        )
        .into_any_element()
    })
    .collect();

    let token_section = match &detected {
        Some(info) => {
            let host = info.repo.host.clone();
            let url = info.help.create_url.clone();
            let is_github = info.repo.kind == ForgeKind::GitHub;
            let status = match (info.has_token, info.token_source) {
                (true, TokenSource::GhCli) => {
                    "Using the GitHub CLI login (`gh auth token`).".to_owned()
                }
                (true, TokenSource::Keychain) => "A token is stored in the OS keychain.".to_owned(),
                (false, _) => "No token yet.".to_owned(),
            };
            let (h1, h2, h3) = (host.clone(), host.clone(), host.clone());
            card(cx)
                .child(Label::new(format!(
                    "{} · {}",
                    info.repo.kind.label(),
                    info.repo.path()
                )))
                .child(hint(info.help.text.clone()))
                .child(
                    Label::new(format!("Required scope: {}", info.help.scopes))
                        .size(LabelSize::Small)
                        .color(Color::Accent),
                )
                .child(hint(status))
                .child(field("Personal access token", form.token.clone()))
                .child(
                    h_flex()
                        .gap_2()
                        .flex_wrap()
                        .child(
                            small_button("forge-create-token", "Create a token in the browser")
                                .on_click(move |_, _, _| open_url(&url)),
                        )
                        .child(primary_button("forge-save-token", "Save token").on_click(
                            cx.listener(move |app, _, window, cx| {
                                let token = match &app.dialog {
                                    Some(DialogState::ForgeSettings(f)) => {
                                        f.token.read(cx).text().to_owned()
                                    }
                                    _ => return,
                                };
                                app.close_dialog(window, cx);
                                app.save_forge_token(id, h1.clone(), token, cx);
                            }),
                        ))
                        .when(is_github, |d| {
                            d.child(
                                small_button("forge-use-gh", "Use GitHub CLI login").on_click(
                                    cx.listener(move |app, _, window, cx| {
                                        app.close_dialog(window, cx);
                                        app.use_gh_cli(id, h2.clone(), cx);
                                    }),
                                ),
                            )
                        })
                        .when(info.has_token, |d| {
                            d.child(danger_button("forge-clear-token", "Remove token").on_click(
                                cx.listener(move |app, _, window, cx| {
                                    app.close_dialog(window, cx);
                                    app.clear_forge_token(id, h3.clone(), cx);
                                }),
                            ))
                        }),
                )
                .into_any_element()
        }
        None => hint(match &info {
            crate::state::Load::Failed(e) => e.clone(),
            _ => "Detecting the forge…".to_owned(),
        })
        .into_any_element(),
    };

    modal("Pull requests", cx)
        .width(width(WIDE))
        .child(v_flex().gap_2().child(caption("Access token")).child(token_section))
        .child(
            v_flex()
                .gap_2()
                .child(caption("Forge for this repository"))
                .child(v_flex().children(kind_rows))
                .child(field("API base URL (optional)", form.api_url.clone()))
                .child(field("Remote (optional, default origin)", form.remote.clone()))
                .child(hint("Change these only for self-hosted servers that are not detected, or to use a different remote (e.g. upstream).")),
        )
        .footer(ghost_button("forge-cancel", "Close").on_click(cx.listener(|app, _, window, cx| app.close_dialog(window, cx))))
        .footer(primary_button("forge-save-settings", "Save settings").on_click(cx.listener(move |app, _, window, cx| {
            let Some(DialogState::ForgeSettings(f)) = &app.dialog else { return };
            let forge = RepoForge {
                remote: f.remote.read(cx).text().to_owned(),
                kind: f.kind,
                api_url: f.api_url.read(cx).text().to_owned(),
            };
            app.close_dialog(window, cx);
            app.save_repo_forge(id, forge, cx);
        })))
        .into_any_element()
}

pub fn render_create(app: &GitView, form: &CreatePrForm, cx: &mut Context<GitView>) -> AnyElement {
    let noun = app
        .state
        .active
        .and_then(|id| {
            app.state
                .view(id)
                .prs
                .info
                .ready()
                .map(|i| i.repo.kind.pr_noun())
        })
        .unwrap_or("pull request");
    let valid = form.build(cx).is_some();
    let draft = form.draft;
    let detached = form.head.is_empty();
    modal(format!("New {noun}"), cx)
        .width(width(WIDE))
        .child(hint(if detached {
            "HEAD is detached; check out the branch to propose first.".to_owned()
        } else {
            format!(
                "From {} · pushed first if it has no upstream or unpushed commits.",
                form.head
            )
        }))
        .child(field("Title", form.title.clone()))
        .child(field("Into (base branch)", form.base.clone()))
        .child(field("Description", form.body.clone()))
        .child(
            h_flex()
                .gap_2()
                .child(
                    small_button(
                        "pr-ai-draft",
                        if form.drafting {
                            "Drafting…"
                        } else {
                            "Draft with AI"
                        },
                    )
                    .disabled(form.drafting || detached)
                    .on_click(cx.listener(|app, _, _, cx| app.draft_pull_request(cx))),
                )
                .child(hint(
                    "Title and description from the branch's commits; edit before creating.",
                )),
        )
        .child(
            Checkbox::new("pr-draft", ToggleState::from(draft))
                .label("Create as draft")
                .on_click(cx.listener(|app, state: &ToggleState, _, cx| {
                    if let Some(DialogState::CreatePr(f)) = &mut app.dialog {
                        f.draft = state.selected();
                    }
                    cx.notify();
                })),
        )
        .footer(
            ghost_button("pr-create-cancel", "Cancel")
                .on_click(cx.listener(|app, _, window, cx| app.close_dialog(window, cx))),
        )
        .footer(
            primary_button("pr-create", format!("Create {noun}"))
                .disabled(!valid || detached)
                .on_click(cx.listener(|app, _, window, cx| app.submit_dialog(window, cx))),
        )
        .into_any_element()
}

pub fn render_review(form: &ReviewForm, cx: &mut Context<GitView>) -> AnyElement {
    let current = form.event;
    let rows: Vec<AnyElement> = ReviewEvent::ALL
        .into_iter()
        .map(|e| {
            let sub = match e {
                ReviewEvent::Comment => "Post a comment without a verdict.",
                ReviewEvent::Approve => "Approve the changes (a comment is optional).",
                ReviewEvent::RequestChanges => "Ask for changes before merging (GitLab: withdraws your approval and posts the comment).",
            };
            radio_row(
                SharedString::from(format!("review-{e:?}")),
                current == e,
                e.label(),
                Some(sub.into()),
                cx.listener(move |app, _, _, cx| {
                    if let Some(DialogState::PrReview(f)) = &mut app.dialog {
                        f.event = e;
                    }
                    cx.notify();
                }),
                cx,
            )
            .into_any_element()
        })
        .collect();
    let valid = form.valid(cx);
    modal(format!("Review #{}", form.number), cx)
        .width(width(WIDE))
        .child(v_flex().children(rows))
        .child(form.body.clone())
        .footer(
            ghost_button("review-cancel", "Cancel")
                .on_click(cx.listener(|app, _, window, cx| app.close_dialog(window, cx))),
        )
        .footer(
            confirm_button(
                "review-submit",
                current.label(),
                current == ReviewEvent::RequestChanges,
            )
            .disabled(!valid)
            .on_click(cx.listener(|app, _, window, cx| app.submit_dialog(window, cx))),
        )
        .into_any_element()
}

pub fn render_merge(form: &MergeForm, cx: &mut Context<GitView>) -> AnyElement {
    let current = form.method;
    let rows: Vec<AnyElement> = form
        .methods
        .iter()
        .copied()
        .map(|m| {
            radio_row(
                SharedString::from(format!("merge-{m:?}")),
                current == Some(m),
                m.label(),
                None,
                cx.listener(move |app, _, _, cx| {
                    if let Some(DialogState::PrMerge(f)) = &mut app.dialog {
                        f.method = Some(m);
                    }
                    cx.notify();
                }),
                cx,
            )
            .into_any_element()
        })
        .collect();
    modal(format!("Merge #{} on the server?", form.number), cx)
        .child(hint(
            "The merge happens on the forge, only if nobody pushed to the branch since this view was loaded. Pull afterwards to get the result locally.",
        ))
        .when(rows.is_empty(), |m| m.child(Label::new("This project allows no merge method for you.").color(Color::Warning)))
        .child(v_flex().children(rows))
        .footer(ghost_button("merge-cancel", "Cancel").on_click(cx.listener(|app, _, window, cx| app.close_dialog(window, cx))))
        .footer(
            confirm_button("merge-confirm", "Merge", true)
                .disabled(current.is_none())
                .on_click(cx.listener(|app, _, window, cx| app.submit_dialog(window, cx))),
        )
        .into_any_element()
}
